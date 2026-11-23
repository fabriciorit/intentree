//! Constraints view: a bipartite graph, the invariants and rules on the left, the entities they
//! touch on the right, ordered by barycenter; gaps, warnings and declared absences go in the
//! entity box.

use super::model::Model;
use super::panels::header;
use super::svg::{self, width_b, wrap as wrap_n, Style, Svg};
use super::super::json::J;
use std::collections::{BTreeMap, BTreeSet};

/// Item of the left column: a numbered invariant (`!n`) or a quantified rule (`∀`).
pub struct ItemR {
    pub label: String,
    pub text: String,
    pub touches: BTreeSet<String>,
    pub color: &'static str,
}

pub struct Constraints {
    pub items: Vec<ItemR>,
    /// entities on the right: touched, or with a gap, warning or declared absence
    pub ents: Vec<String>,
    /// entity -> indices of the items that touch it
    pub touched: BTreeMap<String, BTreeSet<usize>>,
    /// order of the items after the barycenter
    pub order_items: Vec<usize>,
}

impl Constraints {
    pub fn new(m: &Model) -> Constraints {
        let mut items: Vec<ItemR> = m
            .invs
            .iter()
            .enumerate()
            .map(|(i, x)| ItemR { label: format!("!{}", i + 1), text: x.text.clone(), touches: x.touches.clone(), color: svg::INV })
            .collect();
        for r in &m.rules {
            items.push(ItemR { label: "∀".into(), text: r.text.clone(), touches: BTreeSet::from([r.scope.clone()]), color: svg::MUTED });
        }
        let mut touched: BTreeMap<String, BTreeSet<usize>> = BTreeMap::new();
        for (i, it) in items.iter().enumerate() {
            for e in &it.touches {
                if matches!(m.kind(e), "e.value" | "p.state") || m.views.contains(e) {
                    continue;
                }
                touched.entry(e.clone()).or_default().insert(i);
            }
        }
        for e in m.gaps.keys().chain(m.warnings.keys()).chain(m.absences.keys()) {
            touched.entry(e.clone()).or_default();
        }
        let owner = |e: &str| -> String {
            match m.no(e) {
                Some(d) => m.name(&d).to_string(),
                None => format!("~{}", m.name(e)),
            }
        };
        let mut ents: Vec<String> = touched.keys().cloned().collect();
        ents.sort_by(|a, b| (owner(a), m.name(a), a).cmp(&(owner(b), m.name(b), b)));
        let pos_e: BTreeMap<&String, usize> = ents.iter().enumerate().map(|(k, e)| (e, k)).collect();
        let bary_i = |i: usize| -> f64 {
            let ps: Vec<usize> = items[i].touches.iter().filter_map(|e| pos_e.get(e).copied()).collect();
            ps.iter().sum::<usize>() as f64 / ps.len().max(1) as f64
        };
        let mut order_items: Vec<usize> = (0..items.len()).collect();
        order_items.sort_by(|&a, &b| bary_i(a).total_cmp(&bary_i(b)).then(a.cmp(&b)));
        let pos_i: BTreeMap<usize, usize> = order_items.iter().enumerate().map(|(k, &i)| (i, k)).collect();
        let bary_e = |e: &String| -> f64 {
            let v = &touched[e];
            v.iter().map(|i| pos_i[i]).sum::<usize>() as f64 / v.len().max(1) as f64
        };
        ents.sort_by(|a, b| {
            (touched[a].is_empty()).cmp(&touched[b].is_empty()).then(bary_e(a).total_cmp(&bary_e(b))).then(m.name(a).cmp(m.name(b))).then(a.cmp(b))
        });
        Constraints { items, ents, touched, order_items }
    }

    /// Marks of the entity next to the name: `!1 !2 ∀`.
    pub fn marks(&self, e: &str) -> Vec<&str> {
        self.touched[e].iter().map(|&i| self.items[i].label.as_str()).collect()
    }
}

/// Lines inside the box of an entity: (text, color).
pub fn lines_of(m: &Model, e: &str) -> Vec<(String, &'static str)> {
    let mut v = Vec::new();
    let mut ps = m.gaps.get(e).cloned().unwrap_or_default();
    ps.sort();
    for p in ps {
        v.push((format!("? missing {p}"), svg::GAP));
    }
    for t in m.warnings.get(e).into_iter().flatten() {
        v.push((format!("⚠ {t}"), svg::WARNING));
    }
    for p in m.absences.get(e).into_iter().flatten() {
        v.push((format!("∅ no {p}, on purpose"), svg::ABSENT));
    }
    v
}

/// The whole page: header, graph and notes. Returns where it ended.
pub fn svg(s: &mut Svg, m: &Model, x0: f64, y0: f64) -> f64 {
    let r = Constraints::new(m);
    header(s, "Constraints", "what must not break (invariants and rules), what is left unsaid (gaps), what was left out on purpose (absences) and where each thing lands");
    if r.items.is_empty() && r.ents.is_empty() {
        s.t(x0, y0 + 20.0, "no invariant, rule, gap, warning or declared absence", Style::new(11.0, svg::MUTED));
        return y0 + 40.0;
    }
    let (wi, xi) = (600.0, x0);
    let (xe, we) = (x0 + 876.0, 470.0);
    let mut yi = y0;
    let mut box_i: BTreeMap<usize, (f64, f64, f64, f64)> = BTreeMap::new();
    for &i in &r.order_items {
        let n = wrap_n(&r.items[i].text, wi - 50.0, 11.0).len();
        let h = 10.0 + 14.0 * n as f64;
        box_i.insert(i, (xi, yi, wi, h));
        yi += h + 8.0;
    }
    let height_and: BTreeMap<&String, f64> = r.ents.iter().map(|e| (e, 24.0 + 14.0 * lines_of(m, e).len() as f64)).collect();
    let total_e: f64 = height_and.values().sum::<f64>() + 8.0 * r.ents.len() as f64;
    let mut ye = y0 + ((yi - y0 - total_e) / 2.0).max(0.0);
    let mut box_and: BTreeMap<&String, (f64, f64, f64, f64)> = BTreeMap::new();
    for e in &r.ents {
        box_and.insert(e, (xe, ye, we, height_and[e]));
        ye += height_and[e] + 8.0;
    }
    for &i in &r.order_items {
        let (x, y, w, h) = box_i[&i];
        for e in &r.items[i].touches {
            if let Some(&(ex, ey, _, _)) = box_and.get(e) {
                s.raw(format!(
                    "<path d=\"M{:.1} {:.1} C{:.1} {:.1} {:.1} {:.1} {:.1} {:.1}\" fill=\"none\" stroke=\"{}\" stroke-opacity=\"0.35\" stroke-width=\"1\"/>",
                    x + w,
                    y + h / 2.0,
                    x + w + 120.0,
                    y + h / 2.0,
                    ex - 120.0,
                    ey + 12.0,
                    ex,
                    ey + 12.0,
                    svg::INV
                ));
            }
        }
    }
    for &i in &r.order_items {
        let (x, y, w, h) = box_i[&i];
        let it = &r.items[i];
        s.r(x, y, w, h, "white", svg::BORDER, 4.0, false);
        s.t(x + 8.0, y + 15.0, &it.label, Style::new(11.0, it.color).bold());
        for (k, l) in wrap_n(&it.text, w - 50.0, 11.0).iter().enumerate() {
            s.t(x + 38.0, y + 15.0 + 14.0 * k as f64, l, Style::new(11.0, svg::TEXT));
        }
    }
    for e in &r.ents {
        let (x, y, w, h) = box_and[e];
        let lines = lines_of(m, e);
        let risk = m.gaps.contains_key(e) || m.warnings.contains_key(e);
        let (fill, border) = if risk {
            ("#fcf1f1", svg::GAP)
        } else if m.absences.contains_key(e) {
            ("#eef6f2", svg::ABSENT)
        } else {
            (svg::BOX, svg::BORDER)
        };
        let tagged = s.mark();
        s.r(x, y, w, h, fill, border, 4.0, false);
        let location = match m.no(e) {
            Some(d) if d != *e => format!("  in {}", m.name(&d)),
            _ => String::new(),
        };
        s.t(x + 8.0, y + 16.0, m.name(e), Style::new(12.0, svg::TEXT).bold());
        s.t(x + 14.0 + width_b(m.name(e), 12.0), y + 16.0, &format!("{}{location}", m.label_kind(e)), Style::new(10.0, svg::MUTED));
        let marks = r.marks(e);
        if !marks.is_empty() {
            s.t(x + w - 8.0, y + 16.0, &marks.join(" "), Style::new(10.0, svg::INV).bold().end());
        }
        for (k, (t, color)) in lines.iter().enumerate() {
            s.t(x + 14.0, y + 31.0 + 14.0 * k as f64, t, Style::new(11.0, color));
        }
        s.tag(tagged, m.name(e), None);
    }
    yi.max(ye)
}

/// Text: invariants and rules, and the inverse list (entity -> what touches it and what is missing).
pub fn text(o: &mut String, m: &Model) {
    use super::text::{ent, section};
    use std::fmt::Write;
    let r = Constraints::new(m);
    section(o, "constraints");
    if r.items.is_empty() && r.ents.is_empty() {
        o.push_str("no invariant, rule, gap, warning or declared absence\n");
        return;
    }
    for it in &r.items {
        let _ = writeln!(o, "{} {}", it.label, it.text);
    }
    // the sentences already name what they touch: here only what concentrates constraints (two or
    // more) or has a gap, warning or declared absence
    section(o, "where it lands: entities with 2+ constraints, a gap, a warning or an absence");
    let mut order: Vec<&String> = r.ents.iter().filter(|e| r.touched[*e].len() > 1 || !lines_of(m, e).is_empty()).collect();
    order.sort_by(|a, b| (m.kind(a), m.name(a), a).cmp(&(m.kind(b), m.name(b), b)));
    for e in order {
        let mut l = ent(m, e);
        if let Some(d) = m.no(e).filter(|d| d != e) {
            let _ = write!(l, " in {}", super::text::nm(m.name(&d)));
        }
        let marks = r.marks(e);
        if !marks.is_empty() {
            let _ = write!(l, " {}", marks.join(" "));
        }
        for (t, _) in lines_of(m, e) {
            let _ = write!(l, "; {t}");
        }
        let _ = writeln!(o, "{l}");
    }
}

pub fn json(m: &Model) -> J {
    let r = Constraints::new(m);
    let s = |x: &str| J::s(x);
    let names = |es: &BTreeSet<String>| J::Arr(es.iter().filter(|e| r.touched.contains_key(*e)).map(|e| s(m.name(e))).collect());
    let items = r
        .items
        .iter()
        .map(|i| J::obj(vec![("label", s(&i.label)), ("text", s(&i.text)), ("touches", names(&i.touches))]))
        .collect();
    let ents = r
        .ents
        .iter()
        .map(|e| {
            let gap = m.gaps.get(e).cloned().unwrap_or_default();
            let mut v = vec![
                ("name", s(m.name(e))),
                ("type", s(m.label_kind(e))),
                ("marks", J::Arr(r.marks(e).iter().map(|x| s(x)).collect())),
            ];
            if let Some(d) = m.no(e).filter(|d| d != e) {
                v.push(("in", s(m.name(&d))));
            }
            v.push(("gaps", J::Arr(gap.iter().map(|x| s(x)).collect())));
            v.push(("warnings", J::Arr(m.warnings.get(e).into_iter().flatten().map(|x| s(x)).collect())));
            v.push(("absences", J::Arr(m.absences.get(e).into_iter().flatten().map(|x| s(x)).collect())));
            J::obj(v)
        })
        .collect();
    J::obj(vec![("entities", J::Arr(ents)), ("items", J::Arr(items))])
}
