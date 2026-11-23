//! Views declared in the language, in the three outputs. The selection (entities and types)
//! defines the induced subgraph: only relations between two selected things enter. Grouping puts
//! each thing inside the target of a relation (`runs on`; with `contains`, inside whatever
//! contains it), the form chooses the drawing (`boxes`, `matrix`, `list`) and the marks choose
//! the annotations (`gaps`, `invariants`, `states`, `notes`; without the sentence, all of them).

use super::declared::Decl;
use super::model::{Model, ViewNote};
use super::panels::{find_label, header, geometry, notes as panel_notes, Box};
use super::constraints::lines_of;
use super::svg::{self, width, width_b, wrap, Style, Svg};
use super::text::{self, ent, ranges, nm, section};
use super::super::analysis::Fact;
use super::super::json::J;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;

pub struct Freeform<'a> {
    pub d: &'a Decl,
    pub form: String,
    pub sel: BTreeSet<String>,
    /// (group, members): the group is `None` for whatever stayed outside all of them
    pub groups: Vec<(Option<String>, Vec<String>)>,
    /// (from, to, verbs), only between selected things
    pub rels: Vec<(String, String, String)>,
}

impl<'a> Freeform<'a> {
    pub fn new(m: &Model, d: &'a Decl) -> Freeform<'a> {
        let a = m.a;
        let mut sel: BTreeSet<String> = d.focus.clone();
        for e in a.entities.values() {
            if d.kinds.iter().any(|t| a.lex.is_a(&e.kind, t)) && !m.views.contains(&e.id) {
                sel.insert(e.id.clone());
            }
        }
        // relations between the selected ones, except the grouping one
        let mut edges: BTreeMap<(String, String), BTreeSet<String>> = BTreeMap::new();
        for f in &m.facts {
            let Fact::Rel { rel, .. } = f else { continue };
            if f.mode().is_some() || d.group_by.as_deref() == Some(rel.as_str()) || !sel.contains(f.subject()) {
                continue;
            }
            let mut targets: Vec<&str> = f.obj().into_iter().collect();
            if let Fact::Rel { roles, .. } = f {
                targets.extend(roles.values().map(String::as_str));
            }
            for t in targets {
                if t != f.subject() && sel.contains(t) {
                    edges.entry((f.subject().to_string(), t.to_string())).or_default().insert(m.verb_pt(rel).to_string());
                }
            }
        }
        let mut rels: Vec<(String, String, String)> = edges.into_iter().map(|((o, t), v)| (o, t, v.into_iter().collect::<Vec<_>>().join(", "))).collect();
        rels.sort_by(|x, y| (m.name(&x.0), m.name(&x.1), &x.0, &x.1).cmp(&(m.name(&y.0), m.name(&y.1), &y.0, &y.1)));
        // group of each selected one: the first (by name) that the relation indicates
        let mut group_of: BTreeMap<String, String> = BTreeMap::new();
        if let Some(g) = &d.group_by {
            let mut pairs: Vec<(String, String)> = Vec::new();
            for f in &m.facts {
                if f.rel() != g || f.mode().is_some() {
                    continue;
                }
                let Some(o) = f.obj() else { continue };
                let (member, group) = if g == "a.contains" { (o, f.subject()) } else { (f.subject(), o) };
                if sel.contains(member) && member != group {
                    pairs.push((member.to_string(), group.to_string()));
                }
            }
            pairs.sort_by(|x, y| m.name(&x.1).cmp(m.name(&y.1)).then(x.1.cmp(&y.1)));
            for (member, group) in pairs {
                group_of.entry(member).or_insert(group);
            }
        }
        // whoever serves as a group is its own box: not repeated among the ungrouped
        let owners: BTreeSet<&String> = group_of.values().collect();
        let mut grouped: BTreeMap<Option<String>, Vec<String>> = BTreeMap::new();
        for e in &sel {
            let g = group_of.get(e).cloned();
            if g.is_none() && owners.contains(e) {
                continue;
            }
            grouped.entry(g).or_default().push(e.clone());
        }
        let mut keys: Vec<Option<String>> = grouped.keys().cloned().collect();
        // groups: layers in declared order (`is above`); the others by size (largest
        // first) and by name; the ungrouped last
        let prof = m.prof_layers();
        keys.sort_by(|x, y| match (x, y) {
            (Some(a), Some(b)) if prof.contains_key(a) && prof.contains_key(b) => prof[a].cmp(&prof[b]).then(m.name(a).cmp(m.name(b))).then(a.cmp(b)),
            (Some(a), Some(b)) => grouped[y].len().cmp(&grouped[x].len()).then(m.name(a).cmp(m.name(b))).then(a.cmp(b)),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        });
        let groups = keys
            .into_iter()
            .map(|k| {
                let mut v = grouped.remove(&k).unwrap_or_default();
                v.sort_by(|a, b| (m.kind(a), m.name(a), a).cmp(&(m.kind(b), m.name(b), b)));
                (k, v)
            })
            .collect();
        let form = d.form.clone().unwrap_or_else(|| "boxes".to_string());
        Freeform { d, form, sel, groups, rels }
    }

    fn mark(&self, k: &str) -> bool {
        self.d.marks.is_empty() || self.d.marks.contains(k)
    }

    /// Entities that enter the gap and note filters: the selection and the groups.
    pub fn scope(&self) -> BTreeSet<String> {
        let mut s = self.sel.clone();
        s.extend(self.groups.iter().filter_map(|(g, _)| g.clone()));
        s
    }

    /// Invariants (numbers) that touch the entity.
    fn invs(&self, m: &Model, e: &str) -> Vec<usize> {
        if self.mark("invariants") {
            m.invs_of(|t| t == e)
        } else {
            Vec::new()
        }
    }

    /// Annotations of the entity: lines of text with the color.
    pub fn annotations(&self, m: &Model, e: &str) -> Vec<(String, &'static str)> {
        let mut v = Vec::new();
        if self.mark("states") && m.with_state.contains(e) {
            let mut grouped: Vec<&str> = m.changed_by.get(e).into_iter().flatten().filter(|x| *x != e).map(|x| m.name(x)).collect();
            grouped.sort();
            v.push((if grouped.is_empty() { "●".to_string() } else { format!("● ← changed by {}", grouped.join(", ")) }, svg::STATE));
        }
        if self.mark("gaps") {
            v.extend(lines_of(m, e));
        }
        v
    }

    fn outputs(&self, m: &Model, e: &str) -> Vec<String> {
        // verb -> targets
        let mut grouped: BTreeMap<&str, Vec<String>> = BTreeMap::new();
        for (o, t, v) in &self.rels {
            if o == e {
                grouped.entry(v).or_default().push(nm(m.name(t)));
            }
        }
        grouped.into_iter().map(|(v, ts)| format!("{v} {}", ts.join(", "))).collect()
    }

    pub fn description(&self, m: &Model) -> String {
        let mut p = Vec::new();
        if !self.d.focus.is_empty() {
            let mut ns: Vec<String> = self.d.focus.iter().map(|e| nm(m.name(e))).collect();
            ns.sort();
            p.push(format!("shows {}", ns.join(", ")));
        }
        if !self.d.kinds.is_empty() {
            let mut ts: Vec<&str> = self.d.kinds.iter().filter_map(|t| m.a.lex.kind(t)).filter_map(|t| t.en.first().map(String::as_str)).collect();
            ts.sort();
            p.push(format!("every {}", ts.join(", ")));
        }
        if let Some(g) = &self.d.group_by {
            p.push(format!("groups by “{}”", m.verb_pt(g)));
        }
        p.push(format!("form {}", self.form));
        if !self.d.marks.is_empty() {
            p.push(format!("marks {}", self.d.marks.iter().cloned().collect::<Vec<_>>().join(", ")));
        }
        p.join("; ")
    }

    fn notes<'b>(&self, m: &'b Model) -> Vec<&'b ViewNote> {
        if !self.mark("notes") {
            return Vec::new();
        }
        let esc = self.scope();
        m.notes_of(true, |e| esc.contains(e))
    }

    fn invariants_touched(&self, m: &Model) -> Vec<usize> {
        if !self.mark("invariants") {
            return Vec::new();
        }
        m.invs_of(|t| self.sel.contains(t))
    }

    /// (target, [(subject, verbs)]) for the matrix: who touches each thing.
    fn matrix(&self) -> BTreeMap<String, BTreeMap<String, String>> {
        let mut t: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
        for (o, d, v) in &self.rels {
            t.entry(d.clone()).or_default().insert(o.clone(), v.clone());
        }
        t
    }
}

// ------------------------------------------------------------------ text

pub fn text(m: &Model, l: &Freeform) -> String {
    let mut o = String::new();
    let _ = writeln!(o, "view {} · declared: {}", nm(&l.d.name), l.description(m));
    if l.sel.is_empty() {
        o.push_str("empty selection: no entity matches\n");
    }
    if l.form == "matrix" {
        let mat = l.matrix();
        let mut targets: Vec<&String> = mat.keys().collect();
        targets.sort_by(|a, b| mat[*b].len().cmp(&mat[*a].len()).then(m.name(a).cmp(m.name(b))));
        section(&mut o, "matrix (row: target; cells: who does what)");
        for e in targets {
            let mut who: Vec<(&str, &String)> = mat[e].iter().map(|(a, v)| (m.name(a), v)).collect();
            who.sort();
            let who: Vec<String> = who.into_iter().map(|(a, v)| format!("{} {v}", nm(a))).collect();
            let _ = writeln!(o, "{}: {}", ent(m, e), who.join("; "));
        }
        let loose: Vec<String> = l.sel.iter().filter(|e| !mat.contains_key(*e) && !l.rels.iter().any(|(a, _, _)| a == *e)).map(|e| nm(m.name(e))).collect();
        if !loose.is_empty() {
            let _ = writeln!(o, "no relation in the selection: {}", loose.join(", "));
        }
        for e in &l.sel {
            for (t, _) in l.annotations(m, e) {
                let _ = writeln!(o, "{} {t}", nm(m.name(e)));
            }
        }
    } else {
        for (g, members) in &l.groups {
            match g {
                Some(g) => {
                    let _ = writeln!(o, "{}:", ent(m, g));
                }
                None if l.d.group_by.is_some() => o.push_str("no group:\n"),
                None => {}
            }
            for e in members {
                let mut hdr = ent(m, e);
                let inv = l.invs(m, e);
                if !inv.is_empty() {
                    let _ = write!(hdr, " !{}", ranges(&inv));
                }
                let mut parts = l.outputs(m, e);
                parts.extend(l.annotations(m, e).into_iter().map(|(t, _)| t));
                let ind = if l.d.group_by.is_some() { "  " } else { "" };
                let _ = writeln!(o, "{ind}{hdr}{}{}", if parts.is_empty() { "" } else { ": " }, parts.join("; "));
            }
        }
    }
    let inv = l.invariants_touched(m);
    if !inv.is_empty() {
        section(&mut o, "invariants touching the selection");
        text::invariants(&mut o, m, &|n| inv.contains(&n));
    }
    if l.mark("gaps") {
        let esc = l.scope();
        text::gaps_with_title(&mut o, m, &|e| esc.contains(e));
    }
    text::notes_with_title(&mut o, &l.notes(m));
    o
}

// ------------------------------------------------------------------ json

pub fn json(m: &Model, l: &Freeform) -> J {
    let s = J::s;
    let esc = l.scope();
    let groups = l
        .groups
        .iter()
        .map(|(g, members)| {
            let ents = members
                .iter()
                .map(|e| {
                    J::obj(vec![
                        ("annotations", J::Arr(l.annotations(m, e).into_iter().map(|(t, _)| J::Str(t)).collect())),
                        ("invariants", J::Arr(l.invs(m, e).iter().map(|&n| J::Num(n as f64)).collect())),
                        ("links", J::Arr(l.outputs(m, e).iter().map(|x| s(x)).collect())),
                        ("name", s(m.name(e))),
                        ("type", s(m.label_kind(e))),
                    ])
                })
                .collect();
            J::obj(vec![("entities", J::Arr(ents)), ("group", g.as_deref().map(|x| s(m.name(x))).unwrap_or(J::Null))])
        })
        .collect();
    let rels = l.rels.iter().map(|(o, t, v)| J::obj(vec![("from", s(m.name(o))), ("to", s(m.name(t))), ("verbs", s(v))])).collect();
    let mut v = vec![
        ("declared", J::Bool(true)),
        ("description", s(&l.description(m))),
        ("form", s(&l.form)),
        ("groups", J::Arr(groups)),
        ("relations", J::Arr(rels)),
        ("view", s(&l.d.name)),
    ];
    if l.mark("invariants") {
        let inv = l.invariants_touched(m);
        v.push(("invariants", J::Arr(inv.iter().map(|&n| J::obj(vec![("n", J::Num(n as f64)), ("text", s(&m.invs[n - 1].text))])).collect())));
    }
    if l.mark("gaps") {
        v.push(("gaps", super::data::gaps(m, &|e| esc.contains(e))));
    }
    v.push(("notes", super::data::notes(&l.notes(m))));
    J::obj(v)
}

// ------------------------------------------------------------------ svg

pub fn svg(m: &Model, l: &Freeform) -> String {
    let mut s = Svg::default();
    header(&mut s, &format!("View “{}”", l.d.name), &l.description(m));
    let mut y = 80.0;
    if l.sel.is_empty() {
        s.t(24.0, y, "empty selection: no entity matches", Style::new(11.0, svg::GAP));
        y += 20.0;
    } else {
        y = match l.form.as_str() {
            "matrix" => matrix(&mut s, m, l, 24.0, y),
            "list" => list(&mut s, m, l, 24.0, y, 1000.0),
            _ => boxes(&mut s, m, l, 24.0, y, 1500.0),
        };
    }
    y += 20.0;
    let inv = l.invariants_touched(m);
    if !inv.is_empty() {
        s.t(24.0, y, "Invariants touching the selection", Style::new(15.0, svg::TEXT).bold());
        y += 20.0;
        for n in inv {
            s.t(24.0, y, &format!("!{n}"), Style::new(11.0, svg::INV).bold());
            for (k, ln) in wrap(&m.invs[n - 1].text, 1000.0, 11.0).iter().enumerate() {
                s.t(50.0, y + 14.0 * k as f64, ln, Style::new(11.0, svg::TEXT));
                y += if k > 0 { 14.0 } else { 0.0 };
            }
            y += 20.0;
        }
        y += 14.0;
    }
    if l.mark("gaps") {
        let esc = l.scope();
        y = super::panels::gaps(&mut s, m, 24.0, y, 1400.0, &|e| esc.contains(e)) + 10.0;
    }
    let end = panel_notes(&mut s, m, 24.0, y, 1400.0, &l.notes(m)) + 10.0;
    s.svg(1560.0, end)
}

fn alt_box(l: &Freeform, m: &Model, e: &str) -> f64 {
    let extra = l.annotations(m, e).len() + usize::from(!l.invs(m, e).is_empty());
    30.0 + 15.0 * extra as f64
}

/// Groups side by side (in rows, if they do not fit), members in columns inside the group.
fn boxes(s: &mut Svg, m: &Model, l: &Freeform, x0: f64, y0: f64, w_total: f64) -> f64 {
    let (wm, gap) = (230.0, 14.0);
    let mut cx = x0;
    let mut cy = y0;
    let mut alt_line = 0.0f64;
    let mut pos: BTreeMap<String, Box> = BTreeMap::new();
    let mut leaves: Vec<Box> = Vec::new();
    let mut rects: Vec<(Option<String>, Box)> = Vec::new();
    for (g, members) in &l.groups {
        let cols = if members.len() <= 4 { 1 } else if members.len() <= 12 { 2 } else { 3 };
        let wg = cols as f64 * (wm + gap) + gap;
        if cx > x0 && cx + wg > x0 + w_total {
            cx = x0;
            cy += alt_line + 40.0;
            alt_line = 0.0;
        }
        let top = if g.is_some() || l.d.group_by.is_some() { 36.0 } else { 0.0 };
        let mut yy = vec![cy + top + 8.0; cols];
        for e in members {
            let mut c = 0;
            for j in 1..cols {
                if yy[j] < yy[c] {
                    c = j;
                }
            }
            let h = alt_box(l, m, e);
            let b = (cx + gap + c as f64 * (wm + gap), yy[c], wm, h);
            pos.insert(e.clone(), b);
            leaves.push(b);
            yy[c] += h + 14.0;
        }
        let hg = yy.iter().cloned().fold(f64::MIN, f64::max) - cy;
        rects.push((g.clone(), (cx, cy, wg, hg)));
        alt_line = alt_line.max(hg);
        cx += wg + 100.0;
    }
    for (g, (x, y, w, h)) in &rects {
        if g.is_none() && l.d.group_by.is_none() {
            continue;
        }
        let tagged = s.mark();
        s.r(*x, *y, *w, *h, "#fbfaf6", svg::BORDER, 14.0, g.is_none());
        let t = match g {
            Some(g) => format!("{} “{}”", m.label_kind(g), m.name(g)),
            None => "no group".to_string(),
        };
        s.t(x + 14.0, y + 24.0, &t, Style::new(14.0, svg::TEXT).bold());
        if let Some(g) = g {
            s.tag(tagged, m.name(g), None);
        }
    }
    let mut labels = Vec::new();
    let mut busy = Vec::new();
    for (o, d, v) in &l.rels {
        let (Some(&(xo, yo, wo, ho)), Some(&(xd, yd, wd, hd))) = (pos.get(o), pos.get(d)) else { continue };
        let back = l.rels.iter().any(|(a, b, _)| a == d && b == o);
        let dy = if back && o > d {
            8.0
        } else if back {
            -8.0
        } else {
            0.0
        };
        let (p0, p2, curve) = geometry((xo, yo, wo, ho), (xd, yd, wd, hd), dy, &leaves);
        s.arrow(p0.0, p0.1, p2.0, p2.1, svg::A1, curve, false);
        // "uses" is the unlabeled relation (as in the structure): only the arrow
        if v == "uses" {
            continue;
        }
        let p = find_label(p0, p2, curve, v, &leaves, &mut busy).unwrap_or(((p0.0 + p2.0) / 2.0 + curve / 2.0, (p0.1 + p2.1) / 2.0 - 6.0));
        labels.push((p, v.clone()));
    }
    for (e, &(x, y, w, h)) in &pos {
        let tagged = s.mark();
        s.r(x, y, w, h, svg::BOX, svg::BORDER, 6.0, false);
        s.t(x + 8.0, y + 18.0, m.name(e), Style::new(13.0, svg::TEXT).bold());
        let label = m.label_kind(e);
        if width_b(m.name(e), 13.0) + width(label, 10.0) + 20.0 < w {
            s.t(x + 12.0 + width_b(m.name(e), 13.0), y + 18.0, label, Style::new(10.0, svg::MUTED));
        }
        let inv = l.invs(m, e);
        let mut k = 0;
        if !inv.is_empty() {
            s.t(x + 8.0, y + 35.0, &format!("!{}", ranges(&inv)), Style::new(11.0, svg::INV).bold());
            k = 1;
        }
        for (t, color) in l.annotations(m, e) {
            s.t(x + 8.0, y + 35.0 + 15.0 * k as f64, &t, Style::new(11.0, color));
            k += 1;
        }
        s.tag(tagged, m.name(e), None);
    }
    for ((mx, my), v) in &labels {
        let w = width(v, 10.0) + 8.0;
        s.r(mx - w / 2.0, my - 10.0, w, 14.0, "white", svg::BORDER, 3.0, false);
        s.t(*mx, *my, v, Style::new(10.0, svg::A1).middle());
    }
    cy + alt_line + 14.0
}

/// Matrix: columns are the actors, rows are the target; a row touched by more than one is highlighted.
fn matrix(s: &mut Svg, m: &Model, l: &Freeform, x0: f64, y0: f64) -> f64 {
    let mat = l.matrix();
    if mat.is_empty() {
        s.t(x0, y0, "no relation between the selected things", Style::new(11.0, svg::GAP));
        return y0 + 20.0;
    }
    let mut groups: Vec<String> = mat.values().flat_map(|c| c.keys().cloned()).collect::<BTreeSet<_>>().into_iter().collect();
    let group_of = |e: &str| -> String {
        l.groups.iter().find(|(_, v)| v.iter().any(|x| x == e)).and_then(|(g, _)| g.as_ref()).map(|g| m.name(g).to_string()).unwrap_or_default()
    };
    groups.sort_by(|a, b| (group_of(a), m.name(a), a).cmp(&(group_of(b), m.name(b), b)));
    let mut targets: Vec<&String> = mat.keys().collect();
    targets.sort_by(|a, b| mat[*b].len().cmp(&mat[*a].len()).then(m.name(a).cmp(m.name(b))));
    let label = |e: &str| format!("{}{}", m.name(e), if m.with_state.contains(e) && l.mark("states") { " ●" } else { "" });
    let col0 = targets.iter().map(|e| width(&label(e), 10.0) + 12.0).fold(128.0, f64::max);
    let sub = |e: &str| -> Option<String> {
        let g = group_of(e);
        let p = m.period.get(e).map(|p| format!("⟳ {p}"));
        match (g.is_empty(), p) {
            (true, None) => None,
            (true, Some(p)) => Some(p),
            (false, None) => Some(g),
            (false, Some(p)) => Some(format!("{g} {p}")),
        }
    };
    let mut xs = Vec::new();
    let mut x = x0 + col0;
    for a in &groups {
        xs.push(x);
        let mut cw = width(m.name(a), 11.0).max(sub(a).map(|t| width(&t, 10.0)).unwrap_or(0.0));
        for e in &targets {
            if let Some(c) = mat[*e].get(a) {
                cw = cw.max(width(c, 10.0));
            }
        }
        x += cw + 16.0;
    }
    let wt = x - x0;
    let mut y = y0 + 12.0;
    for (j, a) in groups.iter().enumerate() {
        s.t(xs[j] + 4.0, y, m.name(a), Style::new(11.0, svg::TEXT).bold());
        if let Some(t) = sub(a) {
            s.t(xs[j] + 4.0, y + 13.0, &t, Style::new(10.0, svg::MUTED));
        }
    }
    y += 30.0;
    for e in targets {
        let several = mat[e].len() > 1;
        s.r(x0, y - 11.0, wt, 16.0, if several { svg::BOX } else { "white" }, svg::BORDER, 2.0, false);
        s.t(x0 + 4.0, y + 1.0, &label(e), if several { Style::new(10.0, svg::STATE).bold() } else { Style::new(10.0, svg::TEXT) });
        for (j, a) in groups.iter().enumerate() {
            if let Some(c) = mat[e].get(a) {
                s.t(xs[j] + 4.0, y + 1.0, c, Style::new(10.0, svg::TEXT));
            }
        }
        y += 18.0;
    }
    s.t(x0, y + 4.0, "highlighted row: touched by more than one thing", Style::new(10.0, svg::MUTED));
    y += 20.0;
    // annotations per entity (the matrix has nowhere to put them)
    for e in &l.sel {
        for (t, color) in l.annotations(m, e) {
            s.t(x0, y + 8.0, &format!("{}: {t}", m.name(e)), Style::new(10.0, color));
            y += 14.0;
        }
    }
    y
}

/// List: a bold line per thing, and below it what it does with the other selected ones.
fn list(s: &mut Svg, m: &Model, l: &Freeform, x0: f64, y0: f64, w: f64) -> f64 {
    let mut y = y0;
    for (g, members) in &l.groups {
        match g {
            Some(g) => {
                s.t(x0, y, &format!("{} “{}”", m.label_kind(g), m.name(g)), Style::new(14.0, svg::TEXT).bold());
                y += 20.0;
            }
            None if l.d.group_by.is_some() => {
                s.t(x0, y, "no group", Style::new(14.0, svg::GAP).bold());
                y += 20.0;
            }
            None => {}
        }
        for e in members {
            let ind = if l.d.group_by.is_some() { 16.0 } else { 0.0 };
            s.t(x0 + ind, y, m.name(e), Style::new(12.0, svg::TEXT).bold());
            let inv = l.invs(m, e);
            let mut hdr = m.label_kind(e).to_string();
            if !inv.is_empty() {
                let _ = write!(hdr, " !{}", ranges(&inv));
            }
            s.t(x0 + ind + width_b(m.name(e), 12.0) + 8.0, y, &hdr, Style::new(10.0, svg::MUTED));
            y += 15.0;
            for t in l.outputs(m, e) {
                for ln in wrap(&format!("→ {t}"), w - 30.0, 11.0) {
                    s.t(x0 + ind + 14.0, y, &ln, Style::new(11.0, svg::TEXT));
                    y += 14.0;
                }
            }
            for (t, color) in l.annotations(m, e) {
                s.t(x0 + ind + 14.0, y, &t, Style::new(11.0, color));
                y += 14.0;
            }
            y += 6.0;
        }
        y += 8.0;
    }
    y
}
