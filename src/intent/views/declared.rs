//! Views declared in the language itself: four parameters (selection, grouping, form,
//! annotations), each given by sentences `The view "<name>" <verb> ...` with the lexicon's tool
//! words. The grammar stays regular: the view is an entity of type `e.view` and the parameters
//! are relations of it, checked here.
//!
//! ```text
//! The view "energy" shows the key-value store "Store".       selection: entities
//! The view "energy" shows every "thread".                    selection: all of a type
//! The view "deployment" groups by "runs on".                 grouping: by a relation
//! The view "deployment" has the form "boxes".                form: boxes, matrix or list
//! The view "deployment" marks "gaps" and "invariants".       annotations (without the sentence: all)
//! ```

use super::super::analysis::{Analysis, Fact};
use super::super::lexicon::{Lexicon, Tok};
use std::collections::BTreeSet;

/// the general lexicon `shows`: with a subject of type view, it selects entities
pub const SHOW: &str = "a.shows";
pub const SHOW_ALL: &str = "a.shows_every";
pub const GROUP: &str = "a.groups_by";
pub const FORM: &str = "a.has_form";
pub const MARK: &str = "a.marks";

pub const VERBS: [&str; 5] = [SHOW, SHOW_ALL, GROUP, FORM, MARK];

pub const FORMS: [&str; 3] = ["boxes", "matrix", "list"];
pub const MARKS: [&str; 4] = ["gaps", "invariants", "notes", "states"];

/// Lowercase and without accents: `Café` -> `cafe`.
pub fn norm(s: &str) -> String {
    s.chars()
        .flat_map(|c| c.to_lowercase())
        .map(|c| match c {
            'á' | 'à' | 'â' | 'ã' | 'ä' => 'a',
            'é' | 'ê' | 'è' => 'e',
            'í' | 'ì' => 'i',
            'ó' | 'ô' | 'õ' | 'ò' => 'o',
            'ú' | 'ù' | 'ü' => 'u',
            'ç' => 'c',
            c => c,
        })
        .collect()
}

/// Id of the kind whose English name is `lit` (any form in the lexicon).
pub fn kind_by_name(lex: &Lexicon, lit: &str) -> Option<String> {
    match lex.sentences.get(&lit.trim().to_lowercase()) {
        Some(Tok::Noun(id)) => Some(id.clone()),
        _ => None,
    }
}

/// Id of the verb whose English form is `lit` (`runs on`, `contains`).
pub fn verb_by_name(lex: &Lexicon, lit: &str) -> Option<String> {
    match lex.sentences.get(&lit.trim().to_lowercase()) {
        Some(Tok::Verb(id)) => Some(id.clone()),
        _ => None,
    }
}

/// Checks a view sentence at analysis time: no negation, and the verb's literals inside
/// its vocabulary.
pub fn check_sentence(lex: &Lexicon, verb: &str, mode: Option<&str>, literals: &[String]) -> Result<(), String> {
    if mode.is_some() {
        return Err("a view sentence does not accept negation, 'always' or 'exceptionally'".into());
    }
    for l in literals {
        match verb {
            SHOW_ALL if kind_by_name(lex, l).is_none() => {
                return Err(format!("'{l}' is not a lexicon type (use the English name, like 'thread' or 'key-value store')"))
            }
            GROUP if verb_by_name(lex, l).is_none() => {
                return Err(format!("'{l}' is not a lexicon relation (use the English form of the verb, like 'runs on' or 'contains')"))
            }
            FORM if !FORMS.contains(&l.as_str()) => return Err(format!("unknown form '{l}'; forms: {}", FORMS.join(", "))),
            MARK if !MARKS.contains(&l.as_str()) => return Err(format!("unknown mark '{l}'; marks: {}", MARKS.join(", "))),
            _ => {}
        }
    }
    Ok(())
}

/// Parameters of a declared view, read from the IR.
#[derive(Clone, Debug)]
pub struct Decl {
    pub id: String,
    pub name: String,
    /// entities chosen by name
    pub focus: BTreeSet<String>,
    /// types chosen (all entities of the type and of its subtypes)
    pub kinds: BTreeSet<String>,
    pub group_by: Option<String>,
    pub form: Option<String>,
    /// empty: all annotations
    pub marks: BTreeSet<String>,
}

/// The declared views, by name.
pub fn declared(a: &Analysis) -> Vec<Decl> {
    let mut v: Vec<Decl> = a
        .entities
        .values()
        .filter(|e| a.lex.is_a(&e.kind, "e.view"))
        .map(|e| {
            let mut d = Decl {
                id: e.id.clone(),
                name: e.name.clone(),
                focus: BTreeSet::new(),
                kinds: BTreeSet::new(),
                group_by: None,
                form: None,
                marks: BTreeSet::new(),
            };
            for f in a.facts.iter().filter(|f| f.subject() == e.id) {
                match (f.rel(), f.obj(), f.literal()) {
                    (SHOW, Some(o), _) => {
                        d.focus.insert(o.to_string());
                    }
                    (SHOW_ALL, _, Some(l)) => {
                        if let Some(t) = kind_by_name(a.lex, l) {
                            d.kinds.insert(t);
                        }
                    }
                    (GROUP, _, Some(l)) => d.group_by = verb_by_name(a.lex, l),
                    (FORM, _, Some(l)) => d.form = Some(l.to_string()),
                    (MARK, _, Some(l)) => {
                        d.marks.insert(l.to_string());
                    }
                    _ => {}
                }
            }
            d
        })
        .collect();
    v.sort_by(|x, y| x.name.cmp(&y.name).then(x.id.cmp(&y.id)));
    v
}

/// Errors that depend on the set of sentences: a single form and a single grouping per view, and
/// a non-empty selection.
pub fn check(a: &Analysis) -> Vec<String> {
    let mut errors = Vec::new();
    for e in a.entities.values().filter(|e| a.lex.is_a(&e.kind, "e.view")) {
        let of_view = |rel: &str| -> BTreeSet<&str> { a.facts.iter().filter(|f| f.subject() == e.id && f.rel() == rel).filter_map(Fact::literal).collect() };
        for (rel, that) in [(FORM, "one form"), (GROUP, "one grouping")] {
            let v = of_view(rel);
            if v.len() > 1 {
                errors.push(format!("the view '{}' has more than {that}: {}", e.name, v.into_iter().map(|x| format!("\"{x}\"")).collect::<Vec<_>>().join(", ")));
            }
        }
        let selects = a.facts.iter().any(|f| f.subject() == e.id && matches!(f.rel(), SHOW | SHOW_ALL));
        if !selects {
            errors.push(format!("the view '{}' selects nothing (use 'shows' or 'shows every')", e.name));
        }
    }
    errors
}
