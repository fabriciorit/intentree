//! JSON output of the views: the same queries as the SVG and the text, in objects with fixed
//! keys. Names of things (not ids) as values: the reader is another tool or an LLM.

use super::model::{Machine, Model, ViewNote, Role};
use super::panels::texts_of_gaps;
use super::text::items_of_module;
use super::super::json::J;
use std::collections::BTreeSet;

fn s(x: &str) -> J {
    J::s(x)
}

fn list(v: impl IntoIterator<Item = String>) -> J {
    J::Arr(v.into_iter().map(|x| J::Str(x)).collect())
}

fn nums(v: &[usize]) -> J {
    J::Arr(v.iter().map(|&n| J::Num(n as f64)).collect())
}

pub fn structure(m: &Model) -> J {
    let rels = m.relations();
    let l = m.lines(&rels);
    let lines = l
        .lines
        .iter()
        .enumerate()
        .map(|(k, (label, comps))| {
            let comps = comps
                .iter()
                .map(|n| {
                    let outputs = rels
                        .iter()
                        .filter(|(a, _, _)| a == n)
                        .map(|(_, d, v)| J::obj(vec![("to", s(m.name(d))), ("verbs", s(v))]))
                        .collect();
                    J::obj(vec![
                        ("invariants", nums(&m.invs_of(|t| m.owner.get(t).map(String::as_str) == Some(n.as_str())))),
                        ("items", list(items_of_module(&m.lines_of_module(n)))),
                        ("links", J::Arr(outputs)),
                        ("name", s(m.name(n))),
                        ("type", s(m.label_kind(n))),
                    ])
                })
                .collect();
            J::obj(vec![("components", J::Arr(comps)), ("layer", label.as_deref().map(s).unwrap_or(J::Null)), ("level", J::Num(k as f64))])
        })
        .collect();
    let external = m
        .external(&rels)
        .iter()
        .map(|e| {
            J::obj(vec![
                ("absences", list(m.absences.get(e).cloned().unwrap_or_default())),
                ("gaps", list(m.gaps.get(e).cloned().unwrap_or_default())),
                ("name", s(m.name(e))),
                ("timeout", m.deadline.get(e).map(|p| s(p)).unwrap_or(J::Null)),
                ("type", s(m.label_kind(e))),
            ])
        })
        .collect();
    J::obj(vec![("declared_layers", J::Bool(l.declared)), ("externals", J::Arr(external)), ("rows", J::Arr(lines))])
}

pub fn machines(m: &Model, ms: &[Machine]) -> J {
    J::Arr(
        ms.iter()
            .map(|q| {
                let abs = m.absences.get(&q.id).is_some_and(|v| v.iter().any(|p| p == "initial state"));
                J::obj(vec![
                    ("initial", q.initial.as_deref().map(|e| s(m.name(e))).unwrap_or(J::Null)),
                    ("initial_absent_on_purpose", J::Bool(abs)),
                    ("name", s(m.name(&q.id))),
                    ("no_exit", list(q.states.iter().filter(|e| m.gaps.contains_key(*e)).map(|e| m.name(e).to_string()))),
                    ("states", list(q.states.iter().map(|e| m.name(e).to_string()))),
                    (
                        "transitions",
                        J::Arr(q.transitions.iter().map(|(from, to, who)| J::obj(vec![("from", s(m.name(from))), ("to", s(m.name(to))), ("triggered_by", s(m.name(who)))])).collect()),
                    ),
                ])
            })
            .collect(),
    )
}

pub fn threads(m: &Model) -> J {
    let ths = m.threads();
    let touch_map = m.touch_map(&ths);
    let tj = ths
        .iter()
        .map(|t| {
            let touches = touch_map
                .iter()
                .filter(|(_, x)| x.contains_key(t))
                .map(|(e, x)| J::obj(vec![("target", s(m.name(e))), ("verbs", list(x[t].iter().cloned()))]))
                .collect();
            J::obj(vec![
                ("name", s(m.name(t))),
                ("period", m.period.get(t).map(|p| s(p)).unwrap_or(J::Null)),
                ("rules", list(m.rules_of_thread(t))),
                ("touches", J::Arr(touches)),
            ])
        })
        .collect();
    let shared = touch_map
        .iter()
        .filter(|(_, x)| x.len() > 1)
        .map(|(e, x)| {
            J::obj(vec![
                ("protected_by", list(m.protected(e))),
                ("target", s(m.name(e))),
                ("threads", list(x.keys().map(|t| m.name(t).to_string()))),
            ])
        })
        .collect();
    J::obj(vec![("shared", J::Arr(shared)), ("threads", J::Arr(tj))])
}

pub fn state(m: &Model) -> J {
    let (writes, reads) = m.spelling_read();
    J::Arr(
        writes
            .iter()
            .map(|(e, w)| {
                J::obj(vec![
                    ("name", s(m.name(e))),
                    ("reads", list(reads.get(e).into_iter().flatten().cloned())),
                    ("type", s(m.label_kind(e))),
                    ("writes", list(w.iter().cloned())),
                ])
            })
            .collect(),
    )
}

pub fn cards(m: &Model, filter: &dyn Fn(&str) -> bool) -> J {
    J::Arr(
        m.cards()
            .iter()
            .filter(|(e, _)| filter(e))
            .map(|(e, ps)| {
                let roles = ps
                    .iter()
                    .map(|(p, v)| {
                        let (style, val) = match v {
                            Role::Value(x) => ("value", s(x)),
                            Role::Absent => ("absent", J::Null),
                            Role::Gap => ("gap", J::Null),
                        };
                        J::obj(vec![("role", s(p)), ("status", s(style)), ("value", val)])
                    })
                    .collect();
                J::obj(vec![("name", s(m.name(e))), ("roles", J::Arr(roles)), ("type", s(m.label_kind(e)))])
            })
            .collect(),
    )
}

pub fn invariants(m: &Model) -> J {
    J::Arr(m.invs.iter().enumerate().map(|(i, x)| J::obj(vec![("n", J::Num((i + 1) as f64)), ("text", s(&x.text))])).collect())
}

pub fn rules(m: &Model) -> J {
    J::Arr(m.rules.iter().map(|r| s(&r.text)).collect())
}

/// Gaps, warnings and absences: each with the ready text (the same as the panel).
pub fn gaps(m: &Model, filter: &dyn Fn(&str) -> bool) -> J {
    let mut gap = Vec::new();
    let mut warns = Vec::new();
    let mut abs = Vec::new();
    let ents: BTreeSet<&String> = m.gaps.keys().chain(m.warnings.keys()).chain(m.absences.keys()).collect();
    for e in ents.into_iter().filter(|e| filter(e)) {
        for p in m.gaps.get(e).into_iter().flatten() {
            gap.push(J::obj(vec![("entity", s(m.name(e))), ("role", s(p)), ("type", s(m.label_kind(e)))]));
        }
        for t in m.warnings.get(e).into_iter().flatten() {
            warns.push(J::obj(vec![("entity", s(m.name(e))), ("text", s(t)), ("type", s(m.label_kind(e)))]));
        }
        for p in m.absences.get(e).into_iter().flatten() {
            abs.push(J::obj(vec![("entity", s(m.name(e))), ("role", s(p)), ("type", s(m.label_kind(e)))]));
        }
    }
    let _ = texts_of_gaps;
    J::obj(vec![("declared_absences", J::Arr(abs)), ("gaps", J::Arr(gap)), ("warnings", J::Arr(warns))])
}

pub fn notes(ns: &[&ViewNote]) -> J {
    J::Arr(
        ns.iter()
            .map(|n| {
                let mut v = Vec::new();
                if let Some(f) = &n.sentence {
                    v.push(("about", s(f)));
                }
                v.push(("formal", J::Bool(false)));
                v.push(("text", s(&n.text)));
                J::obj(v)
            })
            .collect(),
    )
}
