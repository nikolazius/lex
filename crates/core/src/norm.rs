//! Text folding shared by the index builder and the query path.

enum Kind {
    Word,
    Skip,
    Sep,
}

fn fold_into(c: char, out: &mut String) -> Kind {
    match c {
        // combining marks (Russian stress), apostrophes, soft hyphen: dropped, do not split the word
        '\u{0300}'..='\u{036f}' | '\'' | '\u{2019}' | '\u{00ad}' => return Kind::Skip,
        _ => {}
    }
    if !c.is_alphanumeric() {
        return Kind::Sep;
    }
    for l in c.to_lowercase() {
        match l {
            'ä' | 'à' | 'á' | 'â' | 'ã' | 'å' | 'ā' => out.push('a'),
            'ö' | 'ò' | 'ó' | 'ô' | 'õ' | 'ø' | 'ō' => out.push('o'),
            'ü' | 'ù' | 'ú' | 'û' | 'ū' => out.push('u'),
            'è' | 'é' | 'ê' | 'ë' | 'ē' => out.push('e'),
            'ì' | 'í' | 'î' | 'ï' | 'ī' => out.push('i'),
            'ß' => out.push_str("ss"),
            'æ' => out.push_str("ae"),
            'œ' => out.push_str("oe"),
            'ç' => out.push('c'),
            'ñ' => out.push('n'),
            'ý' | 'ÿ' => out.push('y'),
            'ё' => out.push('е'),
            _ => out.push(l),
        }
    }
    Kind::Word
}

/// Lower-cased, diacritic-folded word tokens.
pub fn tokenize(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for c in s.chars() {
        match fold_into(c, &mut cur) {
            Kind::Word | Kind::Skip => {}
            Kind::Sep => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// Function words that are only indexed when they are the whole headword.
pub fn is_stop(t: &str) -> bool {
    matches!(
        t,
        "der" | "die" | "das" | "den" | "dem" | "des" | "ein" | "eine" | "einen" | "einem" | "einer" | "eines"
            | "sich" | "etw" | "jdn" | "jdm" | "jds" | "jd" | "jmd" | "jmdn" | "jmdm" | "und" | "oder" | "zu"
            | "ich" | "du" | "er" | "sie" | "es" | "wir" | "ihr"
            | "to" | "the" | "a" | "an" | "of" | "and" | "or" | "sth" | "sb" | "so" | "smth" | "smb"
            | "i" | "he" | "she" | "it" | "we" | "you" | "they"
            | "что" | "кого" | "чего" | "кому" | "чему" | "л" | "либо" | "и" | "или"
    )
}
