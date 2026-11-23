//! SVG panels: layered structure, state machines, threads, cards, invariants, gaps (with the
//! absences) and notes. Each panel draws from (x0, y0) and returns where it ended.

use super::model::{Item, Machine, Model, Role};
use super::svg::{self, width, width_b, wrap, Style, Svg};
use std::collections::{BTreeMap, BTreeSet};

pub fn title(s: &mut Svg, x: f64, y: f64, t: &str, sub: &str) {
    s.t(x, y, t, Style::new(15.0, svg::TEXT).bold());
    if !sub.is_empty() {
        s.t(x + width(t, 15.0) * 1.08 + 12.0, y, sub, Style::new(12.0, svg::MUTED));
    }
}

pub fn header(s: &mut Svg, t: &str, sub: &str) {
    s.t(24.0, 30.0, t, Style::new(18.0, svg::TEXT).bold());
    s.t(24.0, 50.0, sub, Style::new(12.0, svg::MUTED));
}

pub type Box = (f64, f64, f64, f64);

/// Structure: components in rows (declared layers, or dependency order), the primitives
/// inside, the externals in a band at the bottom; edges behind the boxes, with a label
/// where there is free space.
pub fn structure(s: &mut Svg, m: &Model, x0: f64, y0: f64, width_total: f64) -> f64 {
    let rels = m.relations();
    let edges: Vec<(String, String)> = rels.iter().map(|(o, d, _)| (o.clone(), d.clone())).collect();
    let l = m.lines(&rels);
    let sub = if l.declared {
        "components by declared layer; inside, the primitives; below, what is external"
    } else {
        "components in layers; inside, the primitives; below, what is external"
    };
    title(s, x0, y0, "Structure", sub);
    // box width: that of the widest content (between 196 and 340), so the text does not pass the border
    let wide = |n: &String| {
        let body = m.lines_of_module(n).iter().map(|i| width(&i.text, 11.0)).fold(0.0, f64::max) + 18.0;
        let mark = m.invs_of(|t| m.owner.get(t) == Some(n));
        let hdr = width_b(m.name(n), 13.0) + width(m.label_kind(n), 10.0) + width(&format!("!{}", list_num(&mark, ",")), 11.0) + 30.0;
        body.max(hdr)
    };
    let w = m.modules.iter().map(wide).fold(196.0, f64::max).min(340.0);
    let gap = 16.0;
    let by_line = (((width_total + gap) / (w + gap)).floor() as usize).max(1);
    let mut pos_x: BTreeMap<String, f64> = BTreeMap::new();
    let mut boxes: BTreeMap<String, Box> = BTreeMap::new();
    let mut sublines: Vec<(Option<String>, Vec<String>)> = Vec::new();
    for (label, line) in &l.lines {
        let bary = |n: &String, pos_x: &BTreeMap<String, f64>| -> f64 {
            let parents: Vec<f64> = edges.iter().filter(|(_, d)| d == n).filter_map(|(o, _)| pos_x.get(o).copied()).collect();
            if parents.is_empty() {
                0.0
            } else {
                parents.iter().sum::<f64>() / parents.len() as f64
            }
        };
        let mut order: Vec<(f64, String)> = line.iter().map(|n| (bary(n, &pos_x), n.clone())).collect();
        order.sort_by(|a, b| a.0.total_cmp(&b.0).then(m.name(&a.1).cmp(m.name(&b.1))));
        let order: Vec<String> = order.into_iter().map(|(_, n)| n).collect();
        let parts = order.len().div_ceil(by_line).max(1);
        let size = order.len().div_ceil(parts).max(1);
        for (k, sub) in order.chunks(size).enumerate() {
            let total = sub.len() as f64 * w + (sub.len() as f64 - 1.0) * gap;
            for (j, n) in sub.iter().enumerate() {
                pos_x.insert(n.clone(), x0 + (width_total - total) / 2.0 + j as f64 * (w + gap) + w / 2.0);
            }
            sublines.push((if k == 0 { label.clone() } else { None }, sub.to_vec()));
        }
    }
    let mut items: BTreeMap<String, Vec<Item>> = BTreeMap::new();
    for n in &m.modules {
        items.insert(n.clone(), m.lines_of_module(n));
    }
    let mut y = y0 + 24.0;
    let mut labels_line = Vec::new();
    for (label, sub) in &sublines {
        if let Some(r) = label {
            y += 14.0;
            labels_line.push((y - 4.0, r.clone()));
        }
        let total = sub.len() as f64 * w + (sub.len() as f64 - 1.0) * gap;
        let mut x = x0 + (width_total - total) / 2.0;
        let h = sub.iter().map(|n| 30.0 + 15.0 * items[n].len() as f64 + 6.0).fold(0.0, f64::max);
        for n in sub {
            boxes.insert(n.clone(), (x, y, w, h));
            x += w + gap;
        }
        y += h + 46.0;
    }
    // externals in a band at the bottom, in barycenter order of whatever links to them
    let mut external: Vec<(f64, String)> = m
        .external(&rels)
        .into_iter()
        .map(|e| {
            let nbr: Vec<f64> = rels
                .iter()
                .filter(|(o, d, _)| *o == e || *d == e)
                .flat_map(|(o, d, _)| [o, d])
                .filter_map(|c| boxes.get(c).map(|b| b.0))
                .collect();
            let b = if nbr.is_empty() { 9999.0 } else { nbr.iter().sum::<f64>() / nbr.len() as f64 };
            (b, e)
        })
        .collect();
    external.sort_by(|a, b| a.0.total_cmp(&b.0).then(m.name(&a.1).cmp(m.name(&b.1))));
    let external: Vec<String> = external.into_iter().map(|(_, e)| e).collect();
    let label_ext = |e: &str| m.np(e) + &m.deadline.get(e).map(|p| format!(" ⏱{p}")).unwrap_or_default();
    let (mut xe, mut ye) = (x0, y + 4.0);
    for e in &external {
        let wb = width(&label_ext(e), 11.0) + 16.0;
        if xe + wb > x0 + width_total && xe > x0 {
            xe = x0;
            ye += 40.0;
        }
        boxes.insert(e.clone(), (xe, ye, wb, 24.0));
        xe += wb + 14.0;
    }
    let end = if external.is_empty() { y } else { ye + 40.0 };
    let ext: BTreeSet<&String> = external.iter().collect();
    let mut labels: Vec<(f64, f64, String)> = Vec::new();
    let mut busy: Vec<(f64, f64, f64, f64)> = Vec::new();
    for (o, d, label) in &rels {
        let (xo, yo, wo, ho) = boxes[o];
        let (xd, yd, wd, hd) = boxes[d];
        let color = if ext.contains(d) || ext.contains(o) { svg::EXTB } else { svg::MUTED };
        let (p0, p2, c);
        if (yo - yd).abs() < 1.0 {
            if xd > xo {
                (p0, p2, c) = ((xo + wo, yo + 14.0), (xd - 2.0, yd + 14.0), 0.0);
            } else {
                (p0, p2, c) = ((xo, yo + ho - 14.0), (xd + wd + 2.0, yd + hd - 14.0), 0.0);
            }
        } else if yd > yo {
            p0 = (xo + wo / 2.0, yo + ho);
            p2 = (xd + wd / 2.0, yd - 2.0);
            c = if yd - (yo + ho) > 60.0 && !ext.contains(d) { 70.0 } else { 0.0 };
        } else {
            (p0, p2, c) = ((xo + wo / 2.0 + 24.0, yo), (xd + wd / 2.0 + 24.0, yd + hd + 2.0), 90.0);
        }
        let back = l.back.contains(&(o.clone(), d.clone()));
        s.arrow(p0.0, p0.1, p2.0, p2.1, color, c, back);
        if label == "uses" {
            continue;
        }
        if let Some((mx, my)) = find_label(p0, p2, c, label, &boxes.values().copied().collect::<Vec<_>>(), &mut busy) {
            labels.push((mx, my, label.clone()));
        }
    }
    for (yl, r) in &labels_line {
        s.t(x0, *yl, r, Style::new(11.0, svg::MUTED).bold());
    }
    for e in &m.modules {
        let (x, yb, wb, h) = boxes[e];
        let mark_inv = m.invs_of(|t| m.owner.get(t) == Some(e));
        let tagged = s.mark();
        s.r(x, yb, wb, h, svg::BOX, svg::BORDER, 6.0, false);
        s.t(x + 8.0, yb + 17.0, m.name(e), Style::new(13.0, svg::TEXT).bold());
        let mark = if mark_inv.is_empty() { String::new() } else { format!("!{}", list_num(&mark_inv, ",")) };
        if width_b(m.name(e), 13.0) + width(m.label_kind(e), 10.0) + width(&mark, 11.0) + 26.0 < wb {
            s.t(x + 8.0 + width_b(m.name(e), 13.0) + 6.0, yb + 17.0, m.label_kind(e), Style::new(10.0, svg::MUTED));
        }
        if !mark.is_empty() {
            s.t(x + wb - 6.0, yb + 17.0, &mark, Style::new(11.0, svg::INV).bold().end());
        }
        for (i, it) in items[e].iter().enumerate() {
            s.t(x + 8.0, yb + 34.0 + 15.0 * i as f64, &it.text, Style::new(11.0, it.color));
        }
        s.tag(tagged, m.name(e), None);
    }
    for e in &external {
        let (x, yb, wb, h) = boxes[e];
        let tagged = s.mark();
        s.r(x, yb, wb, h, svg::EXT, svg::EXTB, 6.0, true);
        s.t(x + 8.0, yb + 16.0, &label_ext(e), Style::new(11.0, svg::TEXT));
        marks_corner(s, m, e, x + wb - 4.0, yb - 3.0);
        s.tag(tagged, m.name(e), None);
    }
    for (mx, my, label) in &labels {
        let wr = width(label, 10.0) + 6.0;
        s.r(mx - wr / 2.0, my - 9.0, wr, 13.0, "white", "white", 2.0, false);
        s.t(*mx, my + 1.0, label, Style::new(10.0, svg::MUTED).middle());
    }
    end
}

/// Ends and curvature of the arrow between two boxes: side to side when they do not overlap in x;
/// in a column, bottom to top (or the reverse), and if there is a box in between, it goes around
/// on the right. `dy` separates the two arrows of a there-and-back pair.
pub fn geometry(o: Box, d: Box, dy: f64, leaves: &[Box]) -> ((f64, f64), (f64, f64), f64) {
    let ((xo, yo, wo, ho), (xd, yd, wd, hd)) = (o, d);
    if xd > xo + wo {
        ((xo + wo, yo + ho / 2.0 + dy), (xd - 2.0, yd + hd / 2.0 + dy), 0.0)
    } else if xd + wd < xo {
        ((xo, yo + ho / 2.0 + dy), (xd + wd + 2.0, yd + hd / 2.0 + dy), 0.0)
    } else {
        let (top, bottom) = if yd > yo { (yo + ho, yd) } else { (yd + hd, yo) };
        let (x1, x2) = (xo.min(xd), (xo + wo).max(xd + wd));
        let in_middle = leaves.iter().any(|&b| b != o && b != d && b.0 < x2 && b.0 + b.2 > x1 && b.1 + b.3 > top && b.1 < bottom);
        if in_middle {
            ((xo + wo, yo + ho / 2.0 + dy), (xd + wd + 2.0, yd + hd / 2.0 + dy), 56.0)
        } else if yd > yo {
            ((xo + wo / 2.0 + dy, yo + ho), (xd + wd / 2.0 + dy, yd - 2.0), 0.0)
        } else {
            ((xo + wo / 2.0 + dy, yo), (xd + wd / 2.0 + dy, yd + hd + 2.0), 0.0)
        }
    }
}

/// Point of the arrow (quadratic from `p0` to `p2`, control offset by `c` in x) where the label fits
/// without covering a box or another label; records the rectangle in `busy`.
pub fn find_label(p0: (f64, f64), p2: (f64, f64), c: f64, label: &str, boxes: &[Box], busy: &mut Vec<(f64, f64, f64, f64)>) -> Option<(f64, f64)> {
    let (cx, cy) = ((p0.0 + p2.0) / 2.0 + c, (p0.1 + p2.1) / 2.0);
    let wr = width(label, 10.0) + 6.0;
    for t in [0.5, 0.35, 0.65, 0.2, 0.8] {
        let mx = (1.0 - t) * (1.0 - t) * p0.0 + 2.0 * (1.0 - t) * t * cx + t * t * p2.0;
        let my = (1.0 - t) * (1.0 - t) * p0.1 + 2.0 * (1.0 - t) * t * cy + t * t * p2.1;
        let ret = (mx - wr / 2.0, my - 9.0, mx + wr / 2.0, my + 4.0);
        let freeform_box = boxes.iter().all(|&(bx, by, bw, bh)| ret.2 < bx - 2.0 || ret.0 > bx + bw + 2.0 || ret.3 < by - 2.0 || ret.1 > by + bh + 2.0);
        let freeform_label = busy.iter().all(|r| ret.2 < r.0 || ret.0 > r.2 || ret.3 < r.1 || ret.1 > r.3);
        if freeform_box && freeform_label {
            busy.push(ret);
            return Some((mx, my));
        }
    }
    None
}

/// `?` (gap) and `∅` (absence) at the corner of a box, right-aligned.
fn marks_corner(s: &mut Svg, m: &Model, e: &str, x: f64, y: f64) {
    let mut x = x;
    if m.absences.contains_key(e) {
        s.t(x, y, "∅", Style::new(12.0, svg::ABSENT).bold().end());
        x -= 12.0;
    }
    if m.gaps.contains_key(e) {
        s.t(x, y, "?", Style::new(12.0, svg::GAP).bold().end());
    }
}

/// "1,2,3" (the SVG writes the whole list).
pub fn list_num(v: &[usize], sep: &str) -> String {
    v.iter().map(|n| n.to_string()).collect::<Vec<_>>().join(sep)
}

/// State machines: states in columns by depth from the initial one, transitions
/// colored by who triggers them, the long way back underneath.
pub fn machines(s: &mut Svg, m: &Model, ms: &[Machine], x0: f64, y0: f64, w: f64) -> f64 {
    let mut y = y0;
    for machine in ms {
        let tagged = s.mark();
        title(s, x0, y, &format!("State machine “{}”", m.name(&machine.id)), "");
        s.tag(tagged, m.name(&machine.id), None);
        if machine.states.is_empty() {
            s.t(x0 + 10.0, y + 24.0, "? no state declared", Style::new(11.0, svg::GAP));
            y += 50.0;
            continue;
        }
        let bw = machine.states.iter().map(|e| width(m.name(e), 11.0) + 16.0).fold(100.0, f64::max);
        let mut outputs: BTreeMap<&String, Vec<&(String, String, String)>> = BTreeMap::new();
        for e in &machine.states {
            outputs.insert(e, machine.transitions.iter().filter(|t| t.0 == *e).collect());
        }
        let root = match &machine.initial {
            Some(i) => i.clone(),
            None => machine
                .states
                .iter()
                .max_by(|a, b| (outputs[a].len(), m.name(a)).cmp(&(outputs[b].len(), m.name(b))))
                .cloned()
                .unwrap_or_default(),
        };
        let mut level: BTreeMap<String, usize> = BTreeMap::new();
        level.insert(root.clone(), 0);
        fn dfs(e: &str, outputs: &BTreeMap<&String, Vec<&(String, String, String)>>, level: &mut BTreeMap<String, usize>, stack: &mut Vec<String>) {
            if stack.len() > 64 {
                return;
            }
            stack.push(e.to_string());
            for (_, d, _) in outputs.get(&e.to_string()).into_iter().flatten().copied() {
                if stack.contains(d) {
                    continue;
                }
                let v = (level.get(d).copied().unwrap_or(0)).max(level.get(e).copied().unwrap_or(0) + 1);
                level.insert(d.clone(), v);
                dfs(d, outputs, level, stack);
            }
            stack.pop();
        }
        dfs(&root, &outputs, &mut level, &mut Vec::new());
        let mut cols: BTreeMap<usize, Vec<String>> = BTreeMap::new();
        for e in &machine.states {
            cols.entry(level.get(e).copied().unwrap_or(0)).or_default().push(e.clone());
        }
        let ncols = cols.len();
        let step = ((w - bw - 10.0) / (ncols.max(2) - 1) as f64).clamp(bw + 30.0, bw + 50.0);
        let n1 = cols.get(&1).map(Vec::len).unwrap_or(0);
        let mut pos: Vec<(String, f64, f64)> = Vec::new();
        for (c, es) in &cols {
            let mut es = es.clone();
            m.by_name(&mut es);
            for (i, e) in es.iter().enumerate() {
                let offset = if es.len() == 1 && n1 > 1 { 31.0 } else { 0.0 };
                pos.push((e.clone(), x0 + 10.0 + *c as f64 * step, y + 30.0 + i as f64 * 62.0 + offset));
            }
        }
        let p = |e: &str| pos.iter().find(|(x, _, _)| x == e).map(|(_, a, b)| (*a, *b)).unwrap_or((x0, y));
        let mut subjects: Vec<String> = machine.transitions.iter().map(|t| t.2.clone()).collect::<BTreeSet<_>>().into_iter().collect();
        m.by_name(&mut subjects);
        let color_ag = |a: &str| if subjects.iter().position(|x| x == a).unwrap_or(0) % 2 == 0 { svg::A1 } else { svg::A2 };
        let mut seen: BTreeSet<(&String, &String)> = BTreeSet::new();
        for (o, d, ag) in &machine.transitions {
            let ((xo, yo), (xd, yd)) = (p(o), p(d));
            let pair = seen.contains(&(d, o));
            seen.insert((o, d));
            let color = color_ag(ag);
            let k = if pair { 6.0 } else { 0.0 };
            if xd > xo + 1.0 {
                s.arrow(xo + bw, yo + 12.0 + k, xd - 2.0, yd + 12.0 + k, color, 0.0, false);
            } else if xd < xo - 1.0 && xo - xd > step + 10.0 {
                let bottom = yo.max(yd) + 24.0;
                s.raw(format!(
                    "<path d=\"M{:.1} {:.1} Q{:.1} {:.1} {:.1} {:.1}\" fill=\"none\" stroke=\"{color}\" stroke-width=\"1\" marker-end=\"url(#m{})\"/>",
                    xo + bw / 2.0,
                    yo + 24.0,
                    (xo + xd) / 2.0 + bw / 2.0,
                    bottom + 46.0,
                    xd + bw / 2.0,
                    yd + 26.0,
                    &color[1..]
                ));
            } else if xd < xo - 1.0 {
                s.arrow(xo, yo + 20.0, xd + bw, yd + 20.0, color, 0.0, false);
            } else {
                let (y1, y2) = if yd > yo { (yo + 24.0, yd - 2.0) } else { (yo, yd + 26.0) };
                s.arrow(xo + bw / 2.0, y1, xd + bw / 2.0, y2, color, if pair { -30.0 } else { 30.0 }, false);
            }
        }
        for (e, x, yy) in &pos {
            let tagged = s.mark();
            s.r(*x, *yy, bw, 24.0, "white", svg::BORDER, 12.0, false);
            let mark = if *e == root && machine.initial.is_none() { " ◂" } else { "" };
            s.t(x + bw / 2.0, yy + 16.0, &format!("{}{mark}", m.name(e)), Style::new(11.0, svg::TEXT).middle());
            if m.gaps.contains_key(e) {
                s.t(x + bw - 2.0, yy - 3.0, "?", Style::new(12.0, svg::GAP).bold().end());
            }
            s.tag(tagged, m.name(&machine.id), Some(m.name(e)));
        }
        let long = machine.transitions.iter().any(|(o, d, _)| p(o).0 - p(d).0 > step + 10.0);
        let ybase = pos.iter().map(|(_, _, yy)| *yy).fold(f64::MIN, f64::max) + if long { 64.0 } else { 44.0 };
        let mut xl = x0 + 10.0;
        for ag in &subjects {
            let t = format!("triggered by {}", m.name(ag));
            s.r(xl, ybase - 9.0, 10.0, 10.0, color_ag(ag), color_ag(ag), 2.0, false);
            s.t(xl + 16.0, ybase, &t, Style::new(11.0, svg::MUTED));
            xl += (16.0 + width(&t, 11.0) + 18.0).max(170.0);
        }
        let mut yend = ybase;
        if machine.initial.is_none() {
            yend += 18.0;
            if m.absences.get(&machine.id).is_some_and(|v| v.iter().any(|p| p == "initial state")) {
                s.t(x0 + 10.0, yend, "◂ assumed root: no initial state, on purpose (declared absence)", Style::new(11.0, svg::ABSENT));
            } else {
                s.t(x0 + 10.0, yend, "◂ assumed root: initial state not declared (gap)", Style::new(11.0, svg::GAP));
            }
        }
        let without_output: Vec<&str> = machine.states.iter().filter(|e| m.gaps.contains_key(*e)).map(|e| m.name(e)).collect();
        if !without_output.is_empty() {
            yend += 18.0;
            s.t(x0 + 10.0, yend, &format!("? no outgoing transition: {}", without_output.join(", ")), Style::new(11.0, svg::GAP));
        }
        y = yend + 40.0;
    }
    y
}

/// Threads: who touches each primitive, and how. Columns as wide as the content.
pub fn threads(s: &mut Svg, m: &Model, x0: f64, y0: f64, w: f64) -> f64 {
    let ths = m.threads();
    if ths.len() < 2 {
        return y0;
    }
    let touch_map = m.touch_map(&ths);
    let mut targets: Vec<&String> = touch_map.keys().collect();
    targets.sort_by(|a, b| touch_map[*b].len().cmp(&touch_map[*a].len()).then(m.name(a).cmp(m.name(b))));
    if targets.is_empty() {
        return y0;
    }
    title(s, x0, y0, "Threads", "who touches each primitive, and how");
    let label_target = |e: &str| format!("{}{}", m.name(e), if m.protected(e).is_empty() { "" } else { " 🔒" });
    let col0 = targets.iter().map(|e| width(&label_target(e), 10.0) + 12.0).fold(128.0, f64::max);
    let base = (w - col0) / ths.len() as f64;
    let subs: Vec<Vec<(String, &str)>> = ths
        .iter()
        .map(|t| {
            let mut v: Vec<(String, &str)> = m.period.get(t).map(|p| (format!("⟳ {p}"), svg::MUTED)).into_iter().collect();
            v.extend(m.rules_of_thread(t).into_iter().map(|r| (r, svg::INV)));
            v
        })
        .collect();
    let cell = |e: &String, t: &String| touch_map[e].get(t).map(|v| v.iter().cloned().collect::<Vec<_>>().join(", "));
    let mut xs = Vec::new();
    let mut x = x0 + col0;
    for (j, t) in ths.iter().enumerate() {
        xs.push(x);
        let mut cw = width(m.name(t), 11.0);
        for (r, _) in &subs[j] {
            cw = cw.max(width(r, 10.0));
        }
        for e in &targets {
            if let Some(c) = cell(e, t) {
                cw = cw.max(width(&c, 10.0));
            }
        }
        x += base.max(cw + 10.0);
    }
    let wt = x - x0;
    let mut y = y0 + 22.0;
    for (j, t) in ths.iter().enumerate() {
        let tagged = s.mark();
        s.t(xs[j] + 4.0, y, m.name(t), Style::new(11.0, svg::TEXT).bold());
        s.tag(tagged, m.name(t), None);
        for (k, (r, color)) in subs[j].iter().enumerate() {
            s.t(xs[j] + 4.0, y + 13.0 + 12.0 * k as f64, r, Style::new(10.0, color));
        }
    }
    y += 14.0 + 12.0 * subs.iter().map(Vec::len).max().unwrap_or(0).max(1) as f64;
    for e in &targets {
        let several = touch_map[*e].len() > 1;
        let tagged = s.mark();
        s.r(x0, y - 11.0, wt, 16.0, if several { svg::BOX } else { "white" }, svg::BORDER, 2.0, false);
        let style = if several { Style::new(10.0, svg::STATE).bold() } else { Style::new(10.0, svg::TEXT) };
        s.t(x0 + 4.0, y + 1.0, &label_target(e), style);
        s.tag(tagged, m.name(e), None);
        for (j, t) in ths.iter().enumerate() {
            if let Some(c) = cell(e, t) {
                s.t(xs[j] + 4.0, y + 1.0, &c, Style::new(10.0, svg::TEXT));
            }
        }
        y += 18.0;
    }
    s.t(x0, y + 4.0, "highlighted row: primitive touched by more than one thread", Style::new(10.0, svg::MUTED));
    y + 24.0
}

/// Mutable state: primitive, who writes and who reads; those touched by more than one thing highlighted.
pub fn state(s: &mut Svg, m: &Model, x0: f64, y0: f64, w: f64) -> f64 {
    let (writes, reads) = m.spelling_read();
    if writes.is_empty() {
        return y0;
    }
    title(s, x0, y0, "Mutable state", "primitive, who writes, who reads");
    let mut es: Vec<&String> = writes.keys().collect();
    es.sort_by(|a, b| {
        let (na, nb) = (writes[*a].len() + reads.get(*a).map_or(0, |x| x.len()), writes[*b].len() + reads.get(*b).map_or(0, |x| x.len()));
        nb.cmp(&na).then(m.name(a).cmp(m.name(b)))
    });
    let list = |x: &BTreeSet<String>| x.iter().cloned().collect::<Vec<_>>().join(", ");
    let label = |e: &String| format!("{} · {}", m.name(e), m.label_kind(e));
    let c1 = es.iter().map(|e| width(&label(e), 11.0)).fold(0.0, f64::max) + 24.0;
    let c2 = es.iter().map(|e| width(&format!("writes: {}", list(&writes[*e])), 11.0)).fold(0.0, f64::max) + 20.0;
    let wt = w.max(c1 + c2 + es.iter().map(|e| width(&format!("reads: {}", reads.get(*e).map(&list).unwrap_or_default()), 11.0)).fold(0.0, f64::max) + 12.0);
    let mut y = y0 + 22.0;
    for e in es {
        let several = writes[e].union(reads.get(e).unwrap_or(&BTreeSet::new())).count() > 1;
        let tagged = s.mark();
        s.r(x0, y - 12.0, wt, 18.0, if several { svg::BOX } else { "white" }, svg::BORDER, 2.0, false);
        s.t(x0 + 6.0, y + 1.0, &label(e), if several { Style::new(11.0, svg::STATE).bold() } else { Style::new(11.0, svg::TEXT).bold() });
        s.tag(tagged, m.name(e), None);
        s.t(x0 + c1, y + 1.0, &format!("writes: {}", list(&writes[e])), Style::new(11.0, svg::TEXT));
        if let Some(r) = reads.get(e) {
            s.t(x0 + c1 + c2, y + 1.0, &format!("reads: {}", list(r)), Style::new(11.0, svg::MUTED));
        }
        y += 20.0;
    }
    y + 6.0
}

/// Cards of the primitives with presupposed roles.
pub fn cards(s: &mut Svg, m: &Model, x0: f64, y0: f64, w: f64) -> f64 {
    let fs = m.cards();
    if fs.is_empty() {
        return y0;
    }
    title(s, x0, y0, "Primitive roles", "roles the lexicon presupposes");
    let mut y = y0 + 16.0;
    for (e, lines) in &fs {
        let h = 20.0 + 14.0 * lines.len() as f64;
        let tagged = s.mark();
        s.r(x0, y, w, h, svg::BOX, svg::BORDER, 6.0, false);
        s.t(x0 + 8.0, y + 15.0, &m.np(e), Style::new(11.0, svg::TEXT).bold());
        for (i, (role, val)) in lines.iter().enumerate() {
            let yy = y + 30.0 + 14.0 * i as f64;
            s.t(x0 + 14.0, yy, &format!("{role}:"), Style::new(11.0, svg::MUTED));
            let (t, color) = match val {
                Role::Value(v) => (v.clone(), svg::TEXT),
                Role::Absent => ("∅ none, on purpose".to_string(), svg::ABSENT),
                Role::Gap => ("? not declared".to_string(), svg::GAP),
            };
            s.t(x0 + 18.0 + width(&format!("{role}: "), 11.0), yy, &t, Style::new(11.0, color));
        }
        s.tag(tagged, m.name(e), None);
        y += h + 6.0;
    }
    y
}

/// Numbered invariants and, after them, the quantified rules (∀).
pub fn invariants(s: &mut Svg, m: &Model, x0: f64, y0: f64, w: f64) -> f64 {
    title(s, x0, y0, "Invariants", "");
    let mut y = y0 + 20.0;
    let items = m.invs.iter().enumerate().map(|(i, x)| (format!("!{}", i + 1), &x.text, svg::INV));
    let rules = m.rules.iter().map(|r| ("∀".to_string(), &r.text, svg::MUTED));
    for (label, txt, color) in items.chain(rules) {
        s.t(x0, y, &label, Style::new(11.0, color).bold());
        let lines = wrap(txt, w - 30.0, 11.0);
        for (k, l) in lines.iter().enumerate() {
            s.t(x0 + 26.0, y + 14.0 * k as f64, l, Style::new(11.0, svg::TEXT));
        }
        y += 14.0 * lines.len() as f64 + 6.0;
    }
    y
}

/// Gaps ("the description does not say"), warnings and declared absences ("none, on purpose").
pub fn gaps(s: &mut Svg, m: &Model, x0: f64, y0: f64, w: f64, filter: &dyn Fn(&str) -> bool) -> f64 {
    title(s, x0, y0, "Gaps, warnings and absences", "the description does not say (?), warning (⚠), none on purpose (∅)");
    let mut y = y0 + 20.0;
    let v = texts_of_gaps(m, filter);
    if v.is_empty() {
        s.t(x0, y, "none", Style::new(11.0, svg::MUTED));
        return y + 15.0;
    }
    for (t, color) in v {
        for (k, l) in wrap(&t, w, 11.0).iter().enumerate() {
            s.t(x0 + if k > 0 { 12.0 } else { 0.0 }, y, l, Style::new(11.0, color));
            y += 15.0;
        }
        y += 2.0;
    }
    y
}

/// The lines of the gaps panel, also used in the text.
pub fn texts_of_gaps(m: &Model, filter: &dyn Fn(&str) -> bool) -> Vec<(String, &'static str)> {
    let mut v = Vec::new();
    let sort = |ks: Vec<&String>| -> Vec<String> {
        let mut ks: Vec<String> = ks.into_iter().filter(|e| filter(e)).cloned().collect();
        ks.sort_by(|a, b| (m.kind(a), m.name(a)).cmp(&(m.kind(b), m.name(b))));
        ks
    };
    let mut warns: Vec<&String> = m.warnings.keys().filter(|e| filter(e)).collect();
    warns.sort_by(|a, b| m.name(a).cmp(m.name(b)));
    for e in warns {
        for t in &m.warnings[e] {
            v.push((format!("⚠ {}: {t}", m.np(e)), svg::WARNING));
        }
    }
    for e in sort(m.gaps.keys().collect()) {
        let mut ps = m.gaps[&e].clone();
        ps.sort();
        v.push((format!("? {}: missing {}", m.np(&e), ps.join(", ")), svg::GAP));
    }
    for e in sort(m.absences.keys().collect()) {
        v.push((format!("∅ {}: no {}, on purpose", m.np(&e), m.absences[&e].join(", ")), svg::ABSENT));
    }
    v
}

/// Free notes: informal, not part of the IR.
pub fn notes(s: &mut Svg, m: &Model, x0: f64, y0: f64, w: f64, ns: &[&super::model::ViewNote]) -> f64 {
    if ns.is_empty() {
        return y0;
    }
    let _ = m;
    title(s, x0, y0, "Notes", "informal: free text, outside the IR");
    let mut y = y0 + 20.0;
    for n in ns {
        for l in wrap(&format!("— {}", n.text), w, 11.0) {
            s.t(x0, y, &l, Style::new(11.0, svg::TEXT).italic());
            y += 14.0;
        }
        if let Some(f) = &n.sentence {
            for l in wrap(&format!("about: {f}"), w - 12.0, 10.0) {
                s.t(x0 + 12.0, y, &l, Style::new(10.0, svg::MUTED));
                y += 13.0;
            }
        }
        y += 4.0;
    }
    y
}

pub fn legend(s: &mut Svg, x0: f64, y0: f64) {
    let items = [
        ("●", svg::STATE, "mutable state (← who changes it)"),
        ("!n", svg::INV, "invariant n touches the component"),
        ("?", svg::GAP, "gap: the description does not say"),
        ("∅", svg::ABSENT, "declared absence: none, on purpose"),
        ("⚠", svg::WARNING, "warning derived from the lexicon"),
    ];
    let mut x = x0;
    for (sym, color, txt) in items {
        s.t(x, y0, sym, Style::new(12.0, color).bold());
        s.t(x + 18.0, y0, txt, Style::new(11.0, svg::MUTED));
        x += 40.0 + width(txt, 11.0);
    }
}
