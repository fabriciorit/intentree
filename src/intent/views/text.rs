//! Text output of the views: compact, for another LLM to read. One line per thing, sections
//! with a short title, names without quotes (unless they carry a separator), one-character
//! marks with the legend at the top. The same queries as the SVG: what one shows, the other says.

use super::model::{Item, Machine, Model, ViewNote, Role};
use super::panels::texts_of_gaps;
use std::collections::BTreeSet;
use std::fmt::Write;

pub const LEGEND: &str = "marks: ● mutable state (← who changes it) · !n invariant n · ? gap: the description does not say · ∅ declared absence: none, on purpose · ⚠ warning · ⏱ timeout · ⟳ period";

/// Name as it appears in the text: quoted only if it carries one of the output separators.
pub fn nm(n: &str) -> String {
    let sep = [", ", " · ", " → ", " ← ", ": ", "; "];
    if n.is_empty() || n != n.trim() || sep.iter().any(|s| n.contains(s)) {
        format!("\"{n}\"")
    } else {
        n.to_string()
    }
}

/// `name · type`.
pub fn ent(m: &Model, e: &str) -> String {
    format!("{} · {}", nm(m.name(e)), m.label_kind(e))
}

/// Numbers as ranges: 1-3,7,9-10.
pub fn ranges(v: &[usize]) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    while i < v.len() {
        let mut j = i;
        while j + 1 < v.len() && v[j + 1] == v[j] + 1 {
            j += 1;
        }
        out.push(if j > i + 1 { format!("{}-{}", v[i], v[j]) } else if j == i + 1 { format!("{},{}", v[i], v[j]) } else { v[i].to_string() });
        i = j + 1;
    }
    out.join(",")
}

/// Invariant mark: ` !1-3,7` (empty without invariants).
pub fn mark_inv(v: &[usize]) -> String {
    if v.is_empty() {
        String::new()
    } else {
        format!(" !{}", ranges(v))
    }
}

pub fn section(o: &mut String, t: &str) {
    if !o.is_empty() && !o.ends_with("\n\n") {
        o.push('\n');
    }
    let _ = writeln!(o, "{t}");
}

/// State machines: states (the initial one marked) and one line per transition with who triggers it.
pub fn machines(o: &mut String, m: &Model, ms: &[Machine]) {
    for q in ms {
        let owner = m.owner.get(&q.id).filter(|d| **d != q.id).map(|d| format!(" (in {})", nm(m.name(d)))).unwrap_or_default();
        let mut es: Vec<String> = q
            .states
            .iter()
            .map(|e| {
                let mut s = nm(m.name(e));
                if Some(e) == q.initial.as_ref() {
                    s.push_str(" (initial)");
                }
                if m.gaps.contains_key(e) {
                    s.push_str(" ?no exit");
                }
                s
            })
            .collect();
        if es.is_empty() {
            es.push("? none declared".into());
        }
        let _ = writeln!(o, "machine {}{owner}: states {}", nm(m.name(&q.id)), es.join(", "));
        if q.initial.is_none() {
            let abs = m.absences.get(&q.id).is_some_and(|v| v.iter().any(|p| p == "initial state"));
            let _ = writeln!(o, "  {}", if abs { "∅ no initial state, on purpose" } else { "? initial state not declared" });
        }
        for (from, to, who) in &q.transitions {
            let _ = writeln!(o, "  {} → {}: {}", nm(m.name(from)), nm(m.name(to)), nm(m.name(who)));
        }
    }
}

/// Threads: period, rules and what each one touches; then the shared primitives.
pub fn threads(o: &mut String, m: &Model) {
    let ths = m.threads();
    if ths.is_empty() {
        return;
    }
    let touch_map = m.touch_map(&ths);
    for t in &ths {
        let mut hdr = nm(m.name(t));
        if let Some(p) = m.period.get(t) {
            let _ = write!(hdr, " ⟳{p}");
        }
        for r in m.rules_of_thread(t) {
            let _ = write!(hdr, " ({r})");
        }
        // verb -> targets
        let mut by_verb: Vec<(String, Vec<String>)> = Vec::new();
        let mut targets: Vec<&String> = touch_map.iter().filter(|(_, ts)| ts.contains_key(t)).map(|(e, _)| e).collect();
        targets.sort_by(|a, b| m.name(a).cmp(m.name(b)));
        for e in targets {
            for v in &touch_map[e][t] {
                match by_verb.iter_mut().find(|(x, _)| x == v) {
                    Some(x) => x.1.push(nm(m.name(e))),
                    None => by_verb.push((v.clone(), vec![nm(m.name(e))])),
                }
            }
        }
        by_verb.sort();
        let body: Vec<String> = by_verb.iter().map(|(v, es)| format!("{v} {}", es.join(", "))).collect();
        if body.is_empty() {
            let _ = writeln!(o, "{hdr}");
        } else {
            let _ = writeln!(o, "{hdr}: {}", body.join("; "));
        }
    }
    let mut shared: Vec<&String> = touch_map.iter().filter(|(_, ts)| ts.len() > 1).map(|(e, _)| e).collect();
    shared.sort_by(|a, b| touch_map[*b].len().cmp(&touch_map[*a].len()).then(m.name(a).cmp(m.name(b))));
    for e in shared {
        let lock = m.protected(e);
        let lock = if lock.is_empty() { String::new() } else { format!(" (protected by {})", lock.join(", ")) };
        let who: Vec<String> = touch_map[e].iter().map(|(t, vs)| format!("{} {}", nm(m.name(t)), vs.iter().cloned().collect::<Vec<_>>().join("+"))).collect();
        let _ = writeln!(o, "shared {}{lock}: {}", ent(m, e), who.join(", "));
    }
}

/// Mutable state: who writes and who reads each primitive.
pub fn state(o: &mut String, m: &Model) {
    let (writes, reads) = m.spelling_read();
    let mut es: Vec<&String> = writes.keys().collect();
    es.sort_by(|a, b| {
        let na = writes[*a].len() + reads.get(*a).map_or(0, |s| s.len());
        let nb = writes[*b].len() + reads.get(*b).map_or(0, |s| s.len());
        nb.cmp(&na).then(m.name(a).cmp(m.name(b)))
    });
    for e in es {
        let list = |s: &BTreeSet<String>| s.iter().map(|x| nm(x)).collect::<Vec<_>>().join(", ");
        let mut l = format!("{}: writes {}", ent(m, e), list(&writes[e]));
        if let Some(r) = reads.get(e) {
            let _ = write!(l, "; reads {}", list(r));
        }
        let _ = writeln!(o, "{l}");
    }
}

/// Roles: `name · type: role=value; role=?; role=∅`. Consecutive entries of the same type and
/// with the same roles become a single line, with the names together.
pub fn cards(o: &mut String, m: &Model, filter: &dyn Fn(&str) -> bool) {
    // (type, body, names)
    let mut groups: Vec<(String, String, Vec<String>)> = Vec::new();
    for (e, ps) in m.cards() {
        if !filter(&e) {
            continue;
        }
        let body: Vec<String> = ps
            .iter()
            .map(|(p, x)| match x {
                Role::Value(v) => format!("{p}={v}"),
                Role::Absent => format!("{p}=∅"),
                Role::Gap => format!("{p}=?"),
            })
            .collect();
        let (kind, body) = (m.label_kind(&e).to_string(), body.join("; "));
        match groups.last_mut() {
            Some(g) if g.0 == kind && g.1 == body => g.2.push(nm(m.name(&e))),
            _ => groups.push((kind, body, vec![nm(m.name(&e))])),
        }
    }
    for (kind, body, names) in groups {
        let _ = writeln!(o, "{} · {kind}: {body}", names.join(", "));
    }
}

pub fn invariants(o: &mut String, m: &Model, filter: &dyn Fn(usize) -> bool) {
    for (i, x) in m.invs.iter().enumerate() {
        if filter(i + 1) {
            let _ = writeln!(o, "!{} {}", i + 1, x.text);
        }
    }
}

pub fn rules(o: &mut String, m: &Model) {
    for r in &m.rules {
        let _ = writeln!(o, "∀ {}", r.text);
    }
}

/// Warnings, gaps and absences (the same lines as the SVG panel).
pub fn gaps(o: &mut String, m: &Model, filter: &dyn Fn(&str) -> bool) -> bool {
    let v = texts_of_gaps(m, filter);
    for (t, _) in &v {
        let _ = writeln!(o, "{t}");
    }
    !v.is_empty()
}

pub fn notes(o: &mut String, ns: &[&ViewNote]) {
    for n in ns {
        match &n.sentence {
            Some(f) => {
                let _ = writeln!(o, "- {} (about: {f})", n.text);
            }
            None => {
                let _ = writeln!(o, "- {}", n.text);
            }
        }
    }
}

/// One line per item of the component; the continuations (← changed by, marked fields) stay on
/// the line above.
pub fn items_of_module(items: &[Item]) -> Vec<String> {
    let mut v: Vec<String> = Vec::new();
    for it in items {
        let t = it.text.trim();
        match (it.sub, v.last_mut()) {
            (true, Some(u)) if t.starts_with('←') => {
                u.push(' ');
                u.push_str(t);
            }
            (true, Some(u)) => {
                let _ = write!(u, " [{t}]");
            }
            _ => v.push(t.to_string()),
        }
    }
    v
}

/// Connections of a component grouped by verb: `listens on a, b`.
fn outputs_by_verb(m: &Model, rels: &[(String, String, String)], n: &str) -> Vec<String> {
    let mut grouped: Vec<(String, Vec<String>)> = Vec::new();
    for (_, d, vs) in rels.iter().filter(|(a, _, _)| a == n) {
        for v in vs.split(", ") {
            match grouped.iter_mut().find(|(x, _)| x == v) {
                Some(x) => x.1.push(nm(m.name(d))),
                None => grouped.push((v.to_string(), vec![nm(m.name(d))])),
            }
        }
    }
    grouped.sort();
    grouped.into_iter().map(|(v, ds)| format!("{v} {}", ds.join(", "))).collect()
}

/// Structure: components by layer (declared, or by dependency order), with the primitives and
/// the marks, and the connections of each one; then what is external.
pub fn structure(o: &mut String, m: &Model) {
    let rels = m.relations();
    let l = m.lines(&rels);
    section(o, if l.declared { "structure (by declared layer)" } else { "structure (level 0 uses the ones below)" });
    if m.modules.is_empty() {
        o.push_str("no component declared\n");
    }
    for (k, (label, line)) in l.lines.iter().enumerate() {
        match label {
            Some(r) => {
                let _ = writeln!(o, "{r}:");
            }
            None if l.lines.len() > 1 => {
                let _ = writeln!(o, "level {k}:");
            }
            None => {}
        }
        for n in line {
            let mark = mark_inv(&m.invs_of(|t| m.owner.get(t).map(String::as_str) == Some(n.as_str())));
            let _ = writeln!(o, "{} · {}{mark}", nm(m.name(n)), m.label_kind(n));
            for t in items_of_module(&m.lines_of_module(n)) {
                let _ = writeln!(o, "  {t}");
            }
            let lim = m.limits_of_fields(n);
            if !lim.is_empty() {
                let _ = writeln!(o, "  limits: {}", lim.join("; "));
            }
            let outputs = outputs_by_verb(m, &rels, n);
            if !outputs.is_empty() {
                let _ = writeln!(o, "  → {}", outputs.join("; "));
            }
        }
    }
    let external = m.external(&rels);
    if !external.is_empty() {
        section(o, "externals");
        for e in &external {
            let mut l = ent(m, e);
            if let Some(p) = m.deadline.get(e) {
                let _ = write!(l, " ⏱{p}");
            }
            if m.gaps.contains_key(e) {
                l.push_str(" ?");
            }
            if m.absences.contains_key(e) {
                l.push_str(" ∅");
            }
            let to_external = outputs_by_verb(m, &rels, e);
            if !to_external.is_empty() {
                let _ = write!(l, " → {}", to_external.join("; "));
            }
            let _ = writeln!(o, "{l}");
        }
    }
}

/// Title of the notes block: they are informal.
pub fn notes_with_title(o: &mut String, ns: &[&ViewNote]) {
    if ns.is_empty() {
        return;
    }
    section(o, "notes (free text, outside the IR)");
    notes(o, ns);
}

/// Section of gaps, warnings and absences; says so when there are none.
pub fn gaps_with_title(o: &mut String, m: &Model, filter: &dyn Fn(&str) -> bool) {
    section(o, "gaps (the description does not say), warnings and absences (none, on purpose)");
    if !gaps(o, m, filter) {
        o.push_str("none\n");
    }
}
