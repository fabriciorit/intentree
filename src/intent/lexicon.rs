//! Lexicon of `.intent`, one entry per line: `=class id | field: value | ...`, lists with ", ",
//! maps with "key=value". The seed is embedded (`lexicon.txt`); `=` lines in the project's
//! `.intent` files extend it.

use super::json::repr_text;
use std::collections::{BTreeSet, HashMap};

pub const EMBEDDED: &str = include_str!("lexicon.txt");
const LABEL_EMBEDDED: &str = "lexicon.txt";
/// Values of the `target` field of a verb.
pub const TARGETS: [&str; 3] = ["literal", "entity", "time"];

/// Lexicon token: class and value (the lexicon value or the text/number read).
#[derive(Clone, Debug, PartialEq)]
pub enum Tok {
    Art,
    Cond(String),
    Neg,
    Alw,
    /// declared exception to the layer rule: `exceptionally`
    Exc,
    /// quantifier: `every`
    Every,
    And,
    Is,
    /// negated copula: `is not` (inequality)
    IsNot,
    Prep(String),
    Noun(String),
    Verb(String),
    /// negated form generated from a verb with `is ...`: `is not in`, `is not keyed by`
    VerbNeg(String),
    Adj(String),
    Cmp(String),
    /// declared absence of a presupposed role: the id of the `=absence` entry
    Abs(String),
    Name(String),
    Num(f64),
    Comma,
    End,
}

impl Tok {
    pub fn class(&self) -> &'static str {
        match self {
            Tok::Art => "ART",
            Tok::Cond(_) => "COND",
            Tok::Neg => "NEG",
            Tok::Alw => "ALW",
            Tok::Exc => "EXC",
            Tok::Every => "EVERY",
            Tok::And => "AND",
            Tok::Is => "IS",
            Tok::IsNot => "ISNOT",
            Tok::Prep(_) => "PREP",
            Tok::Noun(_) => "NOUN",
            Tok::Verb(_) => "VERB",
            Tok::VerbNeg(_) => "VNEG",
            Tok::Adj(_) => "ADJ",
            Tok::Cmp(_) => "CMP",
            Tok::Abs(_) => "AUS",
            Tok::Name(_) => "NAME",
            Tok::Num(_) => "NUM",
            Tok::Comma => "COMMA",
            Tok::End => "END",
        }
    }

    /// Tuple rendering used in messages: `('NOUN', 'e.module')`.
    pub fn repr(&self) -> String {
        let v = match self {
            Tok::Cond(s) | Tok::Prep(s) | Tok::Noun(s) | Tok::Verb(s) | Tok::VerbNeg(s) | Tok::Adj(s) | Tok::Cmp(s) | Tok::Abs(s) | Tok::Name(s) => repr_text(s),
            _ => "None".into(),
        };
        format!("('{}', {v})", self.class())
    }
}

#[derive(Clone, Debug)]
pub struct Requires {
    pub role: String,
    pub verb: String,
    pub extra: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct Kind {
    pub id: String,
    pub en: Vec<String>,
    pub pt: String,
    pub g: String,
    pub layer: Option<String>,
    pub structural: bool,
    pub parent: Option<String>,
    pub requires: Vec<Requires>,
    /// words a writer tries that are not approved forms: the check suggests them as
    /// "roughly this entry" (they do not change what is accepted)
    pub syn: Vec<String>,
    pub origin: String,
}

#[derive(Clone, Debug, Default)]
pub struct Verb {
    pub id: String,
    pub en: Vec<String>,
    pub pt: String,
    pub preps: Vec<(String, String)>,
    pub required: Vec<String>,
    pub subject_kind: Option<Vec<String>>,
    pub obj_kind: Option<Vec<String>>,
    pub irreflexive: bool,
    pub list_literal: bool,
    pub pt_intransitive: Option<String>,
    /// what follows the verb: `literal` (in quotes), `entity` (the <type> "<name>") or `time`
    /// (a time literal); without the field, either one or nothing
    pub target: Option<String>,
    /// as in `Kind`
    pub syn: Vec<String>,
    pub origin: String,
}

impl Verb {
    /// The Portuguese preposition for `p`, if the verb accepts `p` (the English word itself when
    /// the entry gives no Portuguese form).
    pub fn prep_pt(&self, p: &str) -> Option<&str> {
        self.preps.iter().find(|(k, _)| k == p).map(|(k, v)| if v.is_empty() { k.as_str() } else { v.as_str() })
    }

    /// The Portuguese form, or the first English form when the entry has none.
    pub fn label_pt(&self) -> &str {
        if self.pt.is_empty() {
            self.en.first().map(String::as_str).unwrap_or("?")
        } else {
            &self.pt
        }
    }
}

impl Kind {
    /// The Portuguese label, or the first English form when the entry has none.
    pub fn label_pt(&self) -> &str {
        if self.pt.is_empty() {
            self.en.first().map(String::as_str).unwrap_or("?")
        } else {
            &self.pt
        }
    }

    /// The grammatical gender for the Portuguese renderer ("m" when unspecified).
    pub fn gender(&self) -> &str {
        if self.g.is_empty() {
            "m"
        } else {
            &self.g
        }
    }
}

impl Adjective {
    /// The Portuguese form for a gender ("m" or "f"), or the first English form when the entry
    /// has none.
    pub fn form_pt(&self, g: &str) -> &str {
        match self.pt.get(if g == "m" { 0 } else { 1 }) {
            Some(f) => f,
            None => self.en.first().map(String::as_str).unwrap_or("?"),
        }
    }
}

impl Absence {
    /// The Portuguese phrase, or the first English form when the entry has none.
    pub fn label_pt(&self) -> &str {
        if self.pt.is_empty() {
            self.en.first().map(String::as_str).unwrap_or("?")
        } else {
            &self.pt
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Adjective {
    pub id: String,
    pub en: Vec<String>,
    pub pt: Vec<String>,
    pub be: bool,
    /// as in `Kind`
    pub syn: Vec<String>,
    pub origin: String,
}

/// Declared absence: the sentence that fills a presupposed role with "none, on purpose".
/// `role` is the role name in the `requires` of the types (or "unit", of a quantity).
#[derive(Clone, Debug, Default)]
pub struct Absence {
    pub id: String,
    pub role: String,
    pub en: Vec<String>,
    pub pt: String,
    pub origin: String,
}

#[derive(Clone, Debug, Default)]
pub struct Quantity {
    pub id: String,
    pub pt: Vec<String>,
    pub dimension: String,
    pub origin: String,
}

#[derive(Clone, Debug, Default)]
pub struct Lexicon {
    pub version: String,
    pub article: Vec<String>,
    pub conditional: Vec<(String, String)>,
    pub negation: Vec<String>,
    pub universal: Vec<String>,
    pub exception: Vec<String>,
    pub quantifier: Vec<String>,
    pub coordination: Vec<String>,
    pub preposition: Vec<String>,
    pub copula: Vec<String>,
    pub copula_negated: Vec<String>,
    pub kinds: Vec<Kind>,
    pub verbs: Vec<Verb>,
    pub adjectives: Vec<Adjective>,
    /// Phrases of the comparison `c.limit`, in lexicon order: phrase -> operator.
    pub comparisons: Vec<(String, String)>,
    pub absences: Vec<Absence>,
    pub quantities: Vec<Quantity>,
    pub units: Vec<(String, String)>,
    idx_kind: HashMap<String, usize>,
    idx_verb: HashMap<String, usize>,
    idx_adj: HashMap<String, usize>,
    /// English phrase -> token
    pub sentences: HashMap<String, Tok>,
    /// Base form of a verb that is also a type name (`record`: type and base of `records`).
    /// Kept out of `sentences`; the tokenizer reads it as a verb only right after the negation
    /// (`does not record`). Phrase -> verb id.
    pub bases_ambiguous: HashMap<String, String>,
    /// largest number of words in a lexicon phrase
    pub larger: usize,
}

/// Raw `=` line with its origin label (`file:line`).
pub struct Entry<'a> {
    pub label: String,
    pub text: &'a str,
}

fn list(v: &str) -> Vec<String> {
    v.split(", ").map(str::trim).filter(|x| !x.is_empty()).map(String::from).collect()
}

fn map(v: &str) -> Result<Vec<(String, String)>, String> {
    list(v)
        .into_iter()
        .map(|x| match x.split_once('=') {
            Some((k, v)) => Ok((k.trim().to_string(), v.trim().to_string())),
            None => Err(format!("expected key=value, found '{x}'")),
        })
        .collect()
}

impl Lexicon {
    /// The embedded seed only.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn embedded() -> Result<Lexicon, Vec<String>> {
        Lexicon::load(&[])
    }

    /// The embedded seed plus the project entries (`=` lines of the `.intent` files).
    pub fn load(extensions: &[Entry]) -> Result<Lexicon, Vec<String>> {
        let mut lex = Lexicon::default();
        let mut errors = Vec::new();
        for (n, line) in EMBEDDED.lines().enumerate() {
            let s = line.trim();
            if s.is_empty() || s.starts_with('#') {
                continue;
            }
            if let Err(e) = lex.entry(s, &format!("{LABEL_EMBEDDED}:{}", n + 1), true) {
                errors.push(format!("{LABEL_EMBEDDED}:{}: {e}", n + 1));
            }
        }
        for e in extensions {
            if let Err(m) = lex.entry(e.text.trim(), &e.label, false) {
                errors.push(format!("{}: {m}", e.label));
            }
        }
        if errors.is_empty() {
            errors.extend(lex.build_sentences());
        }
        if errors.is_empty() {
            errors.extend(lex.check());
        }
        if errors.is_empty() {
            Ok(lex)
        } else {
            Err(errors)
        }
    }

    fn entry(&mut self, s: &str, label: &str, seed: bool) -> Result<(), String> {
        let mut parts = s.split(" | ");
        let hdr = parts.next().unwrap_or("");
        let hdr = hdr.strip_prefix('=').ok_or("a lexicon entry starts with '='")?;
        let (class, id) = hdr.split_once(' ').map(|(a, b)| (a, b.trim())).unwrap_or((hdr, ""));
        if id.is_empty() || id.contains(' ') {
            return Err(format!("entry '{class}' requires an id without spaces"));
        }
        let mut fields: Vec<(String, Option<String>)> = Vec::new();
        for c in parts {
            let c = c.trim();
            match c.split_once(':') {
                Some((k, v)) => fields.push((k.trim().to_string(), Some(v.trim().to_string()))),
                None => fields.push((c.to_string(), None)),
            }
        }
        let seed_only = |c: &str| -> Result<(), String> {
            if seed {
                Ok(())
            } else {
                Err(format!("'={c}' only exists in the embedded lexicon"))
            }
        };
        let repeated = |exists: bool| -> Result<(), String> {
            if exists {
                Err(format!("duplicate id: {id}"))
            } else {
                Ok(())
            }
        };
        let value = |k: &str, v: &Option<String>| -> Result<String, String> {
            v.clone().ok_or_else(|| format!("field '{k}' requires a value"))
        };
        let flag = |k: &str, v: &Option<String>| -> Result<bool, String> {
            match v {
                None => Ok(true),
                Some(_) => Err(format!("field '{k}' takes no value")),
            }
        };
        let unknown = |k: &str| Err(format!("unknown field in {class}: {k}"));
        match class {
            "version" => {
                seed_only(class)?;
                self.version = id.to_string();
            }
            "core" => {
                seed_only(class)?;
                if id == "conditional" {
                    for (k, v) in &fields {
                        self.conditional.push((k.clone(), value(k, v)?));
                    }
                    return Ok(());
                }
                let mut en = Vec::new();
                for (k, v) in &fields {
                    match k.as_str() {
                        "en" => en = list(&value(k, v)?),
                        _ => return unknown(k),
                    }
                }
                match id {
                    "article" => self.article = en,
                    "negation" => self.negation = en,
                    "universal" => self.universal = en,
                    "exception" => self.exception = en,
                    "quantifier" => self.quantifier = en,
                    "coordination" => self.coordination = en,
                    "preposition" => self.preposition = en,
                    "copula" => self.copula = en,
                    "negated_copula" => self.copula_negated = en,
                    _ => return Err(format!("unknown core entry: {id}")),
                }
            }
            "comparison" => {
                seed_only(class)?;
                if id != "c.limit" {
                    return Err(format!("unknown comparison: {id}"));
                }
                for (k, v) in &fields {
                    self.comparisons.push((k.clone(), value(k, v)?));
                }
            }
            "type" => {
                repeated(self.idx_kind.contains_key(id))?;
                let mut t = Kind { id: id.into(), origin: label.into(), ..Default::default() };
                for (k, v) in &fields {
                    match k.as_str() {
                        "en" => t.en = list(&value(k, v)?),
                        "pt" => t.pt = value(k, v)?,
                        "g" => t.g = value(k, v)?,
                        "layer" => t.layer = Some(value(k, v)?),
                        "parent" => t.parent = Some(value(k, v)?),
                        "syn" => t.syn = list(&value(k, v)?),
                        "structural" => t.structural = flag(k, v)?,
                        "requires" => {
                            for (role, r) in map(&value(k, v)?)? {
                                let mut it = r.split_whitespace();
                                let verb = it.next().ok_or_else(|| format!("requires '{role}' without a verb"))?.to_string();
                                let extra = it.next().map(String::from);
                                if it.next().is_some() {
                                    return Err(format!("requires '{role}': expected 'verb [extra]'"));
                                }
                                t.requires.push(Requires { role, verb, extra });
                            }
                        }
                        _ => return unknown(k),
                    }
                }
                self.idx_kind.insert(id.into(), self.kinds.len());
                self.kinds.push(t);
            }
            "verb" => {
                repeated(self.idx_verb.contains_key(id))?;
                let mut vb = Verb { id: id.into(), origin: label.into(), ..Default::default() };
                for (k, v) in &fields {
                    match k.as_str() {
                        "en" => vb.en = list(&value(k, v)?),
                        "pt" => vb.pt = value(k, v)?,
                        "preps" => vb.preps = map(&value(k, v)?)?,
                        "required" => vb.required = list(&value(k, v)?),
                        "subject_type" => vb.subject_kind = Some(list(&value(k, v)?)),
                        "object_type" => vb.obj_kind = Some(list(&value(k, v)?)),
                        "irreflexive" => vb.irreflexive = flag(k, v)?,
                        "literal_list" => vb.list_literal = flag(k, v)?,
                        "pt_intransitivo" => vb.pt_intransitive = Some(value(k, v)?),
                        "target" => vb.target = Some(value(k, v)?),
                        "syn" => vb.syn = list(&value(k, v)?),
                        _ => return unknown(k),
                    }
                }
                self.idx_verb.insert(id.into(), self.verbs.len());
                self.verbs.push(vb);
            }
            "adjective" => {
                repeated(self.idx_adj.contains_key(id))?;
                let mut a = Adjective { id: id.into(), origin: label.into(), ..Default::default() };
                for (k, v) in &fields {
                    match k.as_str() {
                        "en" => a.en = list(&value(k, v)?),
                        "pt" => a.pt = list(&value(k, v)?),
                        "pt_estar" => a.be = flag(k, v)?,
                        "syn" => a.syn = list(&value(k, v)?),
                        _ => return unknown(k),
                    }
                }
                self.idx_adj.insert(id.into(), self.adjectives.len());
                self.adjectives.push(a);
            }
            "absence" => {
                repeated(self.absences.iter().any(|x| x.id == id))?;
                let mut x = Absence { id: id.into(), origin: label.into(), ..Default::default() };
                for (k, v) in &fields {
                    match k.as_str() {
                        "role" => x.role = value(k, v)?,
                        "en" => x.en = list(&value(k, v)?),
                        "pt" => x.pt = value(k, v)?,
                        _ => return unknown(k),
                    }
                }
                self.absences.push(x);
            }
            "quantity" => {
                repeated(self.quantities.iter().any(|g| g.id == id))?;
                let mut g = Quantity { id: id.into(), origin: label.into(), ..Default::default() };
                for (k, v) in &fields {
                    match k.as_str() {
                        "names" => g.pt = list(&value(k, v)?),
                        "dimension" => g.dimension = value(k, v)?,
                        _ => return unknown(k),
                    }
                }
                self.quantities.push(g);
            }
            "unit" => {
                repeated(self.units.iter().any(|(u, _)| u == id))?;
                let mut dim = None;
                for (k, v) in &fields {
                    match k.as_str() {
                        "dimension" => dim = Some(value(k, v)?),
                        _ => return unknown(k),
                    }
                }
                self.units.push((id.into(), dim.ok_or("unit requires 'dimension'")?));
            }
            _ => return Err(format!("unknown class: {class}")),
        }
        Ok(())
    }

    /// English phrases -> token. One phrase, one class.
    fn build_sentences(&mut self) -> Vec<String> {
        let mut pairs: Vec<(String, Tok)> = Vec::new();
        for w in &self.article {
            pairs.push((w.clone(), Tok::Art));
        }
        for (w, _) in &self.conditional {
            pairs.push((w.clone(), Tok::Cond(w.clone())));
        }
        for w in &self.negation {
            pairs.push((w.clone(), Tok::Neg));
        }
        for w in &self.universal {
            pairs.push((w.clone(), Tok::Alw));
        }
        for w in &self.exception {
            pairs.push((w.clone(), Tok::Exc));
        }
        for w in &self.quantifier {
            pairs.push((w.clone(), Tok::Every));
        }
        for w in &self.coordination {
            pairs.push((w.clone(), Tok::And));
        }
        for w in &self.copula {
            pairs.push((w.clone(), Tok::Is));
        }
        for w in &self.copula_negated {
            pairs.push((w.clone(), Tok::IsNot));
        }
        for p in &self.preposition {
            pairs.push((p.clone(), Tok::Prep(p.clone())));
        }
        for t in &self.kinds {
            for f in &t.en {
                pairs.push((f.clone(), Tok::Noun(t.id.clone())));
            }
        }
        for v in &self.verbs {
            for (k, f) in v.en.iter().enumerate() {
                // base form (the second) equal to a type name: only valid after the negation
                if k >= 1 && self.kinds.iter().any(|t| t.en.contains(f)) {
                    self.bases_ambiguous.insert(f.clone(), v.id.clone());
                    continue;
                }
                pairs.push((f.clone(), Tok::Verb(v.id.clone())));
            }
            // base form of a 'has' verb with a single form ('has the unit'): the one that follows
            // 'does not' ("does not have the unit"); the canonical text writes it (`canon::form_base`)
            if v.en.len() == 1 {
                if let Some(rest) = v.en[0].strip_prefix("has ") {
                    pairs.push((format!("have {rest}"), Tok::Verb(v.id.clone())));
                }
            }
            // a verb with a copula ('is in', 'is keyed by') negates with 'is not ...', as in English
            for f in v.en.iter().filter_map(|f| f.strip_prefix("is ")) {
                for neg in &self.copula_negated {
                    pairs.push((format!("{neg} {f}"), Tok::VerbNeg(v.id.clone())));
                }
            }
        }
        for a in &self.adjectives {
            for f in &a.en {
                pairs.push((f.clone(), Tok::Adj(a.id.clone())));
            }
        }
        for (f, op) in &self.comparisons {
            pairs.push((f.clone(), Tok::Cmp(op.clone())));
        }
        for x in &self.absences {
            for f in &x.en {
                pairs.push((f.clone(), Tok::Abs(x.id.clone())));
            }
        }
        let mut errors = Vec::new();
        for (f, tok) in pairs {
            if let Some(old) = self.sentences.get(&f) {
                if *old != tok {
                    errors.push(format!("lexicon: '{f}' has two classes: {} and {}", old.repr(), tok.repr()));
                }
                continue;
            }
            self.sentences.insert(f, tok);
        }
        self.larger = self.sentences.keys().map(|f| f.split_whitespace().count()).max().unwrap_or(1);
        errors
    }

    /// Static checks: references, cycles, Portuguese forms.
    pub fn check(&self) -> Vec<String> {
        let mut errors = Vec::new();
        let preps: BTreeSet<&str> = self.preposition.iter().map(String::as_str).collect();
        for t in &self.kinds {
            let scope = |m: String| format!("{}: type {}: {m}", t.origin, t.id);
            if t.en.is_empty() {
                errors.push(scope("no English form".into()));
            }
            if !t.g.is_empty() && t.g != "m" && t.g != "f" {
                errors.push(scope(format!("gender '{}' (expected m or f)", t.g)));
            }
            if let Some(p) = &t.parent {
                if !self.idx_kind.contains_key(p) {
                    errors.push(scope(format!("unknown parent: {p}")));
                }
            }
            // cycle: climb the chain without repeating
            let mut seen = vec![t.id.as_str()];
            let mut current = t;
            while let Some(p) = &current.parent {
                if seen.contains(&p.as_str()) {
                    errors.push(scope(format!("subtype cycle: {} -> {p}", seen.join(" -> "))));
                    break;
                }
                seen.push(p);
                match self.kind(p) {
                    Some(x) => current = x,
                    None => break,
                }
            }
            for r in &t.requires {
                match r.verb.as_str() {
                    "prop" => {
                        let adjs = r.extra.as_deref().unwrap_or("");
                        if adjs.is_empty() {
                            errors.push(scope(format!("requires '{}': prop without adjectives", r.role)));
                        }
                        for a in adjs.split(',').filter(|a| !a.is_empty()) {
                            if !self.idx_adj.contains_key(a) {
                                errors.push(scope(format!("requires '{}': unknown adjective: {a}", r.role)));
                            }
                        }
                    }
                    "c.limit" => {
                        if self.comparisons.is_empty() {
                            errors.push(scope(format!("requires '{}': c.limit without comparisons", r.role)));
                        }
                    }
                    v => {
                        if !self.idx_verb.contains_key(v) {
                            errors.push(scope(format!("requires '{}': unknown verb: {v}", r.role)));
                        }
                        if let Some(x) = &r.extra {
                            if x != "inverse" && !self.idx_kind.contains_key(x) {
                                errors.push(scope(format!("requires '{}': unknown type: {x}", r.role)));
                            }
                        }
                    }
                }
            }
        }
        for v in &self.verbs {
            let scope = |m: String| format!("{}: verb {}: {m}", v.origin, v.id);
            if v.en.is_empty() {
                errors.push(scope("no English form".into()));
            }
            for (p, _) in &v.preps {
                if !preps.contains(p.as_str()) {
                    errors.push(scope(format!("preposition outside the core: {p}")));
                }
            }
            for p in &v.required {
                if v.prep_pt(p).is_none() {
                    errors.push(scope(format!("required role without a preposition: {p}")));
                }
            }
            if let Some(x) = &v.target {
                if !TARGETS.contains(&x.as_str()) {
                    errors.push(scope(format!("target '{x}' (expected {})", TARGETS.join(", "))));
                }
                if v.list_literal && x != "literal" {
                    errors.push(scope("literal_list requires target literal".into()));
                }
            }
            for t in v.subject_kind.iter().chain(v.obj_kind.iter()).flatten() {
                if !self.idx_kind.contains_key(t) {
                    errors.push(scope(format!("unknown type: {t}")));
                }
            }
        }
        // a synonym that is already an approved form is a mistake, unless the form came later,
        // from a project extension (approved by the human) over a seed synonym
        let syns = self
            .kinds
            .iter()
            .map(|t| (&t.id, &t.syn, &t.origin))
            .chain(self.verbs.iter().map(|v| (&v.id, &v.syn, &v.origin)))
            .chain(self.adjectives.iter().map(|a| (&a.id, &a.syn, &a.origin)));
        for (id, syn, origin) in syns {
            for w in syn {
                let Some(t) = self.sentences.get(&w.to_lowercase()) else { continue };
                let of_form = match t {
                    Tok::Noun(i) => self.kind(i).map(|x| x.origin.as_str()),
                    Tok::Verb(i) | Tok::VerbNeg(i) => self.verb(i).map(|x| x.origin.as_str()),
                    Tok::Adj(i) => self.adjective(i).map(|x| x.origin.as_str()),
                    _ => None,
                };
                let seed = |o: &str| o.starts_with(LABEL_EMBEDDED);
                if !(seed(origin) && of_form.is_some_and(|o| !seed(o))) {
                    errors.push(format!("{id}: synonym '{w}' is already an approved form"));
                }
            }
        }
        for a in &self.adjectives {
            if a.en.is_empty() || !(a.pt.is_empty() || a.pt.len() == 2) {
                errors.push(format!("{}: adjective {}: requires en forms and, if pt is given, two pt forms (m, f)", a.origin, a.id));
            }
        }
        for x in &self.absences {
            let scope = |m: String| format!("{}: absence {}: {m}", x.origin, x.id);
            if x.en.is_empty() {
                errors.push(scope("requires en forms".into()));
            }
            let exists = x.role == "unit" || self.kinds.iter().any(|t| t.requires.iter().any(|r| r.role == x.role));
            if !exists {
                errors.push(scope(format!("role '{}' is not presupposed by any type", x.role)));
            }
            if self.absences.iter().filter(|y| y.role == x.role).count() > 1 {
                errors.push(scope(format!("role '{}' has more than one absence", x.role)));
            }
        }
        for g in &self.quantities {
            if g.pt.is_empty() || g.dimension.is_empty() {
                errors.push(format!("{}: quantity {}: requires names and dimension", g.origin, g.id));
            }
        }
        if self.version.is_empty() {
            errors.push(format!("{LABEL_EMBEDDED}: no version"));
        }
        if self.article.is_empty() || self.coordination.is_empty() || self.copula.is_empty() {
            errors.push(format!("{LABEL_EMBEDDED}: incomplete core"));
        }
        errors
    }

    /// Phrase overlap (static check of `lexicon check`): a phrase that is a prefix of another of
    /// a different class changes tokenization if the rest of the longer one also reads as tokens
    /// and the short sequence is grammatical. E.g. with 'zaps' (a verb that accepts 'to') and
    /// 'zaps to' (another verb), "X zaps to the ..." would have two readings; the longest match
    /// would hide the other.
    pub fn overlaps(&self) -> Vec<String> {
        let mut out = Vec::new();
        let mut sentences: Vec<(&String, &Tok)> = self.sentences.iter().collect();
        sentences.sort_by(|a, b| a.0.cmp(b.0));
        for (short, tc) in &sentences {
            for (long, tl) in &sentences {
                let Some(rest) = long.strip_prefix(short.as_str()).and_then(|r| r.strip_prefix(' ')) else { continue };
                if tc == tl {
                    continue;
                }
                let Some(toks) = self.segment(rest) else { continue };
                let mut chain = vec![(*tc).clone()];
                chain.extend(toks.iter().map(|(_, t)| t.clone()));
                if chain.windows(2).all(|w| self.can_follow(&w[0], &w[1])) {
                    let parts: Vec<String> = toks.iter().map(|(f, t)| format!("'{f}' {}", t.repr())).collect();
                    out.push(format!("overlap: '{long}' {} also reads '{short}' {} + {}", tl.repr(), tc.repr(), parts.join(" + ")));
                }
            }
        }
        out
    }

    /// Reads the text (words only) as lexicon tokens, by longest match.
    fn segment<'t>(&self, text: &'t str) -> Option<Vec<(String, Tok)>> {
        let ws: Vec<&str> = text.split(' ').collect();
        let mut i = 0;
        let mut out = Vec::new();
        while i < ws.len() {
            let n = (1..=self.larger.min(ws.len() - i)).rev().find(|&n| self.sentences.contains_key(&ws[i..i + n].join(" ")))?;
            let f = ws[i..i + n].join(" ");
            out.push((f.clone(), self.sentences[&f].clone()));
            i += n;
        }
        Some(out)
    }

    /// Whether a token may directly follow another in the grammar (pairs of the regular grammar;
    /// a verb followed by a preposition only if the verb accepts it; a noun followed by 'in'
    /// only in the quantifier).
    fn can_follow(&self, a: &Tok, b: &Tok) -> bool {
        let c = b.class();
        match a {
            Tok::Art => c == "NOUN",
            Tok::Noun(_) => c == "NAME" || *b == Tok::Prep("in".into()),
            Tok::Name(_) => matches!(c, "CMP" | "IS" | "ISNOT" | "AUS" | "NEG" | "ALW" | "EXC" | "VERB" | "VNEG" | "COMMA" | "AND" | "PREP" | "END"),
            Tok::Cond(_) | Tok::Prep(_) => c == "ART",
            Tok::Every => c == "NOUN",
            Tok::Neg | Tok::Alw | Tok::Exc => c == "VERB",
            Tok::Is => matches!(c, "ADJ" | "ART" | "NUM"),
            Tok::IsNot | Tok::Cmp(_) => matches!(c, "ART" | "NUM"),
            Tok::Abs(_) | Tok::Adj(_) | Tok::Num(_) => matches!(c, "END" | "COMMA"),
            Tok::Verb(v) | Tok::VerbNeg(v) => match b {
                Tok::Prep(p) => self.verb(v).is_some_and(|x| x.prep_pt(p).is_some()),
                _ => matches!(c, "ART" | "NAME" | "END" | "COMMA"),
            },
            Tok::And | Tok::Comma => matches!(c, "ART" | "NAME"),
            Tok::End => false,
        }
    }

    pub fn kind(&self, id: &str) -> Option<&Kind> {
        self.idx_kind.get(id).map(|&i| &self.kinds[i])
    }

    pub fn verb(&self, id: &str) -> Option<&Verb> {
        self.idx_verb.get(id).map(|&i| &self.verbs[i])
    }

    pub fn adjective(&self, id: &str) -> Option<&Adjective> {
        self.idx_adj.get(id).map(|&i| &self.adjectives[i])
    }

    pub fn absence(&self, id: &str) -> Option<&Absence> {
        self.absences.iter().find(|x| x.id == id)
    }

    pub fn absence_of_role(&self, role: &str) -> Option<&Absence> {
        self.absences.iter().find(|x| x.role == role)
    }

    pub fn unit(&self, u: &str) -> Option<&str> {
        self.units.iter().find(|(x, _)| x == u).map(|(_, d)| d.as_str())
    }

    /// Subtype chain, from the type itself to the root.
    pub fn ancestors(&self, tid: &str) -> Vec<String> {
        let mut chain = vec![tid.to_string()];
        while let Some(p) = self.kind(chain.last().unwrap()).and_then(|t| t.parent.clone()) {
            if chain.contains(&p) {
                break;
            }
            chain.push(p);
        }
        chain
    }

    pub fn root(&self, tid: &str) -> String {
        self.ancestors(tid).pop().unwrap()
    }

    pub fn is_a(&self, tid: &str, target: &str) -> bool {
        self.ancestors(tid).iter().any(|t| t == target)
    }

    /// Compact list for the writer (`intentree lexicon`).
    pub fn list_entries(&self) -> String {
        let mut out = vec!["# types (nouns)".to_string()];
        let mut kinds: Vec<&Kind> = self.kinds.iter().collect();
        kinds.sort_by(|a, b| a.id.cmp(&b.id));
        for t in kinds {
            let req = if t.requires.is_empty() {
                String::new()
            } else {
                format!(" requires: {}", t.requires.iter().map(|r| r.role.as_str()).collect::<Vec<_>>().join("; "))
            };
            out.push(format!("{}{req}", t.en.join(" | ")));
        }
        out.push("# verbs (accepted prepositions in brackets)".into());
        let mut verbs: Vec<&Verb> = self.verbs.iter().collect();
        verbs.sort_by(|a, b| a.id.cmp(&b.id));
        for v in verbs {
            let preps = if v.preps.is_empty() {
                String::new()
            } else {
                format!(" [{}]", v.preps.iter().map(|(p, _)| p.as_str()).collect::<Vec<_>>().join(", "))
            };
            out.push(format!("{}{preps}", v.en.join(" | ")));
        }
        out.push("# adjectives (after 'is')".into());
        out.push(self.adjectives.iter().flat_map(|a| a.en.iter().map(String::as_str)).collect::<Vec<_>>().join(" | "));
        out.push("# comparisons (followed by a number or the <type> \"<name>\"); equality: is, is not".into());
        out.push(self.comparisons.iter().map(|(f, _)| f.as_str()).collect::<Vec<_>>().join(" | "));
        out.push("# core: mode before the verb, quantifier, copula".into());
        out.push(
            [&self.negation, &self.universal, &self.exception, &self.quantifier, &self.copula, &self.copula_negated]
                .iter()
                .flat_map(|v| v.iter().map(String::as_str))
                .collect::<Vec<_>>()
                .join(" | "),
        );
        out.push("# declared absences (presupposed role left empty on purpose)".into());
        for x in &self.absences {
            out.push(format!("{} ({})", x.en.join(" | "), x.role));
        }
        out.join("\n") + "\n"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_seed_loads_and_passes_the_checks() {
        let l = Lexicon::embedded().expect("valid seed");
        assert_eq!(l.version, "0.3");
        assert!(l.check().is_empty());
        assert_eq!(l.sentences.get("state machine"), Some(&Tok::Noun("p.state_machine".into())));
        assert_eq!(l.sentences.get("is ordered by"), Some(&Tok::Verb("a.ordered_by".into())));
        assert_eq!(l.sentences.get("is"), Some(&Tok::Is));
        assert_eq!(l.verb("a.moves").and_then(|v| v.subject_kind.clone()), Some(vec![]));
        assert_eq!(l.ancestors("m.percentile"), ["m.percentile", "e.metric"]);
    }
}
