//! Check messages that help the writer converge: for each error, what was understood so far,
//! what was missing and the closest approved forms (a word outside the lexicon, a verb with an
//! object of the wrong type, a wrong preposition). The error takes one line; the hint, another.
//! Nothing here decides what the language accepts: it only describes the failure and points the way.

use super::lexicon::{Lexicon, Tok, Verb};
use super::syntax::{sentence_f, tokenize_spans, ErrorF, Sentence, Parsed};

/// Error reading a line: the message (one line) and the hint (another, optional).
pub struct Failure {
    pub msg: String,
    pub hint: Option<String>,
}

/// Reads the sentence (without anchors); the failure carries what was understood, what was missing and the hint.
pub fn read_line(text: &str, lex: &Lexicon) -> Result<Sentence, Failure> {
    let parsed = match tokenize_spans(text, lex) {
        Ok(l) => l,
        Err(e) => {
            return Err(match e.word {
                Some((pos, w)) => failure_word(text, lex, pos, &w),
                None => Failure { msg: e.msg, hint: Some("outside quotes only lexicon words, numbers, commas and periods are allowed; names go in quotes".into()) },
            })
        }
    };
    sentence_f(&parsed.toks).map_err(|e| failure_syntax(text, &parsed, &e, lex))
}

// ------------------------------------------------------------------ word outside the lexicon

/// Edit distance between two words.
pub fn distance(a: &str, b: &str) -> usize {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let mut before: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut current = vec![i + 1];
        for (j, cb) in b.iter().enumerate() {
            let cost = before[j] + usize::from(ca != cb);
            current.push(cost.min(before[j + 1] + 1).min(current[j] + 1));
        }
        before = current;
    }
    before[b.len()]
}

/// The letters differ only by a swap of two neighbors (`teh` and `the`).
fn only_transposition(a: &[char], b: &[char]) -> bool {
    a.len() == b.len() && (0..a.len().saturating_sub(1)).any(|i| a[i] == b[i + 1] && a[i + 1] == b[i] && a[..i] == b[..i] && a[i + 2..] == b[i + 2..])
}

/// The distance between the typed word and the approved form, if small enough to look like a
/// typo (and not another word). Up to 3 letters: only equal; 4 to 5: one letter more, one
/// less or two neighbors swapped, never one letter for another (click/clock, fails/falls are
/// different words); 6: one difference; 7 or more: up to two. The first letter must be the
/// same, except for a single difference in a word of 7 letters or more (allows/follows).
fn similar(w: &str, f: &str) -> Option<usize> {
    let (a, b): (Vec<char>, Vec<char>) = (w.chars().collect(), f.chars().collect());
    let transposed = only_transposition(&a, &b);
    let d = if transposed { 1 } else { distance(w, f) };
    if d == 0 {
        return Some(0);
    }
    let n = a.len();
    let ok = match n {
        0..=3 => false,
        4..=5 => transposed || (d == 1 && a.len() != b.len()),
        6 => d == 1,
        _ => d <= 2,
    };
    let initial = a.first() == b.first() || (d == 1 && n >= 7);
    (ok && initial).then_some(d)
}

/// An approved form that may replace the unknown word.
#[derive(Clone, Debug)]
pub struct Suggestion {
    /// the approved form to write
    pub form: String,
    pub class: &'static str,
    /// the lexicon word that lists it as a synonym (when the suggestion comes from `syn:`)
    pub synonym: Option<String>,
    pub dist: usize,
    order: usize,
    entry: String,
}

impl Suggestion {
    pub fn text(&self) -> String {
        match &self.synonym {
            Some(s) => format!("'{}' ({}; the lexicon lists '{s}' as a synonym)", self.form, self.class),
            None => format!("'{}' ({})", self.form, self.class),
        }
    }
}

/// The (up to 3) approved forms closest to the word, of the requested classes (`type`, `verb`,
/// `adjective`, `core`): by a synonym listed in the lexicon (same word) and by typo
/// (`similar`, against the whole form and against its first word). The caller passes only the
/// class the grammar expects at the position.
pub fn suggestions(lex: &Lexicon, word: &str, classes: &[&str]) -> Vec<Suggestion> {
    let w = word.to_lowercase();
    let mut all: Vec<Suggestion> = Vec::new();
    let mut order = 0;
    let mut cand = |class: &'static str, entry: &str, forms: &[String], syn: &[String]| {
        if !classes.contains(&class) {
            return;
        }
        let first = forms.first().cloned().unwrap_or_default();
        for f in forms {
            let d = [Some(f.as_str()), f.split(' ').next()].into_iter().flatten().filter_map(|c| similar(&w, c)).min();
            if let Some(d) = d {
                all.push(Suggestion { form: f.clone(), class, synonym: None, dist: d, order, entry: entry.into() });
            }
        }
        // a synonym only counts when exact: similar to a synonym is not similar to the entry
        for s in syn {
            if *s == w {
                all.push(Suggestion { form: first.clone(), class, synonym: Some(s.clone()), dist: 0, order, entry: entry.into() });
            }
        }
        order += 1;
    };
    for t in &lex.kinds {
        cand("type", &t.id, &t.en, &t.syn);
    }
    for v in &lex.verbs {
        cand("verb", &v.id, &v.en, &v.syn);
    }
    for a in &lex.adjectives {
        cand("adjective", &a.id, &a.en, &a.syn);
    }
    let core: Vec<String> = [&lex.article, &lex.negation, &lex.universal, &lex.exception, &lex.quantifier, &lex.coordination, &lex.preposition]
        .iter()
        .flat_map(|v| v.iter().cloned())
        .collect();
    for f in &core {
        cand("core", f, std::slice::from_ref(f), &[]);
    }
    all.sort_by(|a, b| (a.dist, a.synonym.is_none(), a.order).cmp(&(b.dist, b.synonym.is_none(), b.order)));
    let mut views: Vec<(&'static str, String)> = Vec::new();
    let mut out = Vec::new();
    for s in all {
        let k = (s.class, s.entry.clone());
        if views.contains(&k) {
            continue;
        }
        views.push(k);
        out.push(s);
        if out.len() == 3 {
            break;
        }
    }
    out
}

/// The lexicon word class the grammar asks for at a position.
fn classes_of(expected: &[&str]) -> Vec<&'static str> {
    let mut v: Vec<&'static str> = Vec::new();
    for e in expected {
        match *e {
            "VERB" | "VNEG" => v.push("verb"),
            "NOUN" => v.push("type"),
            "ADJ" => v.push("adjective"),
            _ => v.push("core"),
        }
    }
    if v.is_empty() {
        v = vec!["type", "verb", "adjective", "core"];
    }
    v.dedup();
    v
}

/// What the grammar asks for after `prefix` (the text already understood), if it is a valid
/// beginning of a sentence.
fn expected_after(prefix: &str, lex: &Lexicon) -> Vec<&'static str> {
    if prefix.is_empty() {
        return Vec::new();
    }
    let Ok(l) = tokenize_spans(prefix, lex) else { return Vec::new() };
    match sentence_f(&l.toks) {
        Err(e) if e.pos >= l.toks.len() && e.text.is_none() => e.expected,
        _ => Vec::new(),
    }
}

fn desc_class(c: &str) -> &'static str {
    match c {
        "ART" => "the article 'the'",
        "NOUN" => "a type (like module)",
        "NAME" => "a quoted name",
        "VERB" | "VNEG" => "a verb",
        "END" => "the final period",
        "COMMA" => "a comma",
        "NUM" => "a number",
        "ADJ" => "an adjective",
        "AND" => "'and'",
        "PREP" => "a preposition",
        "COND" => "'if'",
        "CMP" => "a comparison",
        "IS" => "'is'",
        _ => "something else",
    }
}

fn desc_list(expected: &[&str]) -> String {
    expected.iter().map(|c| desc_class(c)).collect::<Vec<_>>().join(" or ")
}

fn context(prefix: &str) -> String {
    if prefix.is_empty() {
        "at the start of the sentence".to_string()
    } else {
        format!("after '{prefix}'")
    }
}

/// The `lexicon propose` command ready for the word, with the class the position asks for.
fn command_propose(word: &str, classes: &[&str]) -> Option<String> {
    let class = match classes {
        ["verb"] => "verb",
        ["type"] => "type",
        ["adjective"] => "adjective",
        c if c.contains(&"core") && c.len() == 1 => return None,
        _ => "<type|verb|adjective>",
    };
    Some(format!("intentree lexicon propose {class} \"{word}\" --meaning \"<what it means>\""))
}

fn failure_word(text: &str, lex: &Lexicon, pos: usize, w: &str) -> Failure {
    let prefix = text[..pos].trim();
    let expected = expected_after(prefix, lex);
    let expected_part = if expected.is_empty() { String::new() } else { format!("; expected {}", desc_list(&expected)) };
    let msg = format!("word outside the lexicon: '{w}' ({}{expected_part})", context(prefix));
    let classes = classes_of(&expected);
    let mut hint: Vec<String> = Vec::new();
    match expected.first().copied() {
        Some("NAME") => hint.push(format!("names go in quotes: write \"{w}\" with the type before it (the module \"{w}\")")),
        Some("ART") => hint.push("every object starts with 'the' and takes a type and a name: the <type> \"<name>\"".to_string()),
        _ if prefix.is_empty() => hint.push("the sentence starts with 'The <type> \"<name>\"' (or with 'If', 'When', 'While' or 'Every')".to_string()),
        _ if w == "by" => hint.push("there is no passive voice: make the subject the subject (The module \"x\" reads ...)".to_string()),
        _ => {
            let sug = suggestions(lex, w, &classes);
            if sug.is_empty() {
                hint.push("no similar approved form".to_string());
            } else {
                hint.push(format!("close approved forms: {}", sug.iter().map(Suggestion::text).collect::<Vec<_>>().join(", ")));
            }
            match command_propose(w, &classes) {
                Some(c) => hint.push(format!("if none fits, propose one: {c}")),
                None => hint.push(format!(
                    "prepositions, 'and' and the article are fixed: after the object only '.', 'and' or a preposition the verb accepts ({}) fit; another meaning needs another sentence",
                    lex.preposition.join(", ")
                )),
            }
        }
    }
    Failure { msg, hint: Some(hint.join("; ")) }
}

// ------------------------------------------------------------------ syntax error

fn excerpt<'t>(text: &'t str, parsed: &Parsed, pos: usize) -> &'t str {
    parsed.spans.get(pos).map(|&(a, b)| &text[a..b]).unwrap_or("")
}

fn describe_finding(text: &str, parsed: &Parsed, pos: usize) -> String {
    let Some(t) = parsed.toks.get(pos) else { return "the end of the line".to_string() };
    let tr = excerpt(text, parsed, pos);
    match t {
        Tok::Comma => "a comma".into(),
        Tok::End => "the final period".into(),
        Tok::And => "'and'".into(),
        Tok::Art => "the article 'the'".into(),
        Tok::Name(_) => format!("the name {tr}"),
        Tok::Num(_) => format!("the number {tr}"),
        Tok::Noun(_) => format!("the type '{tr}'"),
        Tok::Verb(_) | Tok::VerbNeg(_) => format!("the verb '{tr}'"),
        Tok::Adj(_) => format!("the adjective '{tr}'"),
        Tok::Prep(_) => format!("the preposition '{tr}'"),
        Tok::Cond(_) => format!("'{tr}'"),
        _ => format!("'{tr}'"),
    }
}

fn examples(lex: &Lexicon, forms: &[&str]) -> String {
    forms.iter().filter(|f| lex.sentences.contains_key(**f)).map(|f| format!("'{f}'")).collect::<Vec<_>>().join(", ")
}

fn failure_syntax(text: &str, parsed: &Parsed, e: &ErrorF, lex: &Lexicon) -> Failure {
    let upto = parsed.spans.get(e.pos).map(|&(a, _)| a).unwrap_or(text.len());
    let prefix = text[..upto].trim();
    let class = |i: usize| parsed.toks.get(i).map(Tok::class).unwrap_or("end");
    let finding = class(e.pos);
    let previous = if e.pos > 0 { class(e.pos - 1) } else { "" };
    let in_cond = matches!(parsed.toks.first(), Some(Tok::Cond(_)));
    if let Some(t) = &e.text {
        let hint = if t.starts_with("leftover text") {
            "one sentence per line: end with a period and start the next on a new line".to_string()
        } else if t.contains("every") {
            "for a condition and a quantifier together, write two sentences".to_string()
        } else {
            "see the pattern in FORMAT.md".to_string()
        };
        let msg = if prefix.is_empty() { t.clone() } else { format!("{t} (understood so far: '{prefix}')") };
        return Failure { msg, hint: Some(hint) };
    }
    let expected = e.expected.first().copied().unwrap_or("");
    let msg = format!("expected {} {}, found {}", desc_list(&e.expected), context(prefix), describe_finding(text, parsed, e.pos));
    let hint = match (expected, finding, previous) {
        ("VERB", "COMMA" | "END" | "end", _) if in_cond && !parsed.toks[..e.pos.min(parsed.toks.len())].iter().any(|t| matches!(t, Tok::Verb(_))) => {
            "the condition is a complete clause: If <subject> <verb> <object>, <clause>; for a state use 'is <adjective>', for a number 'is at least 3', for equality 'is the value \"x\"'".to_string()
        }
        ("VERB", "COMMA", _) => "the comma separates the condition from the result; in a list of objects the last one takes 'and' (X, Y and Z)".to_string(),
        ("VERB", ..) | ("VNEG", ..) => format!(
            "after the subject comes a verb from the lexicon (intentree lexicon): e.g. {}; negation with 'does not' or 'never' before the verb",
            examples(lex, &["uses", "reads", "writes", "calls", "sends", "contains"])
        ),
        ("ART", "VERB" | "VNEG", "AND" | "COMMA") => "'and' joins objects, not verbs: write one sentence per verb".to_string(),
        ("ART", "NOUN", _) => format!("every object starts with 'the': after '{}' write the {} \"<name>\"", excerpt(text, parsed, e.pos.saturating_sub(1)), excerpt(text, parsed, e.pos)),
        ("ART", "NAME", _) => "every name takes an article and a type: the <type> \"<name>\" (e.g. the module \"a\")".to_string(),
        ("ART", "END" | "end", "AND" | "COMMA") => "after 'and' comes another complete object: the <type> \"<name>\"".to_string(),
        ("ART", ..) => "after this word comes a complete object: the <type> \"<name>\"".to_string(),
        ("NOUN", "NAME", _) => "every thing takes a type before the name: the <type> \"<name>\" (e.g. the module \"a\")".to_string(),
        ("NOUN", ..) => format!("after 'the' comes a type from the lexicon (e.g. {})", examples(lex, &["module", "process", "file", "value", "thread"])),
        ("NAME", ..) => "after the type comes the quoted name: the module \"name\"".to_string(),
        ("END", "COMMA", _) => "a comma alone does not make a list: the last item takes 'and' (X, Y and Z); the comma also separates condition and result after 'If'".to_string(),
        ("END", "end", _) => "the final period of the sentence is missing".to_string(),
        ("END", ..) => "the sentence ends here: one clause per line; to join objects use 'and', for another verb write another sentence".to_string(),
        ("COMMA", ..) => "the condition ends with a comma: If <clause>, <clause>.".to_string(),
        ("NUM", ..) => "a comparison takes a number (is at least 3) or another value (is at most the value \"x\")".to_string(),
        ("ADJ", ..) => format!(
            "after 'is' comes an adjective from the lexicon ({}), a number or a value: is the value \"x\"",
            lex.adjectives.iter().take(4).filter_map(|a| a.en.first()).map(|x| format!("'{x}'")).collect::<Vec<_>>().join(", ")
        ),
        _ => "see the pattern in FORMAT.md".to_string(),
    };
    Failure { msg, hint: Some(hint) }
}

// ------------------------------------------------------------------ semantic errors

/// The first English form of the type.
pub fn name_kind(lex: &Lexicon, id: &str) -> String {
    lex.kind(id).and_then(|t| t.en.first()).cloned().unwrap_or_else(|| id.to_string())
}

/// The first English form of the verb.
pub fn name_verb(lex: &Lexicon, id: &str) -> String {
    lex.verb(id).and_then(|v| v.en.first()).cloned().unwrap_or_else(|| id.to_string())
}

pub fn list_kinds(lex: &Lexicon, ids: &[String]) -> String {
    ids.iter().map(|t| name_kind(lex, t)).collect::<Vec<_>>().join(", ")
}

/// The role the type plays in the sentence: the subject or the object.
#[derive(Clone, Copy, PartialEq)]
pub enum Role {
    Subject,
    Object,
}

fn accepts(lex: &Lexicon, constraint: &Option<Vec<String>>, kind: &str) -> bool {
    match constraint {
        Some(r) if !r.is_empty() => r.iter().any(|x| lex.is_a(kind, x)),
        _ => true,
    }
}

/// Verbs that accept `kind` in the given role (and `other`, the type at the other end, when known).
/// Returns (close, general): the close ones are those the lexicon ties to the verb by synonym
/// (`syn:` or a shared form); the general ones are `uses`, `reads` and `calls`, when they fit.
/// At most 3 of each.
pub fn verbs_similar(lex: &Lexicon, v: &Verb, role: Role, kind: &str, other: Option<&str>) -> (Vec<String>, Vec<String>) {
    let my: Vec<&str> = v.en.iter().chain(v.syn.iter()).map(String::as_str).collect();
    let mut next_words: Vec<String> = Vec::new();
    for x in &lex.verbs {
        if x.id == v.id || x.target.as_deref().is_some_and(|a| a != "entity") || x.subject_kind.as_ref().is_some_and(|a| a.iter().any(|t| t == "e.view")) {
            continue;
        }
        let (ok_subj, ok_obj) = match role {
            Role::Subject => (accepts(lex, &x.subject_kind, kind), other.is_none_or(|o| accepts(lex, &x.obj_kind, o))),
            Role::Object => (other.is_none_or(|o| accepts(lex, &x.subject_kind, o)), accepts(lex, &x.obj_kind, kind)),
        };
        let its = x.en.iter().chain(x.syn.iter()).map(String::as_str);
        if ok_subj && ok_obj && its.into_iter().any(|d| my.contains(&d)) {
            next_words.extend(x.en.first().cloned());
        }
    }
    next_words.truncate(3);
    let mut general: Vec<String> = Vec::new();
    for word in ["uses", "reads", "calls"] {
        let Some(id) = lex.sentences.get(word).and_then(|t| if let Tok::Verb(id) = t { Some(id) } else { None }) else { continue };
        let Some(x) = lex.verb(id) else { continue };
        let ok = match role {
            Role::Subject => accepts(lex, &x.subject_kind, kind),
            Role::Object => accepts(lex, &x.obj_kind, kind),
        };
        if ok && x.id != v.id {
            general.push(word.to_string());
        }
    }
    (next_words, general)
}

/// Hint for the verb that refused the type: similar verbs that accept it, or change the type.
pub fn hint_kind(lex: &Lexicon, v: &Verb, role: Role, kind: &str, other: Option<&str>) -> String {
    let who = if role == Role::Subject { "as subject" } else { "as object" };
    let (next_words, general) = verbs_similar(lex, v, role, kind, other);
    let quotes = |vs: &[String]| vs.iter().map(|x| format!("'{x}'")).collect::<Vec<_>>().join(", ");
    let verbs = if !next_words.is_empty() {
        format!("close verbs that accept '{}' {who}: {}", name_kind(lex, kind), quotes(&next_words))
    } else if !general.is_empty() {
        format!("general verbs that accept '{}' {who}: {}", name_kind(lex, kind), quotes(&general))
    } else {
        format!("no similar verb accepts '{}' {who}", name_kind(lex, kind))
    };
    format!("{verbs}; or give the thing one of the types '{}' accepts", v.en.first().map(String::as_str).unwrap_or("?"))
}

/// Message and hint for the preposition the verb does not accept.
pub fn failure_prep(lex: &Lexicon, verb_id: &str, prep: &str) -> (String, String) {
    let v = lex.verb(verb_id);
    let en = name_verb(lex, verb_id);
    let accepted: Vec<&str> = v.map(|v| v.preps.iter().map(|(p, _)| p.as_str()).collect()).unwrap_or_default();
    let list = if accepted.is_empty() { "none".to_string() } else { accepted.join(", ") };
    let msg = format!("'{en}' does not accept the preposition '{prep}' (accepted: {list})");
    let with_items: Vec<String> =
        lex.verbs.iter().filter(|x| x.id != verb_id && x.prep_pt(prep).is_some()).take(4).map(|x| format!("'{}'", x.en.first().map(String::as_str).unwrap_or("?"))).collect();
    let mut hint = if with_items.is_empty() { format!("no verb accepts '{prep}'") } else { format!("verbs that accept '{prep}': {}", with_items.join(", ")) };
    hint.push_str("; or write the target as an object (and) or in another sentence");
    (msg, hint)
}

/// Message and hint for the required role that was missing.
pub fn failure_role(lex: &Lexicon, verb_id: &str, role: &str) -> (String, String) {
    let en = name_verb(lex, verb_id);
    let required: Vec<String> = lex.verb(verb_id).map(|v| v.required.clone()).unwrap_or_default();
    let msg = format!("'{en}' requires the preposition '{role}' in the sentence");
    let parts: Vec<String> = required.iter().map(|p| format!("{p} the <type> \"<name>\"")).collect();
    let hint = format!("full form: The <type> \"<name>\" {en} the <type> \"<name>\" {}.", parts.join(" "));
    (msg, hint)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edit_distance() {
        assert_eq!(distance("reads", "reads"), 0);
        assert_eq!(distance("redas", "reads"), 2);
        assert_eq!(distance("", "ab"), 2);
        assert_eq!(distance("kitten", "sitting"), 3);
    }
}
