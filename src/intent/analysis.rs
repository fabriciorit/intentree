//! Semantics of `.intent`: entities (apposition and subtype chain), frames, normal form,
//! selection constraints, transitions, gaps and warnings.

use super::json::{repr_list, J};
use super::lexicon::Lexicon;
use super::sha1::{hex, sha1};
use super::hints::{self, Failure, Role};
use super::{layers, canon, time};
use super::syntax::{split_lines, split_anchors, trim_blank, Sentence, Np, Clause, ValueQ};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

/// entity id: first 10 hex digits of the SHA-1 of "root:name".
pub fn ident(root: &str, name: &str) -> String {
    hex(&sha1(format!("{root}:{name}").as_bytes()))[..10].to_string()
}

#[derive(Clone, Debug, PartialEq)]
pub enum Target {
    /// direct object (entity) or none
    Obj(Option<String>),
    Literal(String),
    /// time literal: canonical form and seconds (one value, or start and end)
    Time(String, Vec<f64>),
}

/// The other side of a comparison: a number or an entity.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Num(f64),
    Ent(String),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Fact {
    Prop { subject: String, adj: String },
    /// presupposed role left empty on purpose (the role name, as in `requires`)
    Absent { subject: String, role: String },
    /// comparison: `c.limit` (lexicon operators) or `c.equal` (`==`, `!=`), against a number or an entity
    Cmp { subject: String, op: String, value: Value },
    Rel { rel: String, subject: String, mode: Option<String>, target: Target, roles: BTreeMap<String, String> },
}

impl Fact {
    pub fn rel(&self) -> &str {
        match self {
            Fact::Prop { .. } => "prop",
            Fact::Absent { .. } => "absent",
            Fact::Cmp { op, .. } if op == "==" || op == "!=" => "c.equal",
            Fact::Cmp { .. } => "c.limit",
            Fact::Rel { rel, .. } => rel,
        }
    }

    pub fn subject(&self) -> &str {
        match self {
            Fact::Prop { subject, .. } | Fact::Absent { subject, .. } | Fact::Cmp { subject, .. } | Fact::Rel { subject, .. } => subject,
        }
    }

    /// `f.get("object")`: only exists in a relation with an object.
    pub fn obj(&self) -> Option<&str> {
        match self {
            Fact::Rel { target: Target::Obj(o), .. } => o.as_deref(),
            _ => None,
        }
    }

    pub fn literal(&self) -> Option<&str> {
        match self {
            Fact::Rel { target: Target::Literal(l) | Target::Time(l, _), .. } => Some(l),
            _ => None,
        }
    }

    pub fn mode(&self) -> Option<&str> {
        match self {
            Fact::Rel { mode, .. } => mode.as_deref(),
            _ => None,
        }
    }

    /// The same fact with another subject (the expansion of a quantified rule).
    pub fn with_subject(&self, new: &str) -> Fact {
        let mut f = self.clone();
        match &mut f {
            Fact::Prop { subject, .. } | Fact::Absent { subject, .. } | Fact::Cmp { subject, .. } | Fact::Rel { subject, .. } => *subject = new.to_string(),
        }
        f
    }

    /// All entities mentioned: subject, object, roles and the other side of the comparison.
    pub fn mentions(&self) -> Vec<&str> {
        let mut v = vec![self.subject()];
        match self {
            Fact::Rel { target: Target::Obj(Some(o)), roles, .. } => {
                v.push(o);
                v.extend(roles.values().map(String::as_str));
            }
            Fact::Rel { roles, .. } => v.extend(roles.values().map(String::as_str)),
            Fact::Cmp { value: Value::Ent(e), .. } => v.push(e),
            _ => {}
        }
        v
    }

    pub fn role(&self, p: &str) -> Option<&str> {
        match self {
            Fact::Rel { roles, .. } => roles.get(p).map(String::as_str),
            _ => None,
        }
    }

    /// JSON with the keys in alphabetical order.
    pub fn json(&self) -> J {
        let s = |x: &str| J::s(x);
        match self {
            Fact::Prop { subject, adj } => J::obj(vec![("adj", s(adj)), ("rel", s("prop")), ("subject", s(subject))]),
            Fact::Absent { subject, role } => J::obj(vec![("rel", s("absent")), ("role", s(role)), ("subject", s(subject))]),
            Fact::Cmp { subject, op, value } => match value {
                Value::Num(x) => J::obj(vec![("op", s(op)), ("rel", s(self.rel())), ("subject", s(subject)), ("value", J::Num(*x))]),
                Value::Ent(e) => J::obj(vec![("op", s(op)), ("other", s(e)), ("rel", s(self.rel())), ("subject", s(subject))]),
            },
            Fact::Rel { rel, subject, mode, target, roles } => {
                let mut v = Vec::new();
                if let Target::Literal(l) | Target::Time(l, _) = target {
                    v.push(("literal", s(l)));
                }
                v.push(("mode", mode.as_deref().map(s).unwrap_or(J::Null)));
                if let Target::Obj(o) = target {
                    v.push(("object", o.as_deref().map(s).unwrap_or(J::Null)));
                }
                v.push(("rel", s(rel)));
                v.push(("roles", J::Obj(roles.iter().map(|(k, x)| (k.clone(), s(x))).collect())));
                if let Target::Time(_, seg) = target {
                    v.push(("seconds", J::Arr(seg.iter().map(|x| J::Num(*x)).collect())));
                }
                v.push(("subject", s(subject)));
                J::obj(v)
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Invariant {
    pub kind: String,
    pub when: Option<Vec<Fact>>,
    pub then: Vec<Fact>,
}

impl Invariant {
    pub fn json(&self) -> J {
        let mut v = Vec::new();
        if let Some(when) = &self.when {
            v.push(("if", J::Arr(when.iter().map(Fact::json).collect())));
        }
        v.push(("then", J::Arr(self.then.iter().map(Fact::json).collect())));
        v.push(("type", J::s(&self.kind)));
        J::obj(v)
    }
}

/// Quantified rule: `Every <type> in <scope> ...`. The fact in `then` has the subject "*", which
/// stands for each member (an entity of the type contained in `scope`); the known members are
/// already expanded into `facts` and `invariants`, and the rule is kept for future members.
#[derive(Clone, Debug, PartialEq)]
pub struct Rule {
    pub kind: String,
    pub scope: String,
    pub then: Fact,
}

impl Rule {
    pub fn json(&self) -> J {
        J::obj(vec![("in", J::s(&self.scope)), ("then", J::Arr(vec![self.then.json()])), ("type", J::s(&self.kind))])
    }
}

#[derive(Clone, Debug)]
pub struct Entity {
    pub id: String,
    pub name: String,
    pub kind: String,
}

#[derive(Clone, Debug)]
pub struct Gap {
    pub entity: String,
    pub role: String,
    pub kind: String,
}

#[derive(Clone, Debug)]
pub struct Warning {
    pub entity: String,
    pub text: String,
}

/// Error of a sentence: the message and the hint (one line each).
type Er = (String, Option<String>);

/// Sentence line with the label used in errors ("line 3" or "file.intent:3").
pub struct Line {
    pub label: String,
    pub text: String,
    /// source file (empty when the text did not come from a project file)
    pub file: String,
}

/// Free note: a `Note: <text>` line right after a sentence (stays with the canonical sentence
/// of each thing it produced) or at the top of a file (stays with the file). It is not part of
/// the formal IR: it goes in the `notes` field, so views can show it as informal.
#[derive(Clone, Debug, PartialEq)]
pub struct Note {
    /// canonical English sentence the note attaches to
    pub sentence: Option<String>,
    pub file: Option<String>,
    pub text: String,
    /// position of the note in the lines read (files in project order): `Analysis::notes` is
    /// sorted by it. Not in the `json`: the formal IR does not depend on order.
    pub order: usize,
}

impl Note {
    pub fn json(&self) -> J {
        let mut v = Vec::new();
        if let Some(a) = &self.file {
            v.push(("file", J::s(a)));
        }
        if let Some(f) = &self.sentence {
            v.push(("sentence", J::s(f)));
        }
        v.push(("text", J::s(&self.text)));
        J::obj(v)
    }
}

/// What a sentence produced, to attach its notes.
enum Product {
    Fact(Fact),
    Inv(Invariant),
    Rule(Rule),
}

/// Lines of a text, labeled "line N", without comments.
#[cfg_attr(not(test), allow(dead_code))]
pub fn lines_of(text: &str) -> Vec<Line> {
    split_lines(text)
        .into_iter()
        .enumerate()
        .map(|(i, l)| Line { label: format!("line {}", i + 1), text: l.to_string(), file: String::new() })
        .collect()
}

/// An accepted sentence: where it came from, the anchors it carried and what it produced.
#[derive(Clone, Debug)]
pub struct Origin {
    pub label: String,
    pub anchors: Vec<String>,
    pub facts: Vec<Fact>,
    pub invariants: Vec<Invariant>,
    /// the quantified rules the sentence declared
    pub rules: Vec<Rule>,
}

pub struct Analysis<'a> {
    pub lex: &'a Lexicon,
    pub errors: Vec<String>,
    /// one hint line per error (the key is the error text); not in the IR
    pub hints: BTreeMap<String, String>,
    /// by id (the IR is ordered by id)
    pub entities: BTreeMap<String, Entity>,
    pub facts: Vec<Fact>,
    pub invariants: Vec<Invariant>,
    pub rules: Vec<Rule>,
    /// Compact JSON of the facts and invariants that come from expanding a rule (the canonical
    /// text does not repeat them: reparsing the rule brings them back)
    pub derived: BTreeSet<String>,
    pub gaps: Vec<Gap>,
    pub warnings: Vec<Warning>,
    pub notes: Vec<Note>,
    /// effective layer per entity (id -> layer id), its own or inherited from the container
    pub layers: BTreeMap<String, String>,
    /// one per accepted sentence, in text order
    pub origins: Vec<Origin>,
    /// anchors per fact or invariant (key: compact JSON), only for those that have them
    anchors: HashMap<String, BTreeSet<String>>,
    key: HashMap<Np, String>,
}

/// Puts the `anchors` field (in alphabetical key order) only when there are anchors.
fn with_anchors(j: J, anchors: Vec<String>) -> J {
    match j {
        J::Obj(mut v) if !anchors.is_empty() => {
            let pos = v.iter().position(|(k, _)| k.as_str() > "anchors").unwrap_or(v.len());
            v.insert(pos, ("anchors".to_string(), J::Arr(anchors.iter().map(|a| J::s(a)).collect())));
            J::Obj(v)
        }
        j => j,
    }
}

/// Mentions (type, name) of a frame.
fn mentions<'q>(o: &'q Clause, out: &mut Vec<&'q Np>) {
    match o {
        Clause::Cmp { subject, value, .. } => {
            out.push(subject);
            if let ValueQ::Np(n) = value {
                out.push(n);
            }
        }
        Clause::Adj { subject, .. } | Clause::Abs { subject, .. } => out.push(subject),
        Clause::Rel { subject, obj, roles, .. } => {
            out.push(subject);
            out.extend(obj.iter());
            out.extend(roles.iter().map(|(_, n)| n));
        }
    }
}

/// Free-note line: `Note: text` (file None) or `Note[file]: text`, a form only the canonical
/// text emits so the top note does not lose its file when reparsed.
pub fn note_of(s: &str) -> Option<(Option<&str>, &str)> {
    let r = s.strip_prefix("Note")?;
    if let Some(t) = r.strip_prefix(':') {
        return Some((None, t));
    }
    let r = r.strip_prefix('[')?;
    let (file, t) = r.split_once("]:")?;
    Some((Some(file), t))
}

/// Start of a free-note line.
pub const PREFIX_NOTE: &str = "Note:";

/// The sentence uses `exceptionally` in some clause.
fn has_exception(q: &Sentence) -> bool {
    let exc = |o: &Clause| matches!(o, Clause::Rel { mode: Some(m), .. } if m == "exception");
    match q {
        Sentence::Simple(o) | Sentence::All { o, .. } => exc(o),
        Sentence::Cond { when, then, .. } => exc(when) || exc(then),
    }
}

/// Sorts by compact JSON and drops duplicates.
fn sort_unique<T>(v: Vec<T>, json: impl Fn(&T) -> J) -> Vec<T> {
    let mut keyed: Vec<(String, T)> = v.into_iter().map(|x| (json(&x).compact(), x)).collect();
    keyed.sort_by(|a, b| a.0.cmp(&b.0));
    keyed.dedup_by(|a, b| a.0 == b.0);
    keyed.into_iter().map(|(_, x)| x).collect()
}

impl<'a> Analysis<'a> {
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn text(text: &str, lex: &'a Lexicon) -> Analysis<'a> {
        Analysis::new(&lines_of(text), lex)
    }

    pub fn new(lines: &[Line], lex: &'a Lexicon) -> Analysis<'a> {
        let mut a = Analysis {
            lex,
            errors: Vec::new(),
            hints: BTreeMap::new(),
            entities: BTreeMap::new(),
            facts: Vec::new(),
            invariants: Vec::new(),
            rules: Vec::new(),
            derived: BTreeSet::new(),
            gaps: Vec::new(),
            warnings: Vec::new(),
            layers: BTreeMap::new(),
            notes: Vec::new(),
            origins: Vec::new(),
            anchors: HashMap::new(),
            key: HashMap::new(),
        };
        let mut syntactic: Vec<(&str, Sentence, Vec<String>)> = Vec::new();
        // notes: (sentence index, text) and (file, text); per file, the last sentence
        // (None if it did not pass: the note drops with it)
        let mut notes_sentence: Vec<(usize, usize, String)> = Vec::new();
        let mut notes_file: Vec<(usize, String, String)> = Vec::new();
        let mut last: HashMap<&str, Option<usize>> = HashMap::new();
        for (order, l) in lines.iter().enumerate() {
            let s = trim_blank(&l.text);
            if s.is_empty() || s.starts_with('#') {
                continue;
            }
            if let Some((file, t)) = note_of(s) {
                let t = trim_blank(t);
                if t.is_empty() {
                    a.errors.push(format!("{}: empty note", l.label));
                    continue;
                }
                // `Note[file]:` (only the canonical text emits it) fixes the file of the top note
                if let Some(file) = file {
                    notes_file.push((order, file.to_string(), t.to_string()));
                    continue;
                }
                match last.get(l.file.as_str()) {
                    None => notes_file.push((order, l.file.clone(), t.to_string())),
                    Some(Some(k)) => notes_sentence.push((order, *k, t.to_string())),
                    Some(None) => {}
                }
                continue;
            }
            last.insert(&l.file, None);
            let split = split_anchors(s)
                .map_err(|msg| Failure { msg, hint: Some("anchors go after the period, separated by spaces: ^file or ^file#symbol".into()) })
                .and_then(|(text, anchors)| hints::read_line(text, lex).map(|q| (q, anchors)));
            match split {
                Ok((q, anchors)) => {
                    last.insert(&l.file, Some(syntactic.len()));
                    syntactic.push((&l.label, q, anchors));
                }
                Err(f) => a.error(format!("{}: {}", l.label, f.msg), f.hint),
            }
        }
        a.resolver(syntactic.iter().map(|(_, q, _)| q));
        let (mut facts, mut invariants) = (Vec::new(), Vec::new());
        // quantified rules: expanded later, over the facts of the simple sentences
        let mut pending: Vec<(usize, &str, &Vec<String>, String, String, Vec<Fact>)> = Vec::new();
        let mut products: HashMap<usize, Vec<Product>> = HashMap::new();
        for (k, (label, q, anchors)) in syntactic.iter().enumerate() {
            let (mut fs_q, mut inv_q) = (Vec::new(), Vec::new());
            let r = match q {
                _ if has_exception(q) && !matches!(q, Sentence::Simple(_)) => {
                    Err(("'exceptionally' only applies in a simple sentence (the relation that goes up a layer)".to_string(), Some("write the sentence with 'exceptionally' and no condition: The module \"x\" exceptionally uses the module \"y\".".to_string())))
                }
                Sentence::All { kind, scope, o } => {
                    match a.semantics_of(o, Some(("*", kind))).and_then(|ts| a.check_rule(kind, &ts).map(|_| ts)) {
                        Ok(ts) => pending.push((k, label, anchors, kind.clone(), a.ent(scope), ts)),
                        Err((e, d)) => a.error(format!("{label}: {e}"), d),
                    }
                    continue;
                }
                // normal form: (P -> Q and R) becomes (P -> Q) and (P -> R); the condition stays whole
                Sentence::Cond { cond, when, then } => a.semantics(when).and_then(|when| {
                    a.semantics(then).map(|en| {
                        for c in en {
                            inv_q.push(Invariant { kind: cond.clone(), when: Some(when.clone()), then: vec![c] });
                        }
                    })
                }),
                Sentence::Simple(o) => a.semantics(o).map(|fs| {
                    for f in fs {
                        match f.mode() {
                            // the layer exception is an ordinary relation, marked
                            Some(m) if m != "exception" => inv_q.push(Invariant { kind: m.to_string(), when: None, then: vec![f] }),
                            _ => fs_q.push(f),
                        }
                    }
                }),
            };
            if let Err((e, d)) = r {
                a.error(format!("{label}: {e}"), d);
                continue;
            }
            for key in fs_q.iter().map(|f| f.json().compact()).chain(inv_q.iter().map(|i| i.json().compact())) {
                if !anchors.is_empty() {
                    a.anchors.entry(key).or_default().extend(anchors.iter().cloned());
                }
            }
            a.origins.push(Origin { label: label.to_string(), anchors: anchors.clone(), facts: fs_q.clone(), invariants: inv_q.clone(), rules: Vec::new() });
            let ps = products.entry(k).or_default();
            ps.extend(fs_q.iter().cloned().map(Product::Fact));
            ps.extend(inv_q.iter().cloned().map(Product::Inv));
            facts.extend(fs_q);
            invariants.extend(inv_q);
        }
        // members: what `scope` contains (simple sentences, no mode) with the rule type
        let mut rules = Vec::new();
        let mut without_members = Vec::new();
        for (k, label, anchors, kind, scope, ts) in pending {
            let mut members: Vec<&str> = facts
                .iter()
                .filter(|f: &&Fact| f.rel() == "a.contains" && f.mode().is_none() && f.subject() == scope)
                .filter_map(|f| f.obj())
                .filter(|o| lex.is_a(a.kind_of(o), &kind))
                .collect();
            members.sort();
            members.dedup();
            let members: Vec<String> = members.into_iter().map(String::from).collect();
            if members.is_empty() {
                let en = lex.kind(&kind).and_then(|t| t.en.first().map(String::as_str)).unwrap_or("?");
                without_members.push(Warning { entity: scope.clone(), text: format!("quantified rule without members: no {en} in \"{}\"", a.name(&scope)) });
            }
            let (mut fs_q, mut inv_q, mut rules_q) = (Vec::new(), Vec::new(), Vec::new());
            for t in &ts {
                let r = Rule { kind: kind.clone(), scope: scope.clone(), then: t.clone() };
                if !anchors.is_empty() {
                    a.anchors.entry(r.json().compact()).or_default().extend(anchors.iter().cloned());
                }
                products.entry(k).or_default().push(Product::Rule(r.clone()));
                rules_q.push(r.clone());
                rules.push(r);
                for m in &members {
                    let f = t.with_subject(m);
                    match f.mode() {
                        Some(md) => inv_q.push(Invariant { kind: md.to_string(), when: None, then: vec![f] }),
                        None => fs_q.push(f),
                    }
                }
            }
            for key in fs_q.iter().map(|f| f.json().compact()).chain(inv_q.iter().map(|i| i.json().compact())) {
                if !anchors.is_empty() {
                    a.anchors.entry(key.clone()).or_default().extend(anchors.iter().cloned());
                }
                a.derived.insert(key);
            }
            a.origins.push(Origin { label: label.to_string(), anchors: anchors.clone(), facts: fs_q.clone(), invariants: inv_q.clone(), rules: rules_q });
            facts.extend(fs_q);
            invariants.extend(inv_q);
        }
        a.facts = sort_unique(facts, Fact::json);
        a.invariants = sort_unique(invariants, Invariant::json);
        a.rules = sort_unique(rules, Rule::json);
        a.validate();
        a.check();
        let c = layers::check(&a);
        a.errors.extend(c.errors);
        a.warnings.extend(c.warnings);
        a.layers = c.from;
        a.warnings.extend(without_members);
        a.warnings.sort_by(|x, y| (&x.entity, &x.text).cmp(&(&y.entity, &y.text)));
        a.warnings.dedup_by(|x, y| x.entity == y.entity && x.text == y.text);
        // notes: one per canonical sentence of what the sentence produced
        let mut notes: Vec<Note> = notes_file.into_iter().map(|(order, file, text)| Note { sentence: None, file: Some(file), text, order }).collect();
        for (order, k, text) in notes_sentence {
            for p in products.get(&k).into_iter().flatten() {
                let lines = match p {
                    Product::Fact(f) => vec![canon::fact_en(&a, f)],
                    Product::Inv(i) => canon::invariant_en(&a, i),
                    Product::Rule(r) => vec![canon::rule_en(&a, r)],
                };
                notes.extend(lines.into_iter().map(|l| Note { sentence: Some(l), file: None, text: text.clone(), order }));
            }
        }
        // file order (ties broken by the json, to be deterministic); the first duplicate stays
        notes.sort_by_cached_key(|n| (n.order, n.json().compact()));
        let mut views = HashSet::new();
        notes.retain(|n| views.insert(n.json().compact()));
        a.notes = notes;
        a
    }

    /// Selection constraint on the subject in the rule template: the quantified type has to fit.
    fn check_rule(&self, kind: &str, ts: &[Fact]) -> Result<(), Er> {
        for f in ts {
            let Some(v) = self.lex.verb(f.rel()) else { continue };
            if let Some(at) = v.subject_kind.as_ref().filter(|x| !x.is_empty()) {
                if !at.iter().any(|x| self.lex.is_a(kind, x)) {
                    let msg = format!(
                        "'{}' does not accept a subject of type '{}' (the rule's 'every {}'); accepts subject: {}",
                        hints::name_verb(self.lex, f.rel()),
                        hints::name_kind(self.lex, kind),
                        hints::name_kind(self.lex, kind),
                        hints::list_kinds(self.lex, at)
                    );
                    return Err((msg, Some(hints::hint_kind(self.lex, v, Role::Subject, kind, None))));
                }
            }
        }
        Ok(())
    }

    /// Records an error with a one-line hint.
    fn error(&mut self, msg: String, hint: Option<String>) {
        if let Some(d) = hint {
            self.hints.insert(msg.clone(), d);
        }
        self.errors.push(msg);
    }

    /// The errors as `check` prints them: each on an `error ...` line with the hint right after.
    pub fn lines_of_error(&self) -> Vec<String> {
        let mut v = Vec::new();
        for e in &self.errors {
            v.push(format!("error {e}"));
            if let Some(d) = self.hints.get(e) {
                v.push(format!("  hint: {d}"));
            }
        }
        v
    }

    fn kind_of(&self, eid: &str) -> &str {
        self.entities.get(eid).map(|e| e.kind.as_str()).unwrap_or("")
    }

    pub fn name(&self, eid: &str) -> &str {
        self.entities.get(eid).map(|e| e.name.as_str()).unwrap_or("?")
    }

    /// Entities: apposition required; the same name with types in the same chain is the same entity.
    fn resolver<'q>(&mut self, syntactic: impl Iterator<Item = &'q Sentence>) {
        let mut by_name: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for q in syntactic {
            let mut ms = Vec::new();
            match q {
                Sentence::Cond { when, then, .. } => {
                    mentions(when, &mut ms);
                    mentions(then, &mut ms);
                }
                Sentence::Simple(o) => mentions(o, &mut ms),
                Sentence::All { scope, o, .. } => {
                    ms.push(scope);
                    // the rule subject is the marker, not an entity
                    mentions(o, &mut ms);
                    ms.remove(1);
                }
            }
            for (kind, name) in ms {
                by_name.entry(name.clone()).or_default().insert(kind.clone());
            }
            // states declared by a literal list, also inside a condition
            let clauses: Vec<&Clause> = match q {
                Sentence::Cond { when, then, .. } => vec![when, then],
                Sentence::Simple(o) | Sentence::All { o, .. } => vec![o],
            };
            for o in clauses {
                if let Clause::Rel { verb, literals: Some(ls), .. } = o {
                    if verb == "a.has_states" {
                        for s in ls {
                            by_name.entry(s.clone()).or_default().insert("p.state".into());
                        }
                    }
                }
            }
        }
        for (name, kinds) in &by_name {
            let mut groups: Vec<(String, Vec<String>)> = Vec::new();
            for kind in kinds {
                let root = self.lex.root(kind);
                match groups.iter_mut().find(|(r, _)| *r == root) {
                    Some(g) => g.1.push(kind.clone()),
                    None => groups.push((root, vec![kind.clone()])),
                }
            }
            for (root, kinds) in groups {
                // the first of the deepest
                let mut deepest = &kinds[0];
                for t in &kinds {
                    if self.lex.ancestors(t).len() > self.lex.ancestors(deepest).len() {
                        deepest = t;
                    }
                }
                if kinds.iter().any(|t| !self.lex.is_a(deepest, t)) {
                    self.errors.push(format!("'{name}': types {} are not in the same chain", repr_list(&kinds)));
                }
                let eid = ident(&root, name);
                self.entities.insert(eid.clone(), Entity { id: eid.clone(), name: name.clone(), kind: deepest.clone() });
                for t in &kinds {
                    self.key.insert((t.clone(), name.clone()), eid.clone());
                }
            }
        }
    }

    /// Roles the entity presupposes: the `requires` of the type chain and the unit, if the
    /// name suggests a quantity.
    fn roles_assumed(&self, kind: &str, name: &str) -> Vec<String> {
        let mut v: Vec<String> = Vec::new();
        for t in self.lex.ancestors(kind) {
            for r in self.lex.kind(&t).map(|t| t.requires.as_slice()).unwrap_or(&[]) {
                if !v.contains(&r.role) {
                    v.push(r.role.clone());
                }
            }
        }
        if self.lex.quantities.iter().any(|g| g.pt.iter().any(|n| n == name)) && !v.iter().any(|p| p == "unit") {
            v.push("unit".into());
        }
        v
    }

    fn absent(&self, eid: &str, role: &str) -> bool {
        self.facts.iter().any(|f| matches!(f, Fact::Absent { subject, role: p } if subject == eid && p == role))
    }

    fn ent(&self, np: &Np) -> String {
        self.key[np].clone()
    }

    /// Syntactic frame -> list of facts (coordination is distributed).
    fn semantics(&self, q: &Clause) -> Result<Vec<Fact>, Er> {
        self.semantics_of(q, None)
    }

    /// Like `semantics`, with the subject given from outside (id, type): the rule marker "*".
    fn semantics_of(&self, q: &Clause, given: Option<(&str, &str)>) -> Result<Vec<Fact>, Er> {
        let subject_of = |n: &Np| given.map(|(id, _)| id.to_string()).unwrap_or_else(|| self.ent(n));
        let (subject, verb, mode, obj, literals, roles_q) = match q {
            Clause::Adj { subject, adj } => return Ok(vec![Fact::Prop { subject: subject_of(subject), adj: adj.clone() }]),
            Clause::Abs { subject, absence } => {
                let e = subject_of(subject);
                let x = self.lex.absence(absence).expect("absence in the lexicon");
                let kind = given.map(|(_, t)| t).unwrap_or_else(|| self.kind_of(&e));
                let roles = self.roles_assumed(kind, &subject.1);
                if !roles.contains(&x.role) {
                    let list = if roles.is_empty() { "none".to_string() } else { roles.join(", ") };
                    let msg = format!(
                        "'{}' ({}) does not presuppose the role '{}' ('{}'); presupposed roles: {list}",
                        subject.1,
                        hints::name_kind(self.lex, kind),
                        x.role,
                        x.en.first().map(String::as_str).unwrap_or("?")
                    );
                    let hint = if roles.is_empty() {
                        "this type presupposes no role: there is no absence to declare".to_string()
                    } else {
                        let absences: Vec<String> = roles
                            .iter()
                            .filter_map(|p| self.lex.absence_of_role(p))
                            .filter_map(|x| x.en.first().map(|e| format!("'{e}'")))
                            .collect();
                        format!("only a role the type presupposes can be declared absent: {}", absences.join(", "))
                    };
                    return Err((msg, Some(hint)));
                }
                return Ok(vec![Fact::Absent { subject: e, role: x.role.clone() }]);
            }
            Clause::Cmp { subject, op, value } => {
                let value = match value {
                    ValueQ::Num(x) => Value::Num(*x),
                    ValueQ::Np(n) => Value::Ent(self.ent(n)),
                };
                let e = subject_of(subject);
                if value == Value::Ent(e.clone()) {
                    return Err((format!("comparison of '{}' with itself", subject.1), Some("compare with another value or with a number".to_string())));
                }
                return Ok(vec![Fact::Cmp { subject: e, op: op.clone(), value }]);
            }
            Clause::Rel { subject, verb, mode, obj, literals, roles } => (subject, verb, mode, obj, literals, roles),
        };
        let a = subject_of(subject);
        let v = self.lex.verb(verb).expect("verb in the lexicon");
        for (p, _) in roles_q {
            if v.prep_pt(p).is_none() {
                let (m, d) = hints::failure_prep(self.lex, verb, p);
                return Err((m, Some(d)));
            }
        }
        for p in &v.required {
            if !roles_q.iter().any(|(x, _)| x == p) {
                let (m, d) = hints::failure_role(self.lex, verb, p);
                return Err((m, Some(d)));
            }
        }
        let en = v.en.first().map(String::as_str).unwrap_or("?");
        let form_literal = format!("form: The <type> \"<name>\" {en} \"<literal>\"");
        let form_object = format!("form: The <type> \"<name>\" {en} the <type> \"<name>\"");
        match v.target.as_deref() {
            Some("literal" | "time") if !obj.is_empty() => {
                return Err((format!("'{en}' expects a quoted literal after the verb, found an object (the <type> \"<name>\")"), Some(form_literal)))
            }
            Some("literal" | "time") if literals.is_none() => return Err((format!("'{en}' requires a quoted literal after the verb"), Some(form_literal))),
            Some("entity") if literals.is_some() => {
                return Err((format!("'{en}' expects an object (the <type> \"<name>\") after the verb, found a quoted literal"), Some(form_object)))
            }
            Some("entity") if obj.is_empty() => return Err((format!("'{en}' requires an object (the <type> \"<name>\") after the verb"), Some(form_object))),
            _ => {}
        }
        let and_view = || self.lex.is_a(given.map(|(_, t)| t).unwrap_or_else(|| self.kind_of(&self.ent(subject))), "e.view");
        // `shows` and `groups by` also apply outside views, with their ordinary meaning
        if super::views::declared::VERBS.contains(&verb.as_str()) && (!matches!(verb.as_str(), "a.shows" | "a.groups_by") || and_view()) {
            let ls: &[String] = literals.as_deref().unwrap_or(&[]);
            if verb == "a.groups_by" && literals.is_none() {
                return Err((format!("'{en}' of a view expects a quoted relation after the verb"), Some(form_literal)));
            }
            super::views::declared::check_sentence(self.lex, verb, mode.as_deref(), ls)
                .map_err(|e| (format!("'{en}': {e}"), Some("see the literals a declared view accepts in FORMAT.md".to_string())))?;
        }
        let roles: BTreeMap<String, String> = roles_q.iter().map(|(p, o)| (p.clone(), self.ent(o))).collect();
        let base = |target: Target| Fact::Rel { rel: verb.clone(), subject: a.clone(), mode: mode.clone(), target, roles: roles.clone() };
        if let Some(ls) = literals {
            if verb == "a.has_states" {
                return Ok(ls.iter().map(|s| base(Target::Obj(Some(ident("p.state", s))))).collect());
            }
            if v.target.as_deref() == Some("time") {
                let mut fs = Vec::new();
                for s in ls {
                    let (canon, seg) = time::read_text(s).map_err(|e| {
                        (format!("'{en}': invalid time literal \"{s}\": {e}"), Some("a time is a number and a unit (ms, s, min, h, d) in quotes: \"30 s\", or a range \"1 s to 60 s\"".to_string()))
                    })?;
                    fs.push(base(Target::Time(canon, seg)));
                }
                return Ok(fs);
            }
            return Ok(ls.iter().map(|s| base(Target::Literal(s.clone()))).collect());
        }
        if obj.is_empty() {
            return Ok(vec![base(Target::Obj(None))]);
        }
        Ok(obj.iter().map(|o| base(Target::Obj(Some(self.ent(o))))).collect())
    }

    pub fn all_facts(&self) -> Vec<&Fact> {
        let mut v: Vec<&Fact> = self.facts.iter().collect();
        for i in &self.invariants {
            v.extend(i.when.iter().flatten());
            v.extend(i.then.iter());
        }
        v
    }

    /// Label of the first sentence that produced the fact (so the error cites the line), with `: `.
    fn locate(&self, f: &Fact) -> String {
        self.origins
            .iter()
            .find(|o| o.facts.contains(f) || o.invariants.iter().any(|i| i.then.contains(f) || i.when.iter().flatten().any(|x| x == f)))
            .map(|o| format!("{}: ", o.label))
            .unwrap_or_default()
    }

    fn validate(&mut self) {
        let mut errors: Vec<(String, Option<String>)> = Vec::new();
        for f in self.all_facts() {
            let Some(v) = self.lex.verb(f.rel()) else { continue };
            let en = hints::name_verb(self.lex, f.rel());
            if v.irreflexive && f.obj() == Some(f.subject()) {
                errors.push((
                    format!("{}'{en}' does not accept the same thing on both sides: \"{}\"", self.locate(f), self.name(f.subject())),
                    Some("use two different things (the same name with the same type is the same thing)".to_string()),
                ));
            }
            if let Some(at) = v.subject_kind.as_ref().filter(|x| !x.is_empty()) {
                let t = self.kind_of(f.subject());
                if !at.iter().any(|x| self.lex.is_a(t, x)) {
                    let msg = format!(
                        "{}'{en}' does not accept a subject of type '{}' (\"{}\"); accepts subject: {}",
                        self.locate(f),
                        hints::name_kind(self.lex, t),
                        self.name(f.subject()),
                        hints::list_kinds(self.lex, at)
                    );
                    let other = f.obj().map(|o| self.kind_of(o));
                    errors.push((msg, Some(hints::hint_kind(self.lex, v, Role::Subject, t, other))));
                }
            }
            if let (Some(ot), Some(o)) = (v.obj_kind.as_ref().filter(|x| !x.is_empty()), f.obj()) {
                let t = self.kind_of(o);
                if !ot.iter().any(|x| self.lex.is_a(t, x)) {
                    let msg = format!(
                        "{}'{en}' does not accept an object of type '{}' (\"{}\"); accepts object: {}",
                        self.locate(f),
                        hints::name_kind(self.lex, t),
                        self.name(o),
                        hints::list_kinds(self.lex, ot)
                    );
                    errors.push((msg, Some(hints::hint_kind(self.lex, v, Role::Object, t, Some(self.kind_of(f.subject()))))));
                }
            }
        }
        // transitions only between states declared by the machine itself
        let mut states: HashMap<Option<&str>, BTreeSet<&str>> = HashMap::new();
        for f in &self.facts {
            if f.rel() == "a.has_states" {
                if let Some(o) = f.obj() {
                    states.entry(Some(f.subject())).or_default().insert(o);
                }
            }
        }
        for f in &self.facts {
            if f.rel() == "a.moves" {
                for p in ["from", "to"] {
                    let Some(e) = f.role(p) else { continue };
                    if !states.get(&f.obj()).is_some_and(|s| s.contains(e)) {
                        let machine = f.obj().map(|m| self.name(m)).unwrap_or("?");
                        let declared: Vec<String> = states.get(&f.obj()).map(|s| s.iter().map(|x| format!("\"{}\"", self.name(x))).collect()).unwrap_or_default();
                        let list = if declared.is_empty() { "none".to_string() } else { declared.join(", ") };
                        errors.push((
                            format!("{}the transition uses the state \"{}\", which the state machine \"{machine}\" does not declare (declared states: {list})", self.locate(f), self.name(e)),
                            Some(format!("declare it first: The state machine \"{machine}\" has the states {}.", declared.iter().cloned().chain([format!("\"{}\"", self.name(e))]).collect::<Vec<_>>().join(", "))),
                        ));
                    }
                }
            }
        }
        for m in super::views::declared::check(self) {
            errors.push((m, None));
        }
        for (m, d) in errors {
            self.error(m, d);
        }
    }

    /// Gaps (presupposed roles left empty) and warnings derived from the lexicon.
    fn check(&mut self) {
        let lex = self.lex;
        let (mut gaps, mut warnings, mut errors) = (Vec::new(), Vec::new(), Vec::new());
        let mut ents: Vec<&Entity> = self.entities.values().collect();
        ents.sort_by(|a, b| (&a.kind, &a.name).cmp(&(&b.kind, &b.name)));
        for e in ents {
            for t in lex.ancestors(&e.kind) {
                let Some(kind) = lex.kind(&t) else { continue };
                for req in &kind.requires {
                    let extra = req.extra.as_deref();
                    let ok = self.facts.iter().any(|f| {
                        if req.verb == "prop" {
                            return match f {
                                Fact::Prop { subject, adj } => {
                                    *subject == e.id && extra.unwrap_or("").split(',').any(|x| x == adj)
                                }
                                // "is bounded by <value>" states the capacity by the value: it counts as bounded
                                Fact::Cmp { subject, op, .. } => {
                                    *subject == e.id && op == "<=" && extra.unwrap_or("").split(',').any(|x| x == "adj.bounded")
                                }
                                _ => false,
                            };
                        }
                        if f.rel() != req.verb {
                            return false;
                        }
                        match extra {
                            Some("inverse") => f.obj() == Some(e.id.as_str()),
                            Some(x) => f.subject() == e.id && f.obj().is_some_and(|o| lex.is_a(self.kind_of(o), x)),
                            None => f.subject() == e.id,
                        }
                    });
                    let absent = self.absent(&e.id, &req.role);
                    if ok && absent {
                        errors.push(format!("'{}': role '{}' declared absent and filled", e.name, req.role));
                    }
                    if !ok && !absent {
                        gaps.push(Gap { entity: e.id.clone(), role: req.role.clone(), kind: t.clone() });
                    }
                }
            }
        }
        // state machine: a state with no exit
        let mut outputs: HashMap<&str, BTreeSet<&str>> = HashMap::new();
        for f in &self.facts {
            if f.rel() == "a.moves" {
                if let (Some(from), Some(to)) = (f.role("from"), f.role("to")) {
                    outputs.entry(from).or_default().insert(to);
                }
            }
        }
        for f in &self.facts {
            if f.rel() == "a.has_states" {
                if let Some(o) = f.obj() {
                    if !outputs.contains_key(o) {
                        gaps.push(Gap { entity: o.into(), role: "outgoing transition".into(), kind: "p.state".into() });
                    }
                }
            }
        }
        // quantity by name x dimension of the declared unit
        let mut unit: HashMap<&str, &str> = HashMap::new();
        for f in &self.facts {
            if f.rel() == "a.has_unit" {
                if let Some(l) = f.literal() {
                    unit.insert(f.subject(), l);
                }
            }
        }
        for e in self.entities.values() {
            for g in &lex.quantities {
                if g.pt.contains(&e.name) {
                    let absent = self.absent(&e.id, "unit");
                    match unit.get(e.id.as_str()) {
                        None if absent => {}
                        None => gaps.push(Gap { entity: e.id.clone(), role: "unit".into(), kind: g.id.clone() }),
                        // the same message as the requires (clock, mesh) comes out once: errors are deduplicated
                        Some(_) if absent => errors.push(format!("'{}': role 'unit' declared absent and filled", e.name)),
                        Some(u) if lex.unit(u) != Some(g.dimension.as_str()) => warnings.push(Warning {
                            entity: e.id.clone(),
                            text: format!(
                                "the name suggests {} ({}), but the unit '{u}' is {}",
                                g.id,
                                g.dimension,
                                lex.unit(u).unwrap_or("unknown")
                            ),
                        }),
                        Some(_) => {}
                    }
                }
            }
        }
        // chained unit: whoever assumes a unit and reads something that inherits it from another entity
        let mut same: HashMap<&str, Option<&str>> = HashMap::new();
        for f in &self.facts {
            if f.rel() == "a.has_same_unit_as" && f.literal().is_none() {
                same.insert(f.subject(), f.obj());
            }
        }
        let resolver_unit = |e: &str| -> (Option<&str>, Option<String>) {
            let mut current = e.to_string();
            let mut seen: Vec<String> = Vec::new();
            loop {
                if let Some(u) = unit.get(current.as_str()) {
                    return (Some(u), Some(current));
                }
                match same.get(current.as_str()) {
                    Some(next) if !seen.contains(&current) => match next {
                        Some(p) => {
                            seen.push(current.clone());
                            current = p.to_string();
                        }
                        None => return (None, None),
                    },
                    _ => return (None, Some(current)),
                }
            }
        };
        for f in &self.facts {
            let (Some(o), Some(u1)) = (f.obj(), unit.get(f.subject())) else { continue };
            if !matches!(f.rel(), "a.reads" | "a.uses" | "a.consumes") {
                continue;
            }
            if !unit.contains_key(o) && !same.contains_key(o) {
                continue;
            }
            match resolver_unit(o) {
                (None, origin) => warnings.push(Warning {
                    entity: f.subject().into(),
                    text: format!(
                        "assumes '{u1}', but '{}' inherits the unit of '{}', which has no declared unit",
                        self.name(o),
                        origin.as_deref().map(|x| self.name(x)).unwrap_or("?")
                    ),
                }),
                (Some(u2), _) if u2 != *u1 => warnings.push(Warning {
                    entity: f.subject().into(),
                    text: format!("assumes '{u1}', but '{}' is in '{u2}'", self.name(o)),
                }),
                _ => {}
            }
        }
        gaps.sort_by(|a, b| (&a.entity, &a.role).cmp(&(&b.entity, &b.role)));
        warnings.sort_by(|a, b| (&a.entity, &a.text).cmp(&(&b.entity, &b.text)));
        self.gaps = gaps;
        self.warnings = warnings;
        errors.sort();
        errors.dedup();
        self.errors.extend(errors);
    }

    /// Anchors (without the `^`) of the sentences that produced the fact, sorted and deduplicated.
    pub fn anchors_of_fact(&self, f: &Fact) -> Vec<String> {
        self.anchors.get(&f.json().compact()).map(|s| s.iter().cloned().collect()).unwrap_or_default()
    }

    pub fn anchors_of_rule(&self, r: &Rule) -> Vec<String> {
        self.anchors.get(&r.json().compact()).map(|s| s.iter().cloned().collect()).unwrap_or_default()
    }

    pub fn anchors_of_invariant(&self, i: &Invariant) -> Vec<String> {
        self.anchors.get(&i.json().compact()).map(|s| s.iter().cloned().collect()).unwrap_or_default()
    }

    pub fn ir(&self) -> J {
        // the IR does not depend on sentence order: the (informal) notes come out in a fixed order by content
        let mut notes_ir: Vec<&Note> = self.notes.iter().collect();
        notes_ir.sort_by_cached_key(|n| n.json().compact());
        J::obj(vec![
            ("lexicon", J::s(&self.lex.version)),
            (
                "entities",
                J::Arr(
                    self.entities
                        .values()
                        .map(|e| J::obj(vec![("id", J::s(&e.id)), ("name", J::s(&e.name)), ("type", J::s(&e.kind)), ("attrs", J::Obj(vec![]))]))
                        .collect(),
                ),
            ),
            ("facts", J::Arr(self.facts.iter().map(|f| with_anchors(f.json(), self.anchors_of_fact(f))).collect())),
            ("invariants", J::Arr(self.invariants.iter().map(|i| with_anchors(i.json(), self.anchors_of_invariant(i))).collect())),
            ("rules", J::Arr(self.rules.iter().map(|r| with_anchors(r.json(), self.anchors_of_rule(r))).collect())),
            (
                "gaps",
                J::Arr(
                    self.gaps
                        .iter()
                        .map(|l| J::obj(vec![("entity", J::s(&l.entity)), ("role", J::s(&l.role)), ("type", J::s(&l.kind))]))
                        .collect(),
                ),
            ),
            (
                "warnings",
                J::Arr(self.warnings.iter().map(|a| J::obj(vec![("entity", J::s(&a.entity)), ("text", J::s(&a.text))])).collect()),
            ),
            ("errors", J::Arr(self.errors.iter().map(|e| J::s(e)).collect())),
            ("notes", J::Arr(notes_ir.iter().map(|n| n.json()).collect())),
        ])
    }

    /// Summary of the `check` result.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn summary(&self) -> String {
        self.summary_with(0, 0)
    }

    /// Summary with the file errors and warnings (globs, anchors, coverage) added in.
    pub fn summary_with(&self, errors: usize, warnings: usize) -> String {
        format!(
            "{} errors, {} entities, {} facts, {} invariants, {} rules, {} gaps, {} warnings",
            self.errors.len() + errors,
            self.entities.len(),
            self.facts.len(),
            self.invariants.len(),
            self.rules.len(),
            self.gaps.len(),
            self.warnings.len() + warnings
        )
    }
}
