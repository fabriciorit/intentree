//! Deployment view: machines side by side (the largest in the center), processes inside, the
//! endpoints each one listens on and the connections labeled by port or message. Whatever has no
//! declared machine goes in a separate dashed column: the lack is stated, not hidden.

use super::model::Model;
use super::panels::{find_label, geometry, Box};
use super::svg::{self, width, width_b, Style, Svg};
use super::super::analysis::Fact;
use super::super::json::J;
use std::collections::{BTreeMap, BTreeSet};

/// A column: the machine (or none, for whatever has no declared machine) and its processes.
pub struct Column {
    pub machine: Option<String>,
    pub procs: Vec<String>,
}

pub struct Deployment {
    pub columns: Vec<Column>,
    /// web services something reads from, by name
    pub external: Vec<String>,
    /// (source, target) -> labels, sorted
    pub links: BTreeMap<(String, String), BTreeSet<String>>,
    /// process -> endpoints it listens on
    pub listens: BTreeMap<String, Vec<String>>,
    /// who reads each external service
    pub reader: BTreeMap<String, String>,
}

impl Deployment {
    pub fn new(m: &Model) -> Deployment {
        let facts = &m.facts;
        let mut runs_in: BTreeMap<String, String> = BTreeMap::new();
        for f in facts {
            if let (true, Some(o)) = (f.rel() == "a.runs_in", f.obj()) {
                if m.kind(o) == "e.machine" {
                    runs_in.insert(f.subject().to_string(), o.to_string());
                }
            }
        }
        let owner = |e: &str| m.no(e).unwrap_or_else(|| e.to_string());
        let mut listens: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let mut owner_ep: BTreeMap<String, String> = BTreeMap::new();
        for f in facts {
            if let (true, Some(o)) = (f.rel() == "a.listens_on", f.obj()) {
                let p = owner(f.subject());
                listens.entry(p.clone()).or_default().push(o.to_string());
                owner_ep.insert(o.to_string(), p);
            }
        }
        for v in listens.values_mut() {
            m.by_name(v);
            v.dedup();
        }
        let external_all: BTreeSet<String> =
            facts.iter().filter_map(|f| f.role("from")).filter(|e| m.kind(e) == "e.web_service").map(String::from).collect();
        let in_scope = |e: &str| m.structural.contains(e) || external_all.contains(e);
        let mut links: BTreeMap<(String, String), BTreeSet<String>> = BTreeMap::new();
        let mut reader: BTreeMap<String, String> = BTreeMap::new();
        for f in facts {
            if !matches!(f, Fact::Rel { .. }) || f.mode().is_some() {
                continue;
            }
            let o = owner(f.subject());
            if !in_scope(&o) {
                continue;
            }
            let (target, label) = match (f.rel(), f.obj()) {
                ("a.connects_to", Some(ob)) if owner_ep.contains_key(ob) => {
                    (owner_ep[ob].clone(), m.name(ob).rsplit(':').next().unwrap_or("").to_string())
                }
                ("a.sends", Some(ob)) if f.role("to").is_some() => (f.role("to").unwrap().to_string(), m.name(ob).to_string()),
                ("a.launches", Some(ob)) => (ob.to_string(), "launches".to_string()),
                ("a.reads", _) if f.role("from").is_some_and(|x| external_all.contains(x)) => {
                    reader.insert(f.role("from").unwrap().to_string(), o.clone());
                    (f.role("from").unwrap().to_string(), "reads".to_string())
                }
                ("a.writes", Some(ob)) if f.role("to").is_some() => (f.role("to").unwrap().to_string(), format!("writes {}", m.name(ob))),
                _ => continue,
            };
            if target != o && in_scope(&target) {
                links.entry((o, target)).or_default().insert(label);
            }
        }
        // nodes: whatever has a machine, listens, or appears in a link (except externals and machines)
        let mut nodes: BTreeSet<String> = runs_in.keys().cloned().collect();
        nodes.extend(listens.keys().cloned());
        for (o, d) in links.keys() {
            nodes.extend([o.clone(), d.clone()]);
        }
        nodes.retain(|e| !external_all.contains(e) && m.kind(e) != "e.machine" && m.kind(e) != "e.program");
        let mut by_machine: BTreeMap<Option<String>, Vec<String>> = BTreeMap::new();
        for n in &nodes {
            by_machine.entry(runs_in.get(n).cloned()).or_default().push(n.clone());
        }
        // machines that are the target of a link also get a column
        for (_, d) in links.keys() {
            if m.kind(d) == "e.machine" {
                by_machine.entry(Some(d.clone())).or_default();
            }
        }
        let mut order: Vec<Option<String>> = by_machine.keys().filter(|k| k.is_some()).cloned().collect();
        order.sort_by(|a, b| {
            let (na, nb) = (by_machine[a].len(), by_machine[b].len());
            nb.cmp(&na).then(m.name(a.as_deref().unwrap_or("")).cmp(m.name(b.as_deref().unwrap_or(""))))
        });
        // the largest in the center, the others alternating on the sides
        let mut cols: Vec<Option<String>> = Vec::new();
        for (i, q) in order.into_iter().enumerate() {
            if i % 2 == 1 {
                cols.insert(0, q);
            } else {
                cols.push(q);
            }
        }
        if by_machine.contains_key(&None) {
            cols.push(None);
        }
        let columns = cols
            .into_iter()
            .map(|q| {
                let mut procs = by_machine.remove(&q).unwrap_or_default();
                m.by_name(&mut procs);
                Column { machine: q, procs }
            })
            .collect();
        let mut external: Vec<String> = external_all.into_iter().collect();
        m.by_name(&mut external);
        Deployment { columns, external, links, listens, reader }
    }

    /// Lines inside the box of a process: where it runs, what it listens on (with the gap or the
    /// absence of authentication) and how many threads.
    pub fn lines(&self, m: &Model, p: &str) -> Vec<(String, &'static str)> {
        let ctx = m.context(p);
        let mut out: Vec<(String, &'static str)> = Vec::new();
        if !ctx.is_empty() {
            out.push((ctx.join(", "), svg::MUTED));
        }
        for ep in self.listens.get(p).into_iter().flatten() {
            out.push((format!("listens on {} · {}", m.name(ep), m.label_kind(ep).replace(" endpoint", "")), svg::TEXT));
            if m.gaps.get(ep).is_some_and(|v| v.iter().any(|x| x == "authentication")) {
                out.push(("   ? no authentication declared".into(), svg::GAP));
            } else if m.absences.get(ep).is_some_and(|v| v.iter().any(|x| x == "authentication")) {
                out.push(("   ∅ no authentication, on purpose".into(), svg::ABSENT));
            }
        }
        let n = m.contains.get(p).map(|v| v.iter().filter(|e| m.kind(e) == "p.thread").count()).unwrap_or(0);
        if n > 0 {
            out.push((format!("{n} threads"), svg::TEXT));
        }
        out
    }

    /// Entities the view shows (to filter gaps and notes).
    pub fn entities(&self, m: &Model) -> BTreeSet<String> {
        let mut s: BTreeSet<String> = BTreeSet::new();
        for c in &self.columns {
            s.extend(c.machine.iter().cloned());
            for p in &c.procs {
                s.insert(p.clone());
                s.extend(self.listens.get(p).into_iter().flatten().cloned());
            }
        }
        s.extend(self.external.iter().cloned());
        let _ = m;
        s
    }
}

fn label_link(i: &Deployment, o: &str, d: &str) -> String {
    i.links[&(o.to_string(), d.to_string())].iter().cloned().collect::<Vec<_>>().join(", ")
}

/// Draws the view from (x0, y0); returns where it ended.
pub fn svg(s: &mut Svg, m: &Model, x0: f64, y0: f64, width_total: f64) -> f64 {
    let im = Deployment::new(m);
    if im.columns.is_empty() {
        s.t(x0, y0 + 26.0, "no process with a machine, endpoint or link declared", Style::new(11.0, svg::GAP));
        return y0 + 40.0;
    }
    let gap_between = 130.0;
    let title_col = |c: &Column| match &c.machine {
        Some(q) => format!("machine “{}”", m.name(q)),
        None => "no machine declared".to_string(),
    };
    let widths: Vec<f64> = im
        .columns
        .iter()
        .map(|c| {
            let base: f64 = if c.procs.len() <= 1 { 250.0 } else { 540.0 };
            let wide = c.procs.iter().flat_map(|p| im.lines(m, p)).map(|(t, _)| width(&t, 11.0)).fold(0.0, f64::max);
            let base = if c.procs.len() <= 1 { base.max(wide + 46.0) } else { base };
            base.max(width(&title_col(c), 14.0) + 40.0)
        })
        .collect();
    let total = widths.iter().sum::<f64>() + gap_between * (im.columns.len() as f64 - 1.0);
    let mut x = x0 + ((width_total - total) / 2.0).max(0.0);
    let ytop = y0 + 28.0;
    let mut boxes: BTreeMap<String, Box> = BTreeMap::new();
    let mut leaves: Vec<Box> = Vec::new();
    let mut heads: Vec<(usize, f64)> = Vec::new();
    let mut height_max = 0.0f64;
    let key = |c: &Column| c.machine.clone().unwrap_or_else(|| "~no-machine".to_string());
    for (k, (c, wq)) in im.columns.iter().zip(&widths).enumerate() {
        let mut procs = c.procs.clone();
        procs.sort_by(|a, b| im.lines(m, b).len().cmp(&im.lines(m, a).len()).then(m.name(a).cmp(m.name(b))));
        let cols = if *wq < 400.0 { 1 } else { 2 };
        let pw = (wq - 30.0 - (cols as f64 - 1.0) * 14.0) / cols as f64;
        let mut yy = vec![ytop + 40.0; cols];
        for p in &procs {
            let mut ci = 0;
            for j in 1..cols {
                if yy[j] < yy[ci] {
                    ci = j;
                }
            }
            let h = 30.0 + 15.0 * im.lines(m, p).len() as f64;
            let cx = x + 15.0 + ci as f64 * (pw + 14.0);
            boxes.insert(p.clone(), (cx, yy[ci], pw, h));
            leaves.push((cx, yy[ci], pw, h));
            yy[ci] += h + 40.0;
        }
        let alt = yy.iter().cloned().fold(f64::MIN, f64::max) - ytop;
        height_max = height_max.max(alt);
        boxes.insert(key(c), (x, ytop, *wq, alt));
        heads.push((k, x));
        x += wq + gap_between;
    }
    for c in &im.columns {
        let b = boxes[&key(c)];
        boxes.insert(key(c), (b.0, b.1, b.2, height_max));
    }
    let yext = ytop + height_max + 50.0;
    let mut end = yext - 30.0;
    if !im.external.is_empty() {
        s.t(x0, yext - 12.0, "outside the machines", Style::new(12.0, svg::MUTED).bold());
        let mut xe = x0;
        for e in &im.external {
            let w = width(&format!("{} “{}”", m.label_kind(e), m.name(e)), 11.0) + 16.0;
            if let Some(b) = im.reader.get(e).and_then(|l| boxes.get(l)) {
                xe = xe.max(b.0 + b.2 / 2.0 - w / 2.0);
            }
            boxes.insert(e.clone(), (xe, yext, w, 26.0));
            leaves.push((xe, yext, w, 26.0));
            xe += w + 20.0;
        }
        end = yext + 36.0;
    }
    for c in &im.columns {
        let (cx, cy, cw, ch) = boxes[&key(c)];
        let dashed = c.machine.is_none();
        let tagged = s.mark();
        s.r(cx, cy, cw, ch, "#fbfaf6", svg::BORDER, 14.0, dashed);
        let t = title_col(c);
        s.t(cx + 14.0, cy + 24.0, &t, Style::new(14.0, if dashed { svg::GAP } else { svg::TEXT }).bold());
        if let Some(machine) = &c.machine {
            s.tag(tagged, m.name(machine), None);
        }
    }
    let mut labels: Vec<(f64, f64, String, &'static str)> = Vec::new();
    let mut busy: Vec<(f64, f64, f64, f64)> = Vec::new();
    for ((o, d), _) in &im.links {
        let (Some(&(xo, yo, wo, ho)), Some(&(xd, yd, wd, hd))) = (boxes.get(o), boxes.get(d)) else { continue };
        let back = im.links.contains_key(&(d.clone(), o.clone()));
        let dy = if back && o > d {
            8.0
        } else if back {
            -8.0
        } else {
            0.0
        };
        let (p0, p2, curve) = geometry((xo, yo, wo, ho), (xd, yd, wd, hd), dy, &leaves);
        let and_machine = |e: &str| m.kind(e) == "e.machine";
        let color = if !and_machine(o) && !and_machine(d) { svg::A1 } else { svg::MUTED };
        s.arrow(p0.0, p0.1, p2.0, p2.1, color, curve, false);
        let label = label_link(&im, o, d);
        let pos = find_label(p0, p2, curve, &label, &leaves, &mut busy).unwrap_or_else(|| {
            let (mx, my) = ((p0.0 + p2.0) / 2.0 + curve / 2.0, (p0.1 + p2.1) / 2.0 + if dy <= 0.0 { -6.0 } else { 14.0 });
            (mx, my)
        });
        labels.push((pos.0, pos.1, label, color));
    }
    for c in &im.columns {
        for p in &c.procs {
            let (x, y, w, h) = boxes[p];
            let tagged = s.mark();
            s.r(x, y, w, h, svg::BOX, svg::BORDER, 6.0, false);
            s.t(x + 8.0, y + 18.0, m.name(p), Style::new(13.0, svg::TEXT).bold());
            s.t(x + 12.0 + width_b(m.name(p), 13.0), y + 18.0, m.label_kind(p), Style::new(10.0, svg::MUTED));
            for (i, (t, color)) in im.lines(m, p).iter().enumerate() {
                s.t(x + 8.0, y + 35.0 + 15.0 * i as f64, t, Style::new(11.0, color));
            }
            s.tag(tagged, m.name(p), None);
        }
    }
    for e in &im.external {
        let (x, y, w, h) = boxes[e];
        let tagged = s.mark();
        s.r(x, y, w, h, svg::EXT, svg::EXTB, 6.0, true);
        s.t(x + 8.0, y + 17.0, &format!("{} “{}”", m.label_kind(e), m.name(e)), Style::new(11.0, svg::TEXT));
        s.tag(tagged, m.name(e), None);
    }
    for (mx, my, label, color) in &labels {
        let w = width(label, 10.0) + 8.0;
        s.r(mx - w / 2.0, my - 10.0, w, 14.0, "white", svg::BORDER, 3.0, false);
        s.t(*mx, *my, label, Style::new(10.0, color).middle());
    }
    let _ = heads;
    end
}

/// Text: machine by machine, what runs and listens; then the connections.
pub fn text(o: &mut String, m: &Model) {
    use super::text::{nm, section};
    use std::fmt::Write;
    let im = Deployment::new(m);
    section(o, "deployment");
    if im.columns.is_empty() {
        o.push_str("no process with a machine, endpoint or link declared\n");
        return;
    }
    for c in &im.columns {
        match &c.machine {
            Some(q) => {
                let _ = writeln!(o, "machine {}:", nm(m.name(q)));
            }
            None => o.push_str("no machine declared:\n"),
        }
        for p in &c.procs {
            let mut parts: Vec<String> = m.context(p);
            for ep in im.listens.get(p).into_iter().flatten() {
                let auth = if m.gaps.get(ep).is_some_and(|v| v.iter().any(|x| x == "authentication")) {
                    " ?authentication"
                } else if m.absences.get(ep).is_some_and(|v| v.iter().any(|x| x == "authentication")) {
                    " ∅authentication"
                } else {
                    ""
                };
                parts.push(format!("listens on {} ({}){auth}", nm(m.name(ep)), m.label_kind(ep).replace(" endpoint", "")));
            }
            let n = m.contains.get(p).map(|v| v.iter().filter(|e| m.kind(e) == "p.thread").count()).unwrap_or(0);
            if n > 0 {
                parts.push(format!("{n} threads"));
            }
            let _ = writeln!(o, "  {} · {}{}{}", nm(m.name(p)), m.label_kind(p), if parts.is_empty() { "" } else { ": " }, parts.join("; "));
        }
    }
    if !im.links.is_empty() {
        section(o, "connections");
        for ((a, b), _) in &im.links {
            let _ = writeln!(o, "  {} → {}: {}", nm(m.name(a)), nm(m.name(b)), label_link(&im, a, b));
        }
    }
    if !im.external.is_empty() {
        section(o, "outside the machines");
        for e in &im.external {
            let _ = writeln!(o, "  {}", super::text::ent(m, e));
        }
    }
}

pub fn json(m: &Model) -> J {
    let im = Deployment::new(m);
    let s = |x: &str| J::s(x);
    let columns = im
        .columns
        .iter()
        .map(|c| {
            let procs = c
                .procs
                .iter()
                .map(|p| {
                    let eps = im
                        .listens
                        .get(p)
                        .into_iter()
                        .flatten()
                        .map(|ep| {
                            let auth = if m.gaps.get(ep).is_some_and(|v| v.iter().any(|x| x == "authentication")) {
                                "gap"
                            } else if m.absences.get(ep).is_some_and(|v| v.iter().any(|x| x == "authentication")) {
                                "absent"
                            } else {
                                "declared"
                            };
                            J::obj(vec![("authentication", s(auth)), ("name", s(m.name(ep))), ("type", s(m.label_kind(ep)))])
                        })
                        .collect();
                    let n = m.contains.get(p).map(|v| v.iter().filter(|e| m.kind(e) == "p.thread").count()).unwrap_or(0);
                    J::obj(vec![
                        ("context", J::Arr(m.context(p).iter().map(|x| s(x)).collect())),
                        ("listens_on", J::Arr(eps)),
                        ("name", s(m.name(p))),
                        ("threads", J::Num(n as f64)),
                        ("type", s(m.label_kind(p))),
                    ])
                })
                .collect();
            J::obj(vec![("machine", c.machine.as_deref().map(|q| s(m.name(q))).unwrap_or(J::Null)), ("processes", J::Arr(procs))])
        })
        .collect();
    let links = im
        .links
        .iter()
        .map(|((a, b), r)| {
            J::obj(vec![("from", s(m.name(a))), ("labels", J::Arr(r.iter().map(|x| s(x)).collect())), ("to", s(m.name(b)))])
        })
        .collect();
    J::obj(vec![
        ("columns", J::Arr(columns)),
        ("externals", J::Arr(im.external.iter().map(|e| J::obj(vec![("name", s(m.name(e))), ("type", s(m.label_kind(e)))])).collect())),
        ("links", J::Arr(links)),
    ])
}
