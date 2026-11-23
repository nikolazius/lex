//! Readers for each open source. Everything is reduced to `Raw` translation rows
//! plus (inflected form -> lemma) pairs.

use serde::Deserialize;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

pub const SRC_DING: u8 = 1;
pub const SRC_WIKT_EN: u8 = 2;
pub const SRC_WIKT_RU: u8 = 4;
pub const SRC_OPENRUSSIAN: u8 = 8;
pub const SRC_WIKDICT: u8 = 16;

pub const DE_EN: u8 = 0;
pub const DE_RU: u8 = 1;
pub const EN_RU: u8 = 2;

pub struct Raw {
    pub pair: u8,
    pub a: String,
    pub b: String,
    pub src: u8,
    /// Whether commas separate alternatives on side a / b (true for Wiktionary-style glosses).
    pub comma: (bool, bool),
    /// Position among the translations of its headword (0 = primary sense).
    pub ord: u8,
}

#[derive(Default)]
pub struct Batch {
    pub rows: Vec<Raw>,
    pub forms: Vec<(String, String)>,
}

fn lines(path: &Path) -> impl Iterator<Item = String> {
    let f = File::open(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    BufReader::with_capacity(1 << 20, f).lines().map_while(Result::ok)
}

fn clean(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

// ---- Ding ------------------------------------------------------------------

const DING_TAGS: &[&str] = &[
    "m", "f", "n", "pl", "vi", "vt", "vr", "vti", "vtr", "adj", "adv", "prp", "conj", "pron", "num", "art", "interj",
    "ppron", "no pl", "sing", "v", "prep",
];

pub fn ding(path: &Path) -> Batch {
    let mut out = Batch::default();
    for line in lines(path) {
        if line.starts_with('#') {
            continue;
        }
        let Some((de, en)) = line.split_once(" :: ") else { continue };
        let dp: Vec<&str> = de.split('|').map(str::trim).collect();
        let ep: Vec<&str> = en.split('|').map(str::trim).collect();
        let mut push = |a: &str, b: &str| {
            if a.is_empty() || b.is_empty() {
                return;
            }
            // English irregular forms are listed in braces: "to go {went; gone}".
            if let (Some(i), Some(j)) = (b.find('{'), b.find('}')) {
                if i < j {
                    let lemma = b[..i].trim().trim_start_matches("to ").to_string();
                    for f in b[i + 1..j].split([';', ',']) {
                        let f = f.trim();
                        if !f.is_empty() && !DING_TAGS.contains(&f) && !f.contains('.') {
                            out.forms.push((f.to_string(), lemma.clone()));
                        }
                    }
                }
            }
            out.rows.push(Raw { pair: DE_EN, a: clean(a), b: clean(b), src: SRC_DING, comma: (false, false), ord: 0 });
        };
        if dp.len() == ep.len() {
            for (a, b) in dp.iter().zip(&ep) {
                push(a, b);
            }
        } else {
            push(de.trim(), en.trim());
        }
    }
    out
}

// ---- WikDict (TSV dumped from sqlite by scripts/fetch.sh) --------------------

pub fn wikdict(path: &Path, pair: u8, swap: bool) -> Batch {
    let mut out = Batch::default();
    for line in lines(path) {
        let mut cols = line.split('\t');
        let (Some(word), Some(list)) = (cols.next(), cols.next()) else { continue };
        for (i, t) in list.split(" | ").enumerate() {
            // Drop bare affixes and trailing plural endings ("Klammeraffe -n") left over from Wiktionary markup.
            let t = match t.rfind(" -") {
                Some(i) if t.len() - i <= 6 => &t[..i],
                _ => t,
            };
            let (w, t) = (clean(word), clean(t));
            if w.is_empty() || t.is_empty() || t.chars().count() > 80 || w.starts_with('-') || t.starts_with('-') {
                continue;
            }
            let (a, b) = if swap { (t, w) } else { (w, t) };
            out.rows.push(Raw { pair, a, b, src: SRC_WIKDICT, comma: (false, false), ord: i.min(9) as u8 });
        }
    }
    out
}

// ---- OpenRussian -----------------------------------------------------------

fn stress(s: &str) -> String {
    s.replace('\'', "\u{301}")
}

pub fn openrussian(path: &Path, first_form_col: &str) -> Batch {
    let mut out = Batch::default();
    let mut it = lines(path);
    let header: Vec<String> = it.next().unwrap_or_default().split('\t').map(str::to_string).collect();
    let col = |name: &str| header.iter().position(|h| h == name);
    let (Some(bare), Some(acc), Some(en), Some(de)) =
        (col("bare"), col("accented"), col("translations_en"), col("translations_de"))
    else {
        return out;
    };
    let forms_from = col(first_form_col).unwrap_or(usize::MAX);
    for line in it {
        let c: Vec<&str> = line.split('\t').collect();
        if c.len() <= de || c[bare].is_empty() {
            continue;
        }
        let ru = stress(if c[acc].is_empty() { c[bare] } else { c[acc] });
        for (text, pair) in [(c[en], EN_RU), (c[de], DE_RU)] {
            for (i, sense) in text.split(';').enumerate() {
                let sense = clean(sense);
                if !sense.is_empty() && sense.chars().count() <= 120 {
                    out.rows.push(Raw { pair, a: sense, b: ru.clone(), src: SRC_OPENRUSSIAN, comma: (true, false), ord: i.min(9) as u8 });
                }
            }
        }
        for cell in c.iter().skip(forms_from) {
            for f in cell.split([',', ';']) {
                let f = f.trim().replace('\'', "");
                if !f.is_empty() {
                    out.forms.push((f, c[bare].to_string()));
                }
            }
        }
    }
    out
}

// ---- kaikki.org Wiktionary extracts ------------------------------------------

#[derive(Deserialize)]
struct KRef {
    #[serde(default)]
    word: String,
}

#[derive(Deserialize)]
struct KSense {
    #[serde(default)]
    glosses: Vec<String>,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    form_of: Vec<KRef>,
    #[serde(default)]
    alt_of: Vec<KRef>,
}

#[derive(Deserialize)]
struct KForm {
    #[serde(default)]
    form: String,
    #[serde(default)]
    tags: Vec<String>,
}

#[derive(Deserialize)]
struct KEntry {
    #[serde(default)]
    word: String,
    #[serde(default)]
    pos: String,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    senses: Vec<KSense>,
    #[serde(default)]
    forms: Vec<KForm>,
}

const SKIP_FORM_TAGS: &[&str] = &[
    "romanization", "table-tags", "inflection-template", "class", "canonical", "diminutive", "augmentative",
    "feminine", "masculine", "alternative", "obsolete", "archaic", "dated",
];

fn gender(tags: &[String]) -> &'static str {
    let has = |t: &str| tags.iter().any(|x| x == t);
    match (has("masculine"), has("feminine"), has("neuter")) {
        (true, false, false) => " {m}",
        (false, true, false) => " {f}",
        (false, false, true) => " {n}",
        _ => "",
    }
}

/// Glosses that describe the word instead of translating it.
const META_GLOSS: &[&str] = &[
    "used ", "alternative ", "obsolete ", "archaic ", "dated ", "misspelling", "nonstandard", "abbreviation of",
    "initialism of", "acronym of", "clipping of", "clipped compound", "pre-1918", "pre-reform", "a transliteration",
    "a surname", "a male given name", "a female given name", "a diminutive of", "romanization", "synonym of",
    "adjective of", "relational adjective", "superseded ", "informal spelling", "standard spelling", "eye dialect",
    "contraction of", "short for ", "ellipsis of", "expresses ", "indicates ", "marks ", "denotes ",
    "вариант ", "то же, что", "форма ", "сокр. от", "сокращение от", "см. ", "действие по знач", "свойство по знач",
    "связанный, соотносящийся", "название буквы",
];

/// Moves leading usage labels ("разг., перен. ...") into an annotation so they are not indexed.
fn bracket_labels(g: &str) -> String {
    let mut rest = g;
    let mut labels: Vec<&str> = Vec::new();
    loop {
        let Some(dot) = rest.find('.') else { break };
        let word = &rest[..dot];
        let n = word.chars().count();
        if n == 0 || n > 8 || !word.chars().all(|c| c.is_alphabetic() || c == '-') {
            break;
        }
        let after = rest[dot + 1..].trim_start_matches([',', ' ']);
        if after.is_empty() {
            break;
        }
        labels.push(&rest[..=dot]);
        rest = after;
    }
    if labels.is_empty() { g.to_string() } else { format!("{} [{}]", rest, labels.join(" ")) }
}

fn good_gloss(g: &str, russian: bool) -> Option<String> {
    let g = clean(g.trim_end_matches(['.', ';', ':']));
    let n = g.chars().count();
    if n == 0 || n > 90 || g.contains('◆') || g.contains('=') || g.contains(" — ") || g.contains(" see ") {
        return None;
    }
    let low = g.to_lowercase();
    if META_GLOSS.iter().any(|m| low.starts_with(m)) || low.contains(" form of ") || low.contains(" spelling of ") {
        return None;
    }
    Some(if russian { bracket_labels(&g) } else { g })
}

/// `headword_is_a`: whether the Wiktionary headword is side a of the pair (the gloss is the other side).
pub fn kaikki(path: &Path, pair: u8, headword_is_a: bool, src: u8, german_headword: bool, strict_forms: bool) -> Batch {
    let mut out = Batch::default();
    for line in lines(path) {
        let Ok(e) = serde_json::from_str::<KEntry>(&line) else { continue };
        if e.word.is_empty() || e.word.chars().count() > 60 {
            continue;
        }
        // Stressed spelling for Russian headwords.
        let shown = e
            .forms
            .iter()
            .find(|f| f.tags.iter().any(|t| t == "canonical") && !f.form.is_empty())
            .map(|f| f.form.clone())
            .unwrap_or_else(|| e.word.clone());
        let mut kept = 0;
        let mut lemma_entry = false;
        for s in &e.senses {
            if !s.form_of.is_empty() || s.tags.iter().any(|t| t == "form-of") {
                for r in &s.form_of {
                    if !r.word.is_empty() {
                        out.forms.push((e.word.clone(), r.word.clone()));
                    }
                }
                continue;
            }
            if !s.alt_of.is_empty() || s.tags.iter().any(|t| t == "alt-of") {
                for r in &s.alt_of {
                    if !r.word.is_empty() {
                        out.forms.push((e.word.clone(), r.word.clone()));
                    }
                }
                continue;
            }
            lemma_entry = true;
            if kept >= 8 {
                continue;
            }
            let Some(gloss) = s.glosses.last().and_then(|g| good_gloss(g, src == SRC_WIKT_RU)) else { continue };
            let head = if german_headword && e.pos == "noun" {
                let g = gender(&s.tags);
                format!("{}{}", shown, if g.is_empty() { gender(&e.tags) } else { g })
            } else {
                shown.clone()
            };
            let (a, b, comma) = if headword_is_a { (head, gloss, (false, true)) } else { (gloss, head, (true, false)) };
            out.rows.push(Raw { pair, a, b, src, comma, ord: kept });
            kept += 1;
        }
        if lemma_entry {
            for f in &e.forms {
                if !f.form.chars().next().is_some_and(char::is_alphabetic) || f.tags.iter().any(|t| SKIP_FORM_TAGS.contains(&t.as_str())) {
                    continue;
                }
                // Some extracts list bare suffixes as forms; require a shared stem there.
                let stem: String = e.word.chars().take(2).collect::<String>().to_lowercase();
                if strict_forms && !f.form.to_lowercase().starts_with(&stem) {
                    continue;
                }
                out.forms.push((f.form.clone(), e.word.clone()));
            }
        }
    }
    out
}

/// Word -> rank (1 = most frequent) from "word count" lines.
pub fn freq(path: &Path) -> std::collections::HashMap<String, u32> {
    let mut m = std::collections::HashMap::new();
    for (i, line) in lines(path).enumerate() {
        if let Some(w) = line.split(' ').next() {
            for t in leo_core::norm::tokenize(w) {
                m.entry(t).or_insert(i as u32 + 1);
            }
        }
    }
    m
}
