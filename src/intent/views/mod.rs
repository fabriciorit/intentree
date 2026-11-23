//! Views of the `.intent` format: built-in views, views declared in the language and three
//! outputs (compact text for an LLM, SVG, JSON).

pub mod data;
pub mod declared;
pub mod builtin;
pub mod deployment;
pub mod freeform;
pub mod model;
pub mod panels;
pub mod constraints;
pub mod svg;
#[cfg(test)]
mod tests;
pub mod text;

use super::analysis::Analysis;
use super::json::J;
use declared::Decl;
use builtin::BuiltinView;
use freeform::Freeform;
use model::Model;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Format {
    Text,
    Svg,
    Json,
}

impl Format {
    pub fn read_text(s: &str) -> Option<Format> {
        match s {
            "text" => Some(Format::Text),
            "svg" => Some(Format::Svg),
            "json" => Some(Format::Json),
            _ => None,
        }
    }
}

/// A view: built-in or declared (index into `Views::decl`).
#[derive(Clone, Debug, PartialEq)]
pub enum ViewKind {
    Builtin(BuiltinView),
    Declared(usize),
}

/// The analysis as seen by all views: the query model and the declared views.
pub struct Views<'a> {
    pub m: Model<'a>,
    pub decl: Vec<Decl>,
}

/// A view with the reason it is suggested.
pub struct Suggestion {
    pub id: String,
    pub reason: String,
}

impl<'a> Views<'a> {
    pub fn new(a: &'a Analysis<'a>) -> Views<'a> {
        Views { m: Model::new(a), decl: declared::declared(a) }
    }

    /// Resolves the typed name: a declared view (wins over a built-in one of the same name), a
    /// built-in view, `machine:<name>` or the name of a state machine.
    pub fn resolve(&self, name: &str) -> Result<ViewKind, String> {
        let n = declared::norm(name);
        if let Some(i) = self.decl.iter().position(|d| declared::norm(&d.name) == n) {
            return Ok(ViewKind::Declared(i));
        }
        match n.as_str() {
            "structure" => return Ok(ViewKind::Builtin(BuiltinView::Structure)),
            "deployment" => return Ok(ViewKind::Builtin(BuiltinView::Deployment)),
            "concurrency" => return Ok(ViewKind::Builtin(BuiltinView::Concurrency)),
            "constraints" => return Ok(ViewKind::Builtin(BuiltinView::Constraints)),
            "roles" => return Ok(ViewKind::Builtin(BuiltinView::Cards)),
            _ => {}
        }
        let (prefix, rest) = match n.split_once(':') {
            Some(("machine", _)) => (true, name.split_once(':').map(|x| x.1).unwrap_or("")),
            _ => (false, name),
        };
        let rn = declared::norm(rest);
        let ms = self.m.machines();
        let found: Vec<&model::Machine> = ms.iter().filter(|q| declared::norm(self.m.name(&q.id)) == rn).collect();
        match found.as_slice() {
            [q] => Ok(ViewKind::Builtin(BuiltinView::Machine(q.id.clone()))),
            [] if prefix => Err(format!("state machine '{rest}' does not exist; the existing ones: {}", self.names_of_machines())),
            [] => Err(format!("unknown view '{name}'; `intentree views` lists the existing ones")),
            _ => Err(format!("there is more than one state machine named '{rest}'")),
        }
    }

    fn names_of_machines(&self) -> String {
        let v: Vec<String> = self.m.machines().iter().map(|q| self.m.name(&q.id).to_string()).collect();
        if v.is_empty() {
            "none".into()
        } else {
            v.join(", ")
        }
    }

    pub fn generate(&self, q: &ViewKind, f: Format) -> String {
        match q {
            ViewKind::Builtin(v) => match f {
                Format::Text => builtin::text(&self.m, v),
                Format::Svg => builtin::svg(&self.m, v),
                Format::Json => builtin::json(&self.m, v).indented() + "\n",
            },
            ViewKind::Declared(i) => {
                let l = Freeform::new(&self.m, &self.decl[*i]);
                match f {
                    Format::Text => freeform::text(&self.m, &l),
                    Format::Svg => freeform::svg(&self.m, &l),
                    Format::Json => freeform::json(&self.m, &l).indented() + "\n",
                }
            }
        }
    }

    /// All the views that exist: the built-in ones (with one per state machine) and the declared
    /// ones, in the order `intentree views` lists them.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn all(&self) -> Vec<ViewKind> {
        let mut v: Vec<ViewKind> = [BuiltinView::Structure, BuiltinView::Deployment, BuiltinView::Concurrency, BuiltinView::Constraints, BuiltinView::Cards].into_iter().map(ViewKind::Builtin).collect();
        v.extend(self.m.machines().into_iter().map(|q| ViewKind::Builtin(BuiltinView::Machine(q.id))));
        v.extend((0..self.decl.len()).map(ViewKind::Declared));
        v
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn id(&self, q: &ViewKind) -> String {
        match q {
            ViewKind::Builtin(f) => builtin::id_of(&self.m, f),
            ViewKind::Declared(i) => self.decl[*i].name.clone(),
        }
    }

    /// Deterministic suggestion from what the description has: always the structure; 2 machines
    /// or more, the deployment; 2 threads or more, concurrency and state; an invariant, rule, gap,
    /// declared absence or warning, the constraints; each state machine, its own view.
    pub fn suggested(&self) -> Vec<Suggestion> {
        let m = &self.m;
        let a = m.a;
        let plural = |n: usize, um: &str, several: &str| format!("{n} {}", if n == 1 { um } else { several });
        let mut v = vec![Suggestion { id: "structure".into(), reason: "always".into() }];
        let n_machines = a.entities.values().filter(|e| e.kind == "e.machine").count();
        if n_machines >= 2 {
            v.push(Suggestion { id: "deployment".into(), reason: plural(n_machines, "machine", "machines") });
        }
        let nth = m.threads().len();
        if nth >= 2 {
            v.push(Suggestion { id: "concurrency".into(), reason: plural(nth, "thread", "threads") });
        }
        let n_gaps: usize = m.gaps.values().map(Vec::len).sum();
        let n_abs: usize = m.absences.values().map(Vec::len).sum();
        let n_warnings: usize = m.warnings.values().map(Vec::len).sum();
        let mut parts: Vec<String> = Vec::new();
        for (n, um, several) in [
            (m.invs.len(), "invariant", "invariants"),
            (m.rules.len(), "rule", "rules"),
            (n_gaps, "gap", "gaps"),
            (n_abs, "declared absence", "declared absences"),
            (n_warnings, "warning", "warnings"),
        ] {
            if n > 0 {
                parts.push(plural(n, um, several));
            }
        }
        if !parts.is_empty() {
            v.push(Suggestion { id: "constraints".into(), reason: parts.join(", ") });
        }
        for q in m.machines() {
            v.push(Suggestion { id: builtin::id_of(m, &BuiltinView::Machine(q.id.clone())), reason: format!("state machine “{}”", m.name(&q.id)) });
        }
        v
    }

    /// `intentree views` as text: the built-in ones, the suggested ones (with the reason) and the declared ones.
    pub fn catalog_text(&self) -> String {
        let mut o = String::new();
        o.push_str("built-in:\n");
        for (id, d) in [
            ("structure", "layers, primitives, machines, threads, roles, invariants, gaps"),
            ("deployment", "machines, processes, endpoints, connections"),
            ("concurrency", "threads, mutable state, state machines"),
            ("constraints", "invariants and rules tied to what they touch, gaps, absences"),
            ("roles", "roles the primitives presuppose"),
            ("machine:<name>", "a state machine"),
        ] {
            o.push_str(&format!("  {id} — {d}\n"));
        }
        o.push_str("suggested:\n");
        for s in self.suggested() {
            o.push_str(&format!("  {} — {}\n", s.id, s.reason));
        }
        o.push_str("declared:\n");
        if self.decl.is_empty() {
            o.push_str("  none\n");
        }
        for d in &self.decl {
            let l = Freeform::new(&self.m, d);
            o.push_str(&format!("  {} — {}\n", d.name, l.description(&self.m)));
        }
        o
    }

    pub fn catalog_json(&self) -> J {
        let s = J::s;
        let builtin = ["structure", "deployment", "concurrency", "constraints", "roles", "machine:<name>"].iter().map(|x| s(x)).collect();
        let sug = self.suggested().iter().map(|x| J::obj(vec![("reason", s(&x.reason)), ("view", s(&x.id))])).collect();
        let dec = self
            .decl
            .iter()
            .map(|d| J::obj(vec![("description", s(&Freeform::new(&self.m, d).description(&self.m))), ("view", s(&d.name))]))
            .collect();
        J::obj(vec![("built_in", J::Arr(builtin)), ("declared", J::Arr(dec)), ("suggested", J::Arr(sug))])
    }
}
