//! Canonical sentences: English (reparsable, one predication per line) and Portuguese (for reading).

use super::analysis::{Target, Analysis, Fact, Invariant, Note, Rule, Value, PREFIX_NOTE};
use super::json::number;

fn cap(t: &str) -> String {
    let mut c = t.chars();
    match c.next() {
        Some(p) => p.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

fn np_en(a: &Analysis, eid: &str) -> String {
    let (kind, name) = a.entities.get(eid).map(|e| (e.kind.as_str(), e.name.as_str())).unwrap_or(("", "?"));
    let en = a.lex.kind(kind).and_then(|t| t.en.first()).map(String::as_str).unwrap_or("?");
    format!("the {en} \"{name}\"")
}

/// The target of a relation in English: a state (`"name"`), a literal (`"text"`) or an object.
fn target_en(a: &Analysis, f: &Fact) -> Option<String> {
    if f.rel() == "a.has_states" {
        return Some(format!("\"{}\"", f.obj().map(|o| a.name(o)).unwrap_or("?")));
    }
    if let Some(l) = f.literal() {
        return Some(format!("\"{l}\""));
    }
    f.obj().map(|o| np_en(a, o))
}

/// 'X', 'X and Y' or 'X, Y and Z'.
fn coordinates(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [x] => x.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// English clause of one or more facts of the same clause (same subject, verb, mode and
/// roles; only the target varies). Coordinated targets come before the roles, as in the input.
fn clause_en_of(a: &Analysis, facts: &[&Fact]) -> String {
    clause_en_with(a, facts, np_en(a, facts[0].subject()))
}

/// Like `clause_en_of`, with the subject already written (the quantified subject of rules).
fn clause_en_with(a: &Analysis, facts: &[&Fact], s: String) -> String {
    let f = facts[0];
    match f {
        Fact::Prop { adj, .. } => {
            let en = a.lex.adjective(adj).and_then(|x| x.en.first()).map(String::as_str).unwrap_or("?");
            return format!("{s} is {en}");
        }
        Fact::Absent { role, .. } => {
            let en = a.lex.absence_of_role(role).and_then(|x| x.en.first()).map(String::as_str).unwrap_or("?");
            return format!("{s} {en}");
        }
        Fact::Cmp { op, value, .. } => {
            // the last phrase with the operator wins
            let sentence = match (op.as_str(), value) {
                ("==", _) => "is",
                ("!=", _) => "is not",
                // a limit by another value reads better with 'is bounded by'
                ("<=", Value::Ent(_)) => "is bounded by",
                _ => a.lex.comparisons.iter().rev().find(|(_, o)| o == op).map(|(f, _)| f.as_str()).unwrap_or("?"),
            };
            let v = match value {
                Value::Num(x) => number(*x),
                Value::Ent(e) => np_en(a, e),
            };
            return format!("{s} {sentence} {v}");
        }
        Fact::Rel { .. } => {}
    }
    let v = a.lex.verb(f.rel()).expect("verb in the lexicon");
    let forms = &v.en;
    let mut verb = forms[0].clone();
    match f.mode() {
        // a verb with a copula negates with 'is not' ('is not in'); the others with 'does not'
        Some("not") if forms[0].starts_with("is ") => verb = format!("is not {}", &forms[0][3..]),
        Some("not") => verb = format!("does not {}", form_base(forms)),
        Some("always") => verb = format!("always {verb}"),
        Some("exception") => verb = format!("exceptionally {verb}"),
        _ => {}
    }
    let mut parts = vec![s, verb];
    let targets: Vec<String> = facts.iter().filter_map(|f| target_en(a, f)).collect();
    if !targets.is_empty() {
        parts.push(coordinates(&targets));
    }
    if let Fact::Rel { roles, .. } = f {
        for (p, e) in roles {
            parts.push(format!("{p} {}", np_en(a, e)));
        }
    }
    parts.join(" ")
}

/// Base form of the verb, the one that follows 'does not': the second lexicon form when it
/// exists ('uses, use'), otherwise the first with 'has' replaced by 'have' ('has the unit' ->
/// 'have the unit', which the lexicon also accepts; see `Lexicon::build_sentences`).
fn form_base(forms: &[String]) -> String {
    if let Some(f) = forms.get(1) {
        return f.clone();
    }
    match forms[0].strip_prefix("has ") {
        Some(rest) => format!("have {rest}"),
        None => forms[0].clone(),
    }
}

fn clause_en(a: &Analysis, f: &Fact) -> String {
    clause_en_of(a, &[f])
}

/// Gathers the coordinated condition ('A uses B and C', 'A prints "a" and "b"') into one clause.
fn condition_en(a: &Analysis, facts: &[Fact]) -> String {
    clause_en_of(a, &facts.iter().collect::<Vec<_>>())
}

/// Canonical English sentence of a fact.
pub fn fact_en(a: &Analysis, f: &Fact) -> String {
    cap(&clause_en(a, f)) + "."
}

/// Canonical English sentences of an invariant (one per consequent).
pub fn invariant_en(a: &Analysis, i: &Invariant) -> Vec<String> {
    match &i.when {
        Some(when) => i.then.iter().map(|en| format!("{} {}, {}.", cap(&i.kind), condition_en(a, when), clause_en(a, en))).collect(),
        None => i.then.iter().map(|f| cap(&clause_en(a, f)) + ".").collect(),
    }
}

/// Canonical English sentence of a quantified rule.
pub fn rule_en(a: &Analysis, r: &Rule) -> String {
    let t = a.lex.kind(&r.kind).and_then(|t| t.en.first()).map(String::as_str).unwrap_or("?");
    cap(&clause_en_with(a, &[&r.then], format!("every {t} in {}", np_en(a, &r.scope)))) + "."
}

/// Canonical Portuguese sentence of a quantified rule.
pub fn rule_pt(a: &Analysis, r: &Rule) -> String {
    let (pt, g) = a.lex.kind(&r.kind).map(|t| (t.label_pt(), t.gender())).unwrap_or(("?", "m"));
    let subject = format!("{} {pt} em {}", if g == "m" { "todo" } else { "toda" }, np_pt(a, &r.scope));
    cap(&contract(clause_pt_with(a, &[&r.then], subject, g))) + "."
}

/// The fact (or invariant) comes from expanding a rule: the canonical text shows only the rule.
fn derived(a: &Analysis, j: crate::intent::json::J) -> bool {
    a.derived.contains(&j.compact())
}

pub fn canonical_en(a: &Analysis) -> Vec<String> {
    let mut lines: Vec<String> = a.facts.iter().filter(|f| !derived(a, f.json())).map(|f| fact_en(a, f)).collect();
    for i in a.invariants.iter().filter(|i| !derived(a, i.json())) {
        lines.extend(invariant_en(a, i));
    }
    lines.extend(a.rules.iter().map(|r| rule_en(a, r)));
    lines.sort();
    if a.notes.is_empty() {
        return lines;
    }
    // notes: the file ones at the top; the sentence ones right after it (reparsing reattaches them)
    let note = |t: &str| format!("{PREFIX_NOTE} {t}");
    // the top note carries the file name (`Note[file]:`) when there is one; notes stay in file order
    let top = |n: &Note| match n.file.as_deref() {
        Some(file) if !file.is_empty() => format!("Note[{file}]: {}", n.text),
        _ => note(&n.text),
    };
    let mut out: Vec<String> = a.notes.iter().filter(|n| n.sentence.is_none()).map(top).collect();
    for l in lines {
        let ns: Vec<String> = a.notes.iter().filter(|n| n.sentence.as_deref() == Some(l.as_str())).map(|n| note(&n.text)).collect();
        out.push(l);
        out.extend(ns);
    }
    out
}

/// Notes attached to a canonical English sentence, in the Portuguese reading format.
fn notes_pt(a: &Analysis, sentence_en: &str) -> String {
    a.notes.iter().filter(|n| n.sentence.as_deref() == Some(sentence_en)).map(|n| format!("Nota: {}\n", n.text)).collect()
}

fn ops_pt(op: &str) -> &'static str {
    match op {
        ">=" => "é no mínimo",
        ">" => "é maior que",
        "<=" => "é no máximo",
        "<" => "é menor que",
        "==" => "é",
        "!=" => "não é",
        _ => "?",
    }
}

fn np_pt(a: &Analysis, eid: &str) -> String {
    let (kind, name) = a.entities.get(eid).map(|e| (e.kind.as_str(), e.name.as_str())).unwrap_or(("", "?"));
    match a.lex.kind(kind) {
        Some(t) => format!("{} {} “{name}”", if t.gender() == "m" { "o" } else { "a" }, t.label_pt()),
        None => format!("a ? “{name}”"),
    }
}

const CONTRACTIONS: [(&str, &str); 8] = [
    (" por o ", " pelo "),
    (" por a ", " pela "),
    (" de o ", " do "),
    (" de a ", " da "),
    (" em o ", " no "),
    (" em a ", " na "),
    (" a o ", " ao "),
    (" a a ", " à "),
];

fn contract(t: String) -> String {
    CONTRACTIONS.iter().fold(t, |t, (from, to)| t.replace(from, to))
}

pub fn clause_pt(a: &Analysis, f: &Fact) -> String {
    clause_pt_of(a, &[f])
}

/// The target of a relation in Portuguese: a state, a literal or an object.
fn target_pt(a: &Analysis, f: &Fact) -> Option<String> {
    if let Fact::Rel { target: Target::Time(l, _), .. } = f {
        return Some(format!("“{}”", l.replace(" to ", " a ")));
    }
    if f.rel() == "a.has_states" {
        return Some(format!("“{}”", f.obj().map(|o| a.name(o)).unwrap_or("?")));
    }
    if let Some(l) = f.literal() {
        return Some(format!("“{l}”"));
    }
    f.obj().map(|o| np_pt(a, o))
}

/// 'X', 'X e Y' or 'X, Y e Z'.
fn coordinates_pt(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [x] => x.clone(),
        [rest @ .., last] => format!("{} e {last}", rest.join(", ")),
    }
}

/// Portuguese clause of facts of the same clause (only the target varies), like `clause_en_of`.
fn clause_pt_of(a: &Analysis, facts: &[&Fact]) -> String {
    let g = a.entities.get(facts[0].subject()).and_then(|e| a.lex.kind(&e.kind)).map(|t| t.gender().to_string()).unwrap_or_else(|| "m".into());
    clause_pt_with(a, facts, np_pt(a, facts[0].subject()), &g)
}

/// Like `clause_pt_of`, with the subject already written and its gender.
fn clause_pt_with(a: &Analysis, facts: &[&Fact], s: String, g: &str) -> String {
    let f = facts[0];
    match f {
        Fact::Prop { adj, .. } => {
            let Some(adj_entry) = a.lex.adjective(adj) else { return format!("{s} é ?") };
            let form = adj_entry.form_pt(g);
            return format!("{s} {} {form}", if adj_entry.be { "está" } else { "é" });
        }
        Fact::Absent { role, .. } => {
            return format!("{s} {}", a.lex.absence_of_role(role).map(|x| x.label_pt()).unwrap_or("?"));
        }
        Fact::Cmp { op, value, .. } => {
            let (op_pt, v) = match value {
                Value::Num(x) => (ops_pt(op), number(*x)),
                Value::Ent(e) if op == "<=" => (if g == "m" { "é limitado por" } else { "é limitada por" }, np_pt(a, e)),
                Value::Ent(e) => (ops_pt(op), np_pt(a, e)),
            };
            return contract(format!("{s} {op_pt} {v}"));
        }
        Fact::Rel { .. } => {}
    }
    let v = a.lex.verb(f.rel()).expect("verb in the lexicon");
    let mut verb = v.label_pt().to_string();
    match f.mode() {
        Some("not") => verb = format!("não {verb}"),
        Some("always") => verb = format!("sempre {verb}"),
        Some("exception") => verb = format!("excepcionalmente {verb}"),
        _ => {}
    }
    if f.obj().is_none() && f.literal().is_none() {
        if let Some(intransitive) = &v.pt_intransitive {
            verb = verb.replace(v.label_pt(), intransitive);
        }
    }
    let mut parts = vec![s, verb];
    let targets: Vec<String> = facts.iter().filter_map(|f| target_pt(a, f)).collect();
    if !targets.is_empty() {
        parts.push(coordinates_pt(&targets));
    }
    if let Fact::Rel { roles, .. } = f {
        for (p, e) in roles {
            parts.push(format!("{} {}", v.prep_pt(p).unwrap_or("?"), np_pt(a, e)));
        }
    }
    contract(parts.join(" "))
}

pub fn invariant_pt(a: &Analysis, i: &Invariant) -> String {
    let join_clauses = |fs: &[Fact], sep: &str| fs.iter().map(|f| clause_pt(a, f)).collect::<Vec<_>>().join(sep);
    match &i.when {
        None => join_clauses(&i.then, "; "),
        Some(when) => {
            let conj = a.lex.conditional.iter().find(|(w, _)| *w == i.kind).map(|(_, pt)| pt.as_str()).unwrap_or("?");
            let cond = contract(clause_pt_of(a, &when.iter().collect::<Vec<_>>()));
            format!("{conj} {cond}, {}", join_clauses(&i.then, " e "))
        }
    }
}

/// Canonical sentence of a fact in Portuguese.
pub fn fact_pt(a: &Analysis, f: &Fact) -> String {
    cap(&clause_pt(a, f)) + "."
}

/// Canonical sentence of an invariant in Portuguese.
pub fn invariant_sentence_pt(a: &Analysis, i: &Invariant) -> String {
    cap(&invariant_pt(a, i)) + "."
}

/// Output of `canon --en`.
pub fn text_en(a: &Analysis) -> String {
    canonical_en(a).join("\n") + "\n"
}

/// Output of `canon --pt`: facts, a blank line, invariants.
pub fn text_pt(a: &Analysis) -> String {
    let mut o = String::new();
    for n in a.notes.iter().filter(|n| n.sentence.is_none()) {
        o.push_str(&format!("Nota: {}\n", n.text));
    }
    for f in a.facts.iter().filter(|f| !derived(a, f.json())) {
        o.push_str(&cap(&clause_pt(a, f)));
        o.push_str(".\n");
        o.push_str(&notes_pt(a, &fact_en(a, f)));
    }
    o.push('\n');
    for i in a.invariants.iter().filter(|i| !derived(a, i.json())) {
        o.push_str(&cap(&invariant_pt(a, i)));
        o.push_str(".\n");
        for l in invariant_en(a, i) {
            o.push_str(&notes_pt(a, &l));
        }
    }
    for r in &a.rules {
        o.push_str(&rule_pt(a, r));
        o.push('\n');
        o.push_str(&notes_pt(a, &rule_en(a, r)));
    }
    o
}
