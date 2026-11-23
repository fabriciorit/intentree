//! Built-in views in the three outputs. Each view is a list of panels over the same `Model`
//! queries; every view ends with the gaps, warnings and declared absences of the things it
//! shows and with the (informal) notes that touch them.

use super::data;
use super::deployment;
use super::model::{Model, ViewNote};
use super::panels::{self, header, legend};
use super::constraints;
use super::svg::{Style, Svg};
use super::text::{self, section};
use super::super::json::J;
use std::collections::BTreeSet;
use std::fmt::Write;

#[derive(Clone, Debug, PartialEq)]
pub enum BuiltinView {
    Structure,
    Deployment,
    Concurrency,
    Constraints,
    Cards,
    /// id of the state machine
    Machine(String),
}

/// Name of the view as typed.
pub fn id_of(m: &Model, f: &BuiltinView) -> String {
    match f {
        BuiltinView::Structure => "structure".into(),
        BuiltinView::Deployment => "deployment".into(),
        BuiltinView::Concurrency => "concurrency".into(),
        BuiltinView::Constraints => "constraints".into(),
        BuiltinView::Cards => "roles".into(),
        BuiltinView::Machine(q) => format!("machine:{}", m.name(q)),
    }
}

/// What the view shows: gaps and notes are filtered by it.
fn scope(m: &Model, f: &BuiltinView) -> Option<BTreeSet<String>> {
    match f {
        BuiltinView::Structure => None,
        BuiltinView::Deployment => Some(deployment::Deployment::new(m).entities(m)),
        BuiltinView::Concurrency => {
            let ths = m.threads();
            let mut s: BTreeSet<String> = ths.iter().cloned().collect();
            s.extend(m.touch_map(&ths).keys().cloned());
            let (e, l) = m.spelling_read();
            s.extend(e.keys().chain(l.keys()).cloned());
            for q in m.machines() {
                s.insert(q.id.clone());
                s.extend(q.states.iter().cloned());
            }
            Some(s)
        }
        BuiltinView::Constraints => Some(constraints::Constraints::new(m).touched.keys().cloned().collect()),
        BuiltinView::Cards => Some(m.cards().into_iter().map(|(e, _)| e).collect()),
        BuiltinView::Machine(q) => {
            let mq = m.machine(q);
            let mut s: BTreeSet<String> = mq.states.iter().cloned().collect();
            s.insert(q.clone());
            Some(s)
        }
    }
}

fn notes_of_view<'a>(m: &'a Model, esc: &Option<BTreeSet<String>>) -> Vec<&'a ViewNote> {
    match esc {
        None => m.notes_of(true, |_| true),
        Some(s) => m.notes_of(true, |e| s.contains(e)),
    }
}

// ------------------------------------------------------------------ text

pub fn text(m: &Model, f: &BuiltinView) -> String {
    let mut o = String::new();
    let prog = m.program.as_ref().map(|p| format!(" · {}", text::nm(m.name(p)))).unwrap_or_default();
    let _ = writeln!(o, "view {}{prog}", id_of(m, f));
    let esc = scope(m, f);
    let filter = |e: &str| esc.as_ref().is_none_or(|s| s.contains(e));
    match f {
        BuiltinView::Structure => {
            let _ = writeln!(o, "{}", text::LEGEND);
            text::structure(&mut o, m);
            let ms = m.machines();
            if !ms.is_empty() {
                section(&mut o, "state machines");
                text::machines(&mut o, m, &ms);
            }
            if m.threads().len() > 1 {
                section(&mut o, "threads");
                text::threads(&mut o, m);
            }
            if !m.cards().is_empty() {
                section(&mut o, "roles (what the primitives presuppose)");
                text::cards(&mut o, m, &|_| true);
            }
            if !m.invs.is_empty() || !m.rules.is_empty() {
                section(&mut o, "invariants and rules");
                text::invariants(&mut o, m, &|_| true);
                text::rules(&mut o, m);
            }
        }
        BuiltinView::Deployment => deployment::text(&mut o, m),
        BuiltinView::Concurrency => {
            if m.threads().len() > 1 {
                section(&mut o, "threads");
                text::threads(&mut o, m);
            }
            let (e, _) = m.spelling_read();
            if !e.is_empty() {
                section(&mut o, "mutable state (who writes, who reads)");
                text::state(&mut o, m);
            }
            let ms = m.machines();
            if !ms.is_empty() {
                section(&mut o, "state machines");
                text::machines(&mut o, m, &ms);
            }
            if m.threads().len() < 2 && e.is_empty() && ms.is_empty() {
                o.push_str("no thread, mutable state or state machine declared\n");
            }
        }
        BuiltinView::Constraints => constraints::text(&mut o, m),
        BuiltinView::Cards => {
            section(&mut o, "roles (what the primitives presuppose)");
            if m.cards().is_empty() {
                o.push_str("no primitive with presupposed roles\n");
            }
            text::cards(&mut o, m, &|_| true);
        }
        BuiltinView::Machine(q) => {
            text::machines(&mut o, m, &[m.machine(q)]);
        }
    }
    if !matches!(f, BuiltinView::Constraints) {
        text::gaps_with_title(&mut o, m, &filter);
    }
    text::notes_with_title(&mut o, &notes_of_view(m, &esc));
    o
}

// ------------------------------------------------------------------ svg

fn footer(s: &mut Svg, m: &Model, x0: f64, y: f64, w: f64, esc: &Option<BTreeSet<String>>, with_gaps: bool) -> f64 {
    let filter = |e: &str| esc.as_ref().is_none_or(|s| s.contains(e));
    let mut y = y;
    if with_gaps {
        y = panels::gaps(s, m, x0, y, w, &filter) + 10.0;
    }
    panels::notes(s, m, x0, y, w, &notes_of_view(m, esc)) + 10.0
}

pub fn svg(m: &Model, f: &BuiltinView) -> String {
    let mut s = Svg::default();
    let esc = scope(m, f);
    let (width, end) = match f {
        BuiltinView::Structure => {
            let w = 1480.0;
            legend(&mut s, 24.0, 24.0);
            let end_struct = panels::structure(&mut s, m, 24.0, 56.0, 960.0);
            let ms = m.machines();
            let end_machine = panels::machines(&mut s, m, &ms, 1010.0, 56.0, 440.0);
            let end_thr = panels::threads(&mut s, m, 1010.0, end_machine + 4.0, 440.0);
            let end_cards = panels::cards(&mut s, m, 1010.0, end_thr + 4.0, 440.0);
            let y = end_struct.max(end_cards) + 16.0;
            let end_inv = panels::invariants(&mut s, m, 24.0, y, 900.0);
            let end_gap = panels::gaps(&mut s, m, 950.0, y, 520.0, &|_| true);
            let y = end_inv.max(end_gap) + 14.0;
            (w, footer(&mut s, m, 24.0, y, 1400.0, &esc, false))
        }
        BuiltinView::Deployment => {
            header(&mut s, "Deployment", "where each process runs, what it listens on, who connects to whom");
            let y = deployment::svg(&mut s, m, 24.0, 60.0, 1480.0);
            (1560.0, footer(&mut s, m, 24.0, y + 20.0, 1400.0, &esc, true))
        }
        BuiltinView::Concurrency => {
            header(&mut s, "Concurrency and state", "threads, what they share, who changes what, and the state machines");
            let ths = m.threads();
            let mut y = 90.0;
            if ths.len() > 1 {
                y = panels::threads(&mut s, m, 24.0, y, 760.0) + 20.0;
            }
            y = panels::state(&mut s, m, 24.0, y, 760.0).max(y);
            let ms = m.machines();
            let ym = if ms.is_empty() { 90.0 } else { panels::machines(&mut s, m, &ms, 820.0, 90.0, 560.0) };
            if ths.len() < 2 && y <= 90.0 && ms.is_empty() {
                s.t(24.0, 100.0, "no thread, mutable state or state machine declared", Style::new(11.0, super::svg::MUTED));
            }
            (1560.0, footer(&mut s, m, 24.0, y.max(ym) + 20.0, 1400.0, &esc, true))
        }
        BuiltinView::Constraints => {
            let y = constraints::svg(&mut s, m, 24.0, 80.0);
            (1560.0, footer(&mut s, m, 24.0, y + 20.0, 1400.0, &esc, false))
        }
        BuiltinView::Cards => {
            header(&mut s, "Primitive roles", "roles the lexicon presupposes; ? is a gap, ∅ is a declared absence");
            let y = if m.cards().is_empty() {
                s.t(24.0, 90.0, "no primitive with presupposed roles", Style::new(11.0, super::svg::MUTED));
                100.0
            } else {
                panels::cards(&mut s, m, 24.0, 80.0, 700.0)
            };
            (780.0, footer(&mut s, m, 24.0, y + 20.0, 700.0, &esc, true))
        }
        BuiltinView::Machine(q) => {
            let y = panels::machines(&mut s, m, &[m.machine(q)], 24.0, 36.0, 900.0);
            (1000.0, footer(&mut s, m, 24.0, y + 10.0, 900.0, &esc, true))
        }
    };
    s.svg(width, end + 10.0)
}

// ------------------------------------------------------------------ json

pub fn json(m: &Model, f: &BuiltinView) -> J {
    let esc = scope(m, f);
    let filter = |e: &str| esc.as_ref().is_none_or(|s| s.contains(e));
    let mut v: Vec<(&str, J)> = vec![("view", J::s(&id_of(m, f)))];
    if let Some(p) = &m.program {
        v.push(("program", J::s(m.name(p))));
    }
    match f {
        BuiltinView::Structure => {
            v.push(("structure", data::structure(m)));
            v.push(("machines", data::machines(m, &m.machines())));
            v.push(("threads", data::threads(m)));
            v.push(("roles", data::cards(m, &|_| true)));
            v.push(("invariants", data::invariants(m)));
            v.push(("rules", data::rules(m)));
        }
        BuiltinView::Deployment => v.push(("deployment", deployment::json(m))),
        BuiltinView::Concurrency => {
            v.push(("threads", data::threads(m)));
            v.push(("state", data::state(m)));
            v.push(("machines", data::machines(m, &m.machines())));
        }
        BuiltinView::Constraints => v.push(("constraints", constraints::json(m))),
        BuiltinView::Cards => v.push(("roles", data::cards(m, &|_| true))),
        BuiltinView::Machine(q) => v.push(("machines", data::machines(m, &[m.machine(q)]))),
    }
    if !matches!(f, BuiltinView::Constraints) {
        v.push(("gaps", data::gaps(m, &filter)));
    }
    v.push(("notes", data::notes(&notes_of_view(m, &esc))));
    J::obj(v)
}
