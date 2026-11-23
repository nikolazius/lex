//! Shared dictionary core: text normalisation, pack format and search.
//! Compiled natively for the builder and to WASM for the app, so both sides
//! normalise text identically.

pub mod norm;

use fst::automaton::{Automaton, Levenshtein, Str};
use fst::{IntoStreamer, Map, Streamer};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

// ---- pack format -----------------------------------------------------------

pub const FILE_POSTINGS: u8 = 0;
pub const FILE_ENTRIES: u8 = 1;
pub const FILE_FORMS: u8 = 2;

pub const BLOCKS_MAGIC: u32 = 0x4C454F42; // "LEOB"
pub const FORMAT_VERSION: u32 = 1;
pub const N_PAIRS: usize = 3;

/// Posting: rank (8 bits) | side (1 bit) | entry id (23 bits). Lists are stored rank-descending.
pub const ID_BITS: u32 = 23;
pub const ID_MASK: u32 = (1 << ID_BITS) - 1;
pub const MAX_ENTRIES: u32 = ID_MASK;

/// Rank bands by how the token relates to the headword alternative it came from.
pub const RANK_HEAD: u8 = 170; // the alternative is exactly this word
pub const RANK_FIRST: u8 = 90; // first word of a multi-word alternative
pub const RANK_OTHER: u8 = 10; // any other word of a phrase

/// FST value: best rank in the list (top 24 bits) | byte offset into the postings file (40 bits).
pub const OFF_BITS: u32 = 40;
pub const OFF_MASK: u64 = (1 << OFF_BITS) - 1;

#[inline]
pub fn pack_posting(rank: u8, side: u8, id: u32) -> u32 {
    ((rank as u32) << 24) | ((side as u32 & 1) << ID_BITS) | (id & ID_MASK)
}

pub trait Reader {
    /// Fill `buf` from `file` at `off`. Returns the number of bytes read.
    fn read_at(&self, file: u8, off: u64, buf: &mut [u8]) -> usize;
}

// ---- search ----------------------------------------------------------------

pub const TIER_EXACT: u8 = 0;
pub const TIER_FORM: u8 = 1;
pub const TIER_PREFIX: u8 = 2;
pub const TIER_FUZZY: u8 = 3;

#[derive(Clone, Copy)]
struct Cand {
    score: u16,
    tier: u8,
}

pub struct Hit {
    pub id: u32,
    pub score: u16,
    pub tier: u8,
}

pub struct Group {
    pub pair: u8,
    pub side: u8,
    pub hits: Vec<Hit>,
}

pub struct Entry {
    pub a: String,
    pub b: String,
    pub src: u8,
}

pub struct Engine<R: Reader> {
    fst: Map<Vec<u8>>,
    forms: Option<Map<Vec<u8>>>,
    blocks: Vec<(u32, u32)>, // (first entry id, byte offset); last item is the end sentinel
    pair_start: [u32; N_PAIRS + 1],
    reader: R,
    cache: RefCell<HashMap<u32, Rc<Vec<u8>>>>,
}

fn u32_at(b: &[u8], i: usize) -> u32 {
    u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]])
}

impl<R: Reader> Engine<R> {
    pub fn new(fst: Vec<u8>, forms_fst: Vec<u8>, blocks: &[u8], reader: R) -> Result<Self, String> {
        if blocks.len() < 8 + 4 * (N_PAIRS + 1) + 4 || u32_at(blocks, 0) != BLOCKS_MAGIC {
            return Err("bad block index".into());
        }
        if u32_at(blocks, 4) != FORMAT_VERSION {
            return Err("unsupported pack version".into());
        }
        let mut pair_start = [0u32; N_PAIRS + 1];
        for (i, p) in pair_start.iter_mut().enumerate() {
            *p = u32_at(blocks, 8 + 4 * i);
        }
        let base = 8 + 4 * (N_PAIRS + 1);
        let n = u32_at(blocks, base) as usize;
        if blocks.len() < base + 4 + 8 * (n + 1) {
            return Err("truncated block index".into());
        }
        let list = (0..=n)
            .map(|i| (u32_at(blocks, base + 4 + 8 * i), u32_at(blocks, base + 8 + 8 * i)))
            .collect();
        Ok(Engine {
            fst: Map::new(fst).map_err(|e| e.to_string())?,
            forms: if forms_fst.is_empty() { None } else { Some(Map::new(forms_fst).map_err(|e| e.to_string())?) },
            blocks: list,
            pair_start,
            reader,
            cache: RefCell::new(HashMap::new()),
        })
    }

    pub fn entry_count(&self) -> u32 {
        self.pair_start[N_PAIRS]
    }

    fn pair_of(&self, id: u32) -> u8 {
        let mut p = 0;
        while p + 1 < N_PAIRS && id >= self.pair_start[p + 1] {
            p += 1;
        }
        p as u8
    }

    /// First `max` postings (best rank first) of the list an FST value points to.
    fn postings(&self, value: u64, max: usize) -> Vec<u32> {
        let off = value & OFF_MASK;
        let mut head = [0u8; 4];
        if self.reader.read_at(FILE_POSTINGS, off, &mut head) != 4 {
            return Vec::new();
        }
        let n = (u32::from_le_bytes(head) as usize).min(max);
        let mut buf = vec![0u8; n * 4];
        let got = self.reader.read_at(FILE_POSTINGS, off + 4, &mut buf) / 4;
        (0..got).map(|i| u32_at(&buf, i * 4)).collect()
    }

    fn lemmas_of(&self, token: &str) -> Vec<String> {
        let Some(off) = self.forms.as_ref().and_then(|f| f.get(token)) else { return Vec::new() };
        let mut buf = [0u8; 512];
        let got = self.reader.read_at(FILE_FORMS, off, &mut buf);
        let mut out = Vec::new();
        if got == 0 {
            return out;
        }
        let n = buf[0] as usize;
        let mut i = 1;
        for _ in 0..n {
            if i >= got {
                break;
            }
            let len = buf[i] as usize;
            if i + 1 + len > got {
                break;
            }
            if let Ok(s) = std::str::from_utf8(&buf[i + 1..i + 1 + len]) {
                out.push(s.to_string());
            }
            i += 1 + len;
        }
        out
    }

    /// Best keys matching an automaton, by list quality minus a length penalty.
    fn top_keys<A: Automaton>(&self, aut: A, skip: &str, base_len: usize, keep: usize) -> Vec<(u64, u16)> {
        let mut best: Vec<(i32, u64, u16)> = Vec::with_capacity(keep + 1);
        let mut floor = i32::MIN;
        let mut stream = self.fst.search(aut).into_stream();
        while let Some((key, value)) = stream.next() {
            if key == skip.as_bytes() {
                continue;
            }
            let klen = std::str::from_utf8(key).map(|s| s.chars().count()).unwrap_or(key.len());
            let penalty = (3 * klen.saturating_sub(base_len)).min(45) as i32;
            let q = (value >> OFF_BITS) as i32 - penalty;
            if best.len() == keep && q <= floor {
                continue;
            }
            best.push((q, value, penalty as u16));
            if best.len() > keep {
                best.sort_unstable_by(|x, y| y.0.cmp(&x.0));
                best.truncate(keep);
                floor = best[keep - 1].0;
            }
        }
        best.into_iter().map(|(_, v, p)| (v, p)).collect()
    }

    fn add(cands: &mut HashMap<u32, Cand>, key: u32, score: u16, tier: u8) {
        let c = cands.entry(key).or_insert(Cand { score: 0, tier });
        if score > c.score {
            *c = Cand { score, tier };
        }
    }

    fn single(&self, t: &str, cands: &mut HashMap<u32, Cand>, accept: &dyn Fn(u32) -> bool) {
        let nchars = t.chars().count();
        if let Some(v) = self.fst.get(t) {
            for p in self.postings(v, 8000) {
                if accept(p & ID_MASK) {
                    Self::add(cands, p & 0x00FF_FFFF, 256 + (p >> 24) as u16, TIER_EXACT);
                }
            }
        }
        for lemma in self.lemmas_of(t) {
            if let Some(v) = self.fst.get(&lemma) {
                for p in self.postings(v, 400) {
                    let rank = (p >> 24) as u16;
                    if rank < RANK_HEAD as u16 {
                        break;
                    }
                    if accept(p & ID_MASK) {
                        Self::add(cands, p & 0x00FF_FFFF, 170 + rank, TIER_FORM);
                    }
                }
            }
        }
        let keep = if nchars <= 2 { 24 } else { 48 };
        for (v, penalty) in self.top_keys(Str::new(t).starts_with(), t, nchars, keep) {
            for p in self.postings(v, 48) {
                if accept(p & ID_MASK) {
                    let s = (100 + (p >> 24) as u16).saturating_sub(penalty);
                    Self::add(cands, p & 0x00FF_FFFF, s, TIER_PREFIX);
                }
            }
        }
        if cands.len() < 6 && nchars >= 4 {
            // Two edits for longer words so swapped letters ("recieve") are caught.
            let lev = if nchars >= 6 { Levenshtein::new(t, 2).or_else(|_| Levenshtein::new(t, 1)) } else { Levenshtein::new(t, 1) };
            if let Ok(lev) = lev {
                let q: Vec<char> = t.chars().collect();
                let mut keys: Vec<(u64, usize)> = Vec::new();
                let mut stream = self.fst.search(&lev).into_stream();
                while let Some((key, value)) = stream.next() {
                    if let Ok(k) = std::str::from_utf8(key) {
                        let d = osa_distance(&q, &k.chars().collect::<Vec<_>>());
                        if d <= 2 {
                            keys.push((value, d));
                        }
                    }
                }
                keys.sort_unstable_by_key(|&(v, d)| (d, std::cmp::Reverse(v >> OFF_BITS)));
                keys.truncate(24);
                for (v, d) in keys {
                    for p in self.postings(v, 24) {
                        if accept(p & ID_MASK) {
                            let s = (90 + (p >> 24) as u16 / 2).saturating_sub(40 * d as u16);
                            Self::add(cands, p & 0x00FF_FFFF, s, TIER_FUZZY);
                        }
                    }
                }
            }
        }
    }

    fn multi(&self, tokens: &[String], cands: &mut HashMap<u32, Cand>, accept: &dyn Fn(u32) -> bool) {
        // Every token but the last must match exactly; the last may still be a prefix.
        let mut sets: Vec<HashMap<u32, u16>> = Vec::new();
        for (i, t) in tokens.iter().enumerate() {
            let mut set: HashMap<u32, u16> = HashMap::new();
            let mut take = |v: u64, max: usize, penalty: u16| {
                for p in self.postings(v, max) {
                    if accept(p & ID_MASK) {
                        let r = ((p >> 24) as u16).saturating_sub(penalty);
                        let e = set.entry(p & 0x00FF_FFFF).or_insert(0);
                        *e = (*e).max(r);
                    }
                }
            };
            if let Some(v) = self.fst.get(t) {
                take(v, 40000, 0);
            }
            if i + 1 == tokens.len() {
                let n = t.chars().count();
                for (v, penalty) in self.top_keys(Str::new(t.as_str()).starts_with(), t, n, 40) {
                    take(v, 4000, penalty);
                }
            }
            sets.push(set);
        }
        sets.sort_by_key(|s| s.len());
        let (first, rest) = sets.split_first().unwrap();
        'outer: for (&key, &r0) in first {
            let mut sum = r0 as u32;
            for s in rest {
                match s.get(&key) {
                    Some(&r) => sum += r as u32,
                    None => continue 'outer,
                }
            }
            let avg = (sum / tokens.len() as u32) as u16;
            Self::add(cands, key, 256 + avg, TIER_EXACT);
        }
    }

    /// `filter`: pair index, or negative for all pairs. `limit` applies per result group.
    pub fn search(&self, query: &str, filter: i32, limit: usize) -> Vec<Group> {
        let all = norm::tokenize(query);
        if all.is_empty() {
            return Vec::new();
        }
        let mut tokens: Vec<String> = all.iter().filter(|t| !norm::is_stop(t)).cloned().collect();
        if tokens.is_empty() {
            tokens = all;
        }
        let accept = |id: u32| filter < 0 || self.pair_of(id) as i32 == filter;
        let mut cands: HashMap<u32, Cand> = HashMap::new();
        if tokens.len() == 1 {
            self.single(&tokens[0], &mut cands, &accept);
        } else {
            self.multi(&tokens, &mut cands, &accept);
            if cands.is_empty() {
                // No phrase contains all words: fall back to the longest one.
                let t = tokens.iter().max_by_key(|t| t.chars().count()).unwrap().clone();
                self.single(&t, &mut cands, &accept);
            }
        }

        let mut groups: Vec<Group> = Vec::new();
        for (key, c) in cands {
            let id = key & ID_MASK;
            let side = ((key >> ID_BITS) & 1) as u8;
            let pair = self.pair_of(id);
            let g = match groups.iter_mut().position(|g| g.pair == pair && g.side == side) {
                Some(i) => &mut groups[i],
                None => {
                    groups.push(Group { pair, side, hits: Vec::new() });
                    groups.last_mut().unwrap()
                }
            };
            g.hits.push(Hit { id, score: c.score, tier: c.tier });
        }
        for g in &mut groups {
            g.hits.sort_unstable_by(|x, y| y.score.cmp(&x.score).then(x.id.cmp(&y.id)));
            g.hits.truncate(limit);
        }
        groups.sort_by(|x, y| {
            y.hits[0].score.cmp(&x.hits[0].score).then((x.pair, x.side).cmp(&(y.pair, y.side)))
        });
        groups
    }

    fn block(&self, idx: usize) -> Option<Rc<Vec<u8>>> {
        if let Some(b) = self.cache.borrow().get(&(idx as u32)) {
            return Some(b.clone());
        }
        let (start, end) = (self.blocks[idx].1 as u64, self.blocks[idx + 1].1 as u64);
        let mut raw = vec![0u8; (end - start) as usize];
        if self.reader.read_at(FILE_ENTRIES, start, &mut raw) != raw.len() {
            return None;
        }
        let data = Rc::new(miniz_oxide::inflate::decompress_to_vec(&raw).ok()?);
        let mut cache = self.cache.borrow_mut();
        if cache.len() >= 256 {
            cache.clear();
        }
        cache.insert(idx as u32, data.clone());
        Some(data)
    }

    pub fn entry(&self, id: u32) -> Option<Entry> {
        if id >= self.entry_count() {
            return None;
        }
        let idx = self.blocks.partition_point(|b| b.0 <= id) - 1;
        let data = self.block(idx)?;
        let mut pos = 0usize;
        let mut cur = self.blocks[idx].0;
        loop {
            let (alen, n) = read_varint(&data, pos)?;
            let a0 = pos + n;
            let (blen, n2) = read_varint(&data, a0 + alen)?;
            let b0 = a0 + alen + n2;
            let end = b0 + blen + 1;
            if end > data.len() {
                return None;
            }
            if cur == id {
                return Some(Entry {
                    a: String::from_utf8_lossy(&data[a0..a0 + alen]).into_owned(),
                    b: String::from_utf8_lossy(&data[b0..b0 + blen]).into_owned(),
                    src: data[end - 1],
                });
            }
            cur += 1;
            pos = end;
        }
    }

    /// Compact JSON: {"groups":[{"pair":0,"side":0,"items":[[id,"a","b",src,tier],...]}]}
    pub fn search_json(&self, query: &str, filter: i32, limit: usize) -> String {
        let groups = self.search(query, filter, limit);
        let mut out = String::with_capacity(8192);
        out.push_str("{\"groups\":[");
        for (gi, g) in groups.iter().enumerate() {
            if gi > 0 {
                out.push(',');
            }
            out.push_str(&format!("{{\"pair\":{},\"side\":{},\"items\":[", g.pair, g.side));
            let mut first = true;
            for h in &g.hits {
                let Some(e) = self.entry(h.id) else { continue };
                if !first {
                    out.push(',');
                }
                first = false;
                out.push_str(&format!("[{},", h.id));
                json_str(&mut out, &e.a);
                out.push(',');
                json_str(&mut out, &e.b);
                out.push_str(&format!(",{},{}]", e.src, h.tier));
            }
            out.push_str("]}");
        }
        out.push_str("]}");
        out
    }
}

/// Edit distance where swapping two adjacent letters counts as one edit.
fn osa_distance(a: &[char], b: &[char]) -> usize {
    let (n, m) = (a.len(), b.len());
    let mut d = vec![vec![0usize; m + 1]; n + 1];
    for i in 0..=n {
        d[i][0] = i;
    }
    for j in 0..=m {
        d[0][j] = j;
    }
    for i in 1..=n {
        for j in 1..=m {
            let cost = (a[i - 1] != b[j - 1]) as usize;
            d[i][j] = (d[i - 1][j] + 1).min(d[i][j - 1] + 1).min(d[i - 1][j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                d[i][j] = d[i][j].min(d[i - 2][j - 2] + 1);
            }
        }
    }
    d[n][m]
}

pub fn write_varint(out: &mut Vec<u8>, mut v: usize) {
    while v >= 0x80 {
        out.push((v as u8 & 0x7F) | 0x80);
        v >>= 7;
    }
    out.push(v as u8);
}

fn read_varint(b: &[u8], mut pos: usize) -> Option<(usize, usize)> {
    let (mut v, mut shift, start) = (0usize, 0, pos);
    loop {
        let byte = *b.get(pos)?;
        pos += 1;
        v |= ((byte & 0x7F) as usize) << shift;
        if byte < 0x80 {
            return Some((v, pos - start));
        }
        shift += 7;
        if shift > 28 {
            return None;
        }
    }
}

fn json_str(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            c if (c as u32) < 0x20 => out.push(' '),
            c => out.push(c),
        }
    }
    out.push('"');
}
