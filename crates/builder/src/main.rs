//! Builds the dictionary pack from the open sources in data/raw, and queries it natively.
//!
//!   builder build [raw_dir] [pack_dir] [public_dir]
//!   builder query <pack_dir> <text> [pair]
//!   builder bench <pack_dir>

mod ingest;

use ingest::*;
use lex_core::{norm, Engine, Reader};
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::fs::{self, File};
use std::hash::{Hash, Hasher};
use std::io::Write;
use std::os::unix::fs::FileExt;
use std::path::{Path, PathBuf};
use std::time::Instant;

#[derive(Clone, Copy, PartialEq)]
enum Lang {
    De,
    En,
    Ru,
}

fn langs(pair: u8) -> (Lang, Lang) {
    match pair {
        DE_EN => (Lang::De, Lang::En),
        DE_RU => (Lang::De, Lang::Ru),
        _ => (Lang::En, Lang::Ru),
    }
}

/// Drops {..} [..] (..) <..> annotations and Ding's /abbr./ notes.
fn strip_annotations(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let mut depth = 0i32;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '{' | '[' | '(' | '<' => depth += 1,
            '}' | ']' | ')' | '>' => depth = (depth - 1).max(0),
            '/' if depth == 0 && (i == 0 || chars[i - 1] == ' ') => {
                let close = (i + 1..chars.len().min(i + 40)).find(|&j| {
                    chars[j] == '/' && (j + 1 == chars.len() || matches!(chars[j + 1], ' ' | ';' | ','))
                });
                match close {
                    Some(j) => i = j,
                    None => out.push(' '),
                }
            }
            _ if depth == 0 => out.push(c),
            _ => {}
        }
        i += 1;
    }
    out
}

/// Headword alternatives of one side, as normalised tokens with function words removed.
fn alts(text: &str, comma: bool) -> Vec<Vec<String>> {
    let stripped = strip_annotations(text);
    let mut out = Vec::new();
    for part in stripped.split(|c| c == ';' || (comma && c == ',')) {
        let all = norm::tokenize(part);
        if all.is_empty() {
            continue;
        }
        let kept: Vec<String> = all.iter().filter(|t| !norm::is_stop(t)).cloned().collect();
        let toks = if kept.is_empty() { all } else { kept };
        if toks.iter().all(|t| t.len() <= 48) && toks.len() <= 12 && !out.contains(&toks) {
            out.push(toks);
        }
        if out.len() == 12 {
            break;
        }
    }
    out
}

/// Byte ranges of the alternatives of one side, split at separators outside annotations.
fn parts(text: &str, comma: bool) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let (mut depth, mut start) = (0i32, 0);
    for (i, c) in text.char_indices() {
        match c {
            '{' | '[' | '(' | '<' => depth += 1,
            '}' | ']' | ')' | '>' => depth = (depth - 1).max(0),
            ';' | ',' if depth == 0 && (c == ';' || comma) => {
                out.push((start, i));
                start = i + 1;
            }
            _ => {}
        }
    }
    out.push((start, text.len()));
    out
}

/// Gender bits (1 = m, 2 = f, 4 = n) of a "{m}" or "{m,f}" tag in the part, if it has exactly one.
fn gender_bits(part: &str) -> Option<u8> {
    let mut found = None;
    let mut rest = part;
    while let Some(i) = rest.find('{') {
        let j = i + rest[i..].find('}')?;
        let mut bits = 0u8;
        for g in rest[i + 1..j].split(',') {
            bits |= match g.trim() {
                "m" => 1,
                "f" => 2,
                "n" => 4,
                _ => {
                    bits = 0;
                    break;
                }
            };
        }
        if bits != 0 {
            if found.is_some() {
                return None;
            }
            found = Some(bits);
        }
        rest = &rest[j + 1..];
    }
    found
}

/// Adds the gender to German nouns that lack it (WikDict and OpenRussian carry none), taken from the
/// entries of the other sources that spell the noun the same way. Returns the number of sides changed.
fn fill_genders(entries: &mut [Entry]) -> usize {
    let key = |part: &str| strip_annotations(part).split_whitespace().collect::<Vec<_>>().join(" ");
    let german = |e: &Entry| e.pair == DE_EN || e.pair == DE_RU; // German is side a of both pairs
    let mut known: HashMap<String, [u32; 3]> = HashMap::new();
    for e in entries.iter().filter(|e| german(e)) {
        for (s, t) in parts(&e.a, e.comma.0) {
            let part = &e.a[s..t];
            let Some(bits) = gender_bits(part) else { continue };
            let k = key(part);
            if k.is_empty() {
                continue;
            }
            let counts = known.entry(k).or_default();
            for g in 0..3 {
                counts[g] += u32::from(bits >> g & 1);
            }
        }
    }
    let mut changed = 0;
    for e in entries.iter_mut().filter(|e| german(e)) {
        let mut out = String::new();
        let mut at = 0;
        for (s, t) in parts(&e.a, e.comma.0) {
            let part = &e.a[s..t];
            if part.contains('{') {
                continue;
            }
            let Some(counts) = known.get(&key(part)) else { continue };
            // A gender seen only in a stray entry is more likely a source error than a second gender.
            let most = *counts.iter().max().unwrap();
            let tag: Vec<&str> = (0..3).filter(|&g| counts[g] * 5 >= most).map(|g| ["m", "f", "n"][g]).collect();
            // The tag goes right after the word, before trailing notes: "Katze {f} (Peitsche)".
            let (mut depth, mut end) = (0i32, 0);
            for (i, c) in part.char_indices() {
                match c {
                    '{' | '[' | '(' | '<' => depth += 1,
                    '}' | ']' | ')' | '>' => depth = (depth - 1).max(0),
                    _ if depth == 0 && !c.is_whitespace() => end = i + c.len_utf8(),
                    _ => {}
                }
            }
            out.push_str(&e.a[at..s + end]);
            out.push_str(&format!(" {{{}}}", tag.join(",")));
            at = s + end;
        }
        if at > 0 {
            out.push_str(&e.a[at..]);
            e.a = out;
            changed += 1;
        }
    }
    changed
}

struct Entry {
    pair: u8,
    a: String,
    b: String,
    src: u8,
    comma: (bool, bool),
    ord: u8,
    sort: String,
}

fn hash_combo(pair: u8, a: &[String], b: &[String]) -> u64 {
    let mut h = DefaultHasher::new();
    (pair, a, b).hash(&mut h);
    h.finish()
}

fn fscore(freq: &HashMap<String, u32>, tok: &str) -> f32 {
    match freq.get(tok) {
        Some(&r) => 30.0 * (1.0 - (r as f32).ln() / 50001f32.ln()).max(0.0),
        None => 0.0,
    }
}

fn write_file(dir: &Path, name: &str, data: &[u8]) {
    fs::write(dir.join(name), data).unwrap_or_else(|e| panic!("write {name}: {e}"));
}

fn build(raw: &Path, pack: &Path, public: &Path) {
    let t0 = Instant::now();
    let k = raw.join("kaikki");
    let w = raw.join("wikdict");
    let o = raw.join("openrussian");
    // Order matters: when two sources carry the same translation, the earlier row is kept.
    let jobs: Vec<(&str, Box<dyn FnOnce() -> Batch + Send>)> = vec![
        ("ding", Box::new(|| ding(&raw.join("ding/de-en.txt")))),
        ("enwikt-de", Box::new(|| kaikki(&k.join("enwikt-de.jsonl"), DE_EN, true, SRC_WIKT_EN, true, false))),
        ("enwikt-ru", Box::new(|| kaikki(&k.join("enwikt-ru.jsonl"), EN_RU, false, SRC_WIKT_EN, false, false))),
        ("ruwikt-de", Box::new(|| kaikki(&k.join("ruwikt-de.jsonl"), DE_RU, true, SRC_WIKT_RU, true, false))),
        ("ruwikt-en", Box::new(|| kaikki(&k.join("ruwikt-en.jsonl"), EN_RU, true, SRC_WIKT_RU, false, true))),
        ("openrussian-nouns", Box::new(|| openrussian(&o.join("nouns.csv"), "sg_nom"))),
        ("openrussian-verbs", Box::new(|| openrussian(&o.join("verbs.csv"), "imperative_sg"))),
        ("openrussian-adjectives", Box::new(|| openrussian(&o.join("adjectives.csv"), "comparative"))),
        ("openrussian-others", Box::new(|| openrussian(&o.join("others.csv"), "-"))),
        ("wikdict de-en", Box::new(|| wikdict(&w.join("de-en.tsv"), DE_EN, false))),
        ("wikdict en-de", Box::new(|| wikdict(&w.join("en-de.tsv"), DE_EN, true))),
        ("wikdict de-ru", Box::new(|| wikdict(&w.join("de-ru.tsv"), DE_RU, false))),
        ("wikdict ru-de", Box::new(|| wikdict(&w.join("ru-de.tsv"), DE_RU, true))),
        ("wikdict en-ru", Box::new(|| wikdict(&w.join("en-ru.tsv"), EN_RU, false))),
        ("wikdict ru-en", Box::new(|| wikdict(&w.join("ru-en.tsv"), EN_RU, true))),
    ];
    let batches: Vec<(&str, Batch)> = std::thread::scope(|s| {
        let handles: Vec<_> = jobs.into_iter().map(|(name, job)| (name, s.spawn(job))).collect();
        handles.into_iter().map(|(name, h)| (name, h.join().expect("ingest failed"))).collect()
    });
    let freq = [Lang::De, Lang::En, Lang::Ru].map(|l| {
        ingest::freq(&raw.join("freq").join(match l {
            Lang::De => "de.txt",
            Lang::En => "en.txt",
            Lang::Ru => "ru.txt",
        }))
    });
    let fq = |l: Lang| &freq[l as usize];
    eprintln!("ingested in {:.1}s", t0.elapsed().as_secs_f32());

    // ---- merge ----
    let mut entries: Vec<Entry> = Vec::new();
    let mut seen: HashMap<u64, u32> = HashMap::new();
    let mut forms_raw: Vec<(String, String)> = Vec::new();
    for (name, batch) in batches {
        let (mut added, mut merged) = (0, 0);
        for r in batch.rows {
            let (aa, ab) = (alts(&r.a, r.comma.0), alts(&r.b, r.comma.1));
            if aa.is_empty() || ab.is_empty() {
                continue;
            }
            let mut combos = Vec::new();
            'c: for x in &aa {
                for y in &ab {
                    combos.push(hash_combo(r.pair, x, y));
                    if combos.len() == 24 {
                        break 'c;
                    }
                }
            }
            if combos.iter().all(|h| seen.contains_key(h)) {
                entries[seen[&combos[0]] as usize].src |= r.src;
                merged += 1;
                continue;
            }
            let idx = entries.len() as u32;
            for h in combos {
                seen.entry(h).or_insert(idx);
            }
            entries.push(Entry { pair: r.pair, sort: aa[0].join(" "), a: r.a, b: r.b, src: r.src, comma: r.comma, ord: r.ord });
            added += 1;
        }
        eprintln!("{name:24} +{added:7} entries, {merged:7} merged, {:7} forms", batch.forms.len());
        forms_raw.extend(batch.forms);
    }
    drop(seen);
    let filled = fill_genders(&mut entries);
    eprintln!("genders: {filled} German sides completed");
    entries.sort_by(|x, y| (x.pair, &x.sort, &x.a, &x.b).cmp(&(y.pair, &y.sort, &y.a, &y.b)));
    assert!((entries.len() as u32) < lex_core::MAX_ENTRIES, "too many entries for the id width");
    let mut pair_start = [0u32; lex_core::N_PAIRS + 1];
    for p in 0..=lex_core::N_PAIRS {
        pair_start[p] = entries.partition_point(|e| (e.pair as usize) < p) as u32;
    }
    eprintln!("entries: {} (DE-EN {}, DE-RU {}, EN-RU {})", entries.len(),
        pair_start[1], pair_start[2] - pair_start[1], pair_start[3] - pair_start[2]);

    // ---- index ----
    let mut key_ids: HashMap<String, u32> = HashMap::new();
    let mut lists: Vec<Vec<u32>> = Vec::new();
    for (id, e) in entries.iter().enumerate() {
        let (la, lb) = langs(e.pair);
        let sides = [(alts(&e.a, e.comma.0), la), (alts(&e.b, e.comma.1), lb)];
        let nsrc = e.src.count_ones().min(4) as f32;
        for side in 0..2 {
            let (mine, lang) = &sides[side];
            let (other, olang) = &sides[1 - side];
            let f_other = other
                .iter()
                .filter(|a| a.len() == 1)
                .map(|a| fscore(fq(*olang), &a[0]))
                .fold(0.0, f32::max);
            let base = 8.0 + 8.0 * (nsrc - 1.0) + 1.2 * f_other
                - 4.0 * (mine.len().min(4) as f32 - 1.0)
                - 2.0 * e.ord.min(6) as f32
                - if e.src == SRC_WIKDICT { 6.0 } else { 0.0 };
            for (ai, alt) in mine.iter().enumerate() {
                let later = if ai > 0 { 5.0 } else { 0.0 };
                for (ti, tok) in alt.iter().enumerate() {
                    let rank = if alt.len() == 1 {
                        let q = base + 0.5 * fscore(fq(*lang), tok) - later;
                        lex_core::RANK_HEAD + q.clamp(0.0, 85.0) as u8
                    } else {
                        let q = (base - later - 6.0 * (alt.len() as f32 - 2.0)).clamp(0.0, 79.0) as u8;
                        if ti == 0 { lex_core::RANK_FIRST + q } else { lex_core::RANK_OTHER + q }
                    };
                    let kid = *key_ids.entry(tok.clone()).or_insert_with(|| {
                        lists.push(Vec::new());
                        lists.len() as u32 - 1
                    });
                    lists[kid as usize].push(lex_core::pack_posting(rank, side as u8, id as u32));
                }
            }
        }
    }
    let mut keys: Vec<(String, u32)> = key_ids.into_iter().collect();
    keys.sort();
    let mut postings: Vec<u8> = Vec::new();
    let mut fst = fst::MapBuilder::memory();
    let mut n_postings = 0usize;
    for (key, kid) in &keys {
        let list = &mut lists[*kid as usize];
        // one posting per (entry, side): keep the best rank
        list.sort_unstable_by(|x, y| (x & 0x00FF_FFFF).cmp(&(y & 0x00FF_FFFF)).then(y.cmp(x)));
        list.dedup_by_key(|p| *p & 0x00FF_FFFF);
        list.sort_unstable_by(|x, y| y.cmp(x));
        let off = postings.len() as u64;
        postings.extend((list.len() as u32).to_le_bytes());
        for p in list.iter() {
            postings.extend(p.to_le_bytes());
        }
        n_postings += list.len();
        fst.insert(key, (((list[0] >> 24) as u64) << lex_core::OFF_BITS) | off).unwrap();
    }
    let fst_bytes = fst.into_inner().unwrap();
    eprintln!("index: {} keys, {} postings", keys.len(), n_postings);

    // ---- inflected forms ----
    let key_set: std::collections::HashSet<&str> = keys.iter().map(|(k, _)| k.as_str()).collect();
    let mut forms: HashMap<String, Vec<String>> = HashMap::new();
    for (form, lemma) in forms_raw {
        let (f, l) = (norm::tokenize(&form), norm::tokenize(&lemma));
        if f.len() != 1 || l.len() != 1 || f[0] == l[0] || l[0].len() > 60 || !key_set.contains(l[0].as_str()) {
            continue;
        }
        let v = forms.entry(f.into_iter().next().unwrap()).or_default();
        if v.len() < 4 && !v.contains(&l[0]) {
            v.push(l.into_iter().next().unwrap());
        }
    }
    let mut form_keys: Vec<(String, Vec<String>)> = forms.into_iter().collect();
    form_keys.sort();
    let mut forms_bin: Vec<u8> = Vec::new();
    let mut rec_at: HashMap<Vec<String>, u64> = HashMap::new();
    let mut forms_fst = fst::MapBuilder::memory();
    for (form, lemmas) in &form_keys {
        let off = *rec_at.entry(lemmas.clone()).or_insert_with(|| {
            let off = forms_bin.len() as u64;
            forms_bin.push(lemmas.len() as u8);
            for l in lemmas {
                forms_bin.push(l.len() as u8);
                forms_bin.extend(l.as_bytes());
            }
            off
        });
        forms_fst.insert(form, off).unwrap();
    }
    let forms_fst_bytes = forms_fst.into_inner().unwrap();
    eprintln!("forms: {}", form_keys.len());

    // ---- entry blocks ----
    let mut entries_bin: Vec<u8> = Vec::new();
    let mut blocks: Vec<(u32, u32)> = Vec::new();
    let mut cur: Vec<u8> = Vec::new();
    let mut cur_first = 0u32;
    let mut raw_text = 0usize;
    for (id, e) in entries.iter().enumerate() {
        if cur.is_empty() {
            cur_first = id as u32;
        }
        lex_core::write_varint(&mut cur, e.a.len());
        cur.extend(e.a.as_bytes());
        lex_core::write_varint(&mut cur, e.b.len());
        cur.extend(e.b.as_bytes());
        cur.push(e.src);
        if cur.len() >= 6000 || id + 1 == entries.len() {
            blocks.push((cur_first, entries_bin.len() as u32));
            raw_text += cur.len();
            entries_bin.extend(miniz_oxide::deflate::compress_to_vec(&cur, 8));
            cur.clear();
        }
    }
    let mut blocks_idx: Vec<u8> = Vec::new();
    blocks_idx.extend(lex_core::BLOCKS_MAGIC.to_le_bytes());
    blocks_idx.extend(lex_core::FORMAT_VERSION.to_le_bytes());
    for p in pair_start {
        blocks_idx.extend(p.to_le_bytes());
    }
    blocks_idx.extend((blocks.len() as u32).to_le_bytes());
    for (first, off) in blocks.iter().chain([&(entries.len() as u32, entries_bin.len() as u32)]) {
        blocks_idx.extend(first.to_le_bytes());
        blocks_idx.extend(off.to_le_bytes());
    }
    eprintln!("entry text: {} MB raw -> {} MB", raw_text >> 20, entries_bin.len() >> 20);

    // ---- write pack (raw files for native use, gzipped parts for the app) ----
    let files: Vec<(&str, &[u8])> = vec![
        ("index.fst", &fst_bytes),
        ("forms.fst", &forms_fst_bytes),
        ("blocks.idx", &blocks_idx),
        ("postings.bin", &postings),
        ("entries.bin", &entries_bin),
        ("forms.bin", &forms_bin),
    ];
    let _ = fs::remove_dir_all(pack);
    fs::create_dir_all(pack).unwrap();
    let mut h = DefaultHasher::new();
    for (name, data) in &files {
        write_file(pack, name, data);
        (name, data.len(), &data[..data.len().min(1 << 16)]).hash(&mut h);
    }
    let version = format!("{:012x}", h.finish() & 0xFFFF_FFFF_FFFF);
    let _ = fs::remove_dir_all(public);
    let out = public.join(&version);
    fs::create_dir_all(&out).unwrap();
    const PART: usize = 4 << 20;
    let mut total_gz = 0usize;
    let mut total_raw = 0usize;
    let mut jfiles = Vec::new();
    for (name, data) in &files {
        let mut parts = Vec::new();
        for (i, chunk) in data.chunks(PART).enumerate() {
            let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::new(9));
            gz.write_all(chunk).unwrap();
            let bytes = gz.finish().unwrap();
            let pname = format!("{name}.{i:03}.gz");
            write_file(&out, &pname, &bytes);
            total_gz += bytes.len();
            parts.push(serde_json::json!({ "name": pname, "size": bytes.len(), "raw": chunk.len() }));
        }
        total_raw += data.len();
        jfiles.push(serde_json::json!({ "name": name, "size": data.len(), "parts": parts }));
    }
    let manifest = serde_json::json!({
        "version": version,
        "format": lex_core::FORMAT_VERSION,
        "entries": entries.len(),
        "pairs": [pair_start[1], pair_start[2] - pair_start[1], pair_start[3] - pair_start[2]],
        "download": total_gz,
        "installed": total_raw,
        "files": jfiles,
    });
    write_file(&out, "manifest.json", serde_json::to_string(&manifest).unwrap().as_bytes());
    write_file(public, "latest.json", format!("{{\"version\":\"{version}\"}}").as_bytes());
    for (name, data) in &files {
        eprintln!("  {name:14} {:8.1} MB", data.len() as f32 / 1048576.0);
    }
    eprintln!(
        "pack {version}: {:.1} MB download, {:.1} MB installed, built in {:.1}s",
        total_gz as f32 / 1048576.0,
        total_raw as f32 / 1048576.0,
        t0.elapsed().as_secs_f32()
    );
}

struct Files([File; 3]);

impl Reader for Files {
    fn read_at(&self, file: u8, off: u64, buf: &mut [u8]) -> usize {
        let mut done = 0;
        while done < buf.len() {
            match self.0[file as usize].read_at(&mut buf[done..], off + done as u64) {
                Ok(0) | Err(_) => break,
                Ok(n) => done += n,
            }
        }
        done
    }
}

fn open(pack: &Path) -> Engine<Files> {
    let rd = |n: &str| fs::read(pack.join(n)).unwrap_or_else(|e| panic!("{n}: {e}"));
    let f = |n: &str| File::open(pack.join(n)).unwrap_or_else(|e| panic!("{n}: {e}"));
    Engine::new(rd("index.fst"), rd("forms.fst"), &rd("blocks.idx"), Files([f("postings.bin"), f("entries.bin"), f("forms.bin")]))
        .expect("open pack")
}

const PAIR_NAMES: [[&str; 2]; 3] = [["DE", "EN"], ["DE", "RU"], ["EN", "RU"]];

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let arg = |i: usize, d: &str| PathBuf::from(args.get(i).map(String::as_str).unwrap_or(d));
    match args.first().map(String::as_str) {
        Some("build") => build(&arg(1, "data/raw"), &arg(2, "data/pack"), &arg(3, "app/public/packs")),
        Some("query") if args.len() >= 3 => {
            let eng = open(&arg(1, "data/pack"));
            let filter = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(-1);
            let t = Instant::now();
            let groups = eng.search(&args[2], filter, 8);
            let dt = t.elapsed();
            for g in &groups {
                let n = PAIR_NAMES[g.pair as usize];
                println!("── {} → {}", n[g.side as usize], n[1 - g.side as usize]);
                for h in &g.hits {
                    let e = eng.entry(h.id).unwrap();
                    let (l, r) = if g.side == 0 { (&e.a, &e.b) } else { (&e.b, &e.a) };
                    println!("  {:3} t{} s{:02x}  {}  ⇔  {}", h.score, h.tier, e.src, l, r);
                }
            }
            println!("({:.2} ms)", dt.as_secs_f64() * 1e3);
        }
        Some("bench") => {
            let eng = open(&arg(1, "data/pack"));
            let words = ["g", "ge", "geh", "gehen", "ging", "haus", "house", "д", "до", "дом", "идти", "шёл",
                "in betracht ziehen", "to go", "nach hau", "hauss", "recieve", "собака", "weltanschauung", "s"];
            for w in words {
                let mut typed = String::new();
                let _ = &mut typed;
                let t = Instant::now();
                let mut n = 0;
                for _ in 0..20 {
                    n = eng.search_json(w, -1, 30).len();
                }
                println!("{:22} {:8.3} ms  ({} bytes json)", w, t.elapsed().as_secs_f64() * 1e3 / 20.0, n);
            }
        }
        _ => eprintln!("usage: builder build [raw] [pack] [public] | query <pack> <text> [pair] | bench <pack>"),
    }
}
