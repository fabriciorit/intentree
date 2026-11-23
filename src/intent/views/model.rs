//! Queries over the analysis that the views use. Everything in `BTreeMap`/`BTreeSet` or in sorted
//! lists: no output depends on the iteration order of a `HashMap`.

use super::super::analysis::{Analysis, Fact, Value};
use super::super::canon;
use super::super::json::number;
use super::svg;
use std::collections::{BTreeMap, BTreeSet};

/// Kind of the views declared in the language itself (a word of the tool).
pub const VIEW: &str = "e.view";

/// Relations that do not become a structure edge: state, primitive role, anchoring,
/// layers and those that only say where something lives.
const IGNORE: [&str; 25] = [
    "a.contains",
    "a.has_states",
    "a.moves",
    "a.has_same_unit_as",
    "a.has_unit",
    "a.starts_in",
    "a.keyed_by",
    "a.protected_by",
    "a.repeats_every",
    "a.times_out_after",
    "a.ordered_by",
    "a.breaks_ties_by",
    "a.seeded_with",
    "a.measures",
    "a.parameterized_by",
    "a.follows",
    "a.pins",
    "a.receives",
    "a.returns",
    "a.validates",
    "a.changes",
    "a.implemented_in",
    "a.excludes",
    "a.is_in",
    "a.above",
];

/// Numbered invariant (the number is the position + 1): Portuguese sentence and the entities it touches.
pub struct Inv {
    pub text: String,
    pub touches: BTreeSet<String>,
}

/// Quantified rule: Portuguese sentence and the set it applies to.
pub struct ViewRule {
    pub text: String,
    pub scope: String,
}

/// Free (informal) note: text, the Portuguese sentence it is attached to and its entities.
pub struct ViewNote {
    pub text: String,
    pub sentence: Option<String>,
    pub ents: BTreeSet<String>,
}

/// A line inside a component's box: text and the kind of mark (for the color).
#[derive(Clone)]
pub struct Item {
    pub text: String,
    pub color: &'static str,
    /// continuation of the line above (← changed by, marked fields)
    pub sub: bool,
}

/// Rows of components of the structure panel: label (declared layer, or nothing when the
/// order comes from the dependencies), components in each row, level of each one and edges that go up.
pub struct Lines {
    pub declared: bool,
    pub lines: Vec<(Option<String>, Vec<String>)>,
    pub back: BTreeSet<(String, String)>,
}

/// State machine: states, declared initial state and transitions (from, to, who triggers).
pub struct Machine {
    pub id: String,
    pub states: Vec<String>,
    pub initial: Option<String>,
    pub transitions: Vec<(String, String, String)>,
}

/// Value of a presupposed role on a card.
#[derive(Clone, PartialEq)]
pub enum Role {
    Value(String),
    /// "none, on purpose" (declared absence)
    Absent,
    /// "the description does not say" (gap)
    Gap,
}

pub struct Model<'a> {
    pub a: &'a Analysis<'a>,
    /// facts without the view declarations
    pub facts: Vec<&'a Fact>,
    pub contains: BTreeMap<String, Vec<String>>,
    pub structural: BTreeSet<String>,
    pub program: Option<String>,
    pub modules: Vec<String>,
    pub owner: BTreeMap<String, String>,
    pub changed_by: BTreeMap<String, BTreeSet<String>>,
    pub adjs: BTreeMap<String, BTreeSet<String>>,
    pub with_state: BTreeSet<String>,
    pub invs: Vec<Inv>,
    pub rules: Vec<ViewRule>,
    pub deadline: BTreeMap<String, String>,
    pub period: BTreeMap<String, String>,
    pub gaps: BTreeMap<String, Vec<String>>,
    pub warnings: BTreeMap<String, Vec<String>>,
    pub absences: BTreeMap<String, Vec<String>>,
    pub notes: Vec<ViewNote>,
    pub views: BTreeSet<String>,
}

impl<'a> Model<'a> {
    pub fn new(a: &'a Analysis<'a>) -> Model<'a> {
        let views: BTreeSet<String> = a.entities.values().filter(|e| a.lex.is_a(&e.kind, VIEW)).map(|e| e.id.clone()).collect();
        let facts: Vec<&Fact> = a.facts.iter().filter(|f| !views.contains(f.subject())).collect();
        let mut contains: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let mut parents: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for f in &facts {
            if let (true, Some(o)) = (f.rel() == "a.contains", f.obj()) {
                contains.entry(f.subject().to_string()).or_default().push(o.to_string());
                parents.entry(o.to_string()).or_default().push(f.subject().to_string());
            }
        }
        let kind = |e: &str| a.entities.get(e).map(|x| x.kind.as_str()).unwrap_or("");
        let name = |e: &str| a.entities.get(e).map(|x| x.name.as_str()).unwrap_or("?");
        let mut structural: BTreeSet<String> =
            a.entities.values().filter(|e| a.lex.kind(&e.kind).is_some_and(|t| t.structural)).map(|e| e.id.clone()).collect();
        structural.retain(|e| kind(e) != "e.program");
        // the program: the one that contains the most components (ties by name)
        let mut progs: Vec<&str> = a.entities.values().filter(|e| e.kind == "e.program").map(|e| e.id.as_str()).collect();
        let children_struct = |p: &str| contains.get(p).map(|v| v.iter().filter(|x| structural.contains(*x)).count()).unwrap_or(0);
        progs.sort_by(|x, y| children_struct(y).cmp(&children_struct(x)).then(name(x).cmp(name(y))));
        let program = progs.first().map(|p| p.to_string());
        let mut modules: Vec<String> = match &program {
            Some(p) => contains.get(p).into_iter().flatten().filter(|e| structural.contains(*e)).cloned().collect(),
            None => {
                // no program: the loose components that have files or content
                let loose: Vec<String> = structural.iter().filter(|e| !parents.contains_key(*e)).cloned().collect();
                let with_items: Vec<String> = loose
                    .iter()
                    .filter(|e| contains.contains_key(*e) || facts.iter().any(|f| f.rel() == "a.implemented_in" && f.subject() == e.as_str()))
                    .cloned()
                    .collect();
                if with_items.is_empty() {
                    loose
                } else {
                    with_items
                }
            }
        };
        modules.sort_by(|x, y| name(x).cmp(name(y)).then(x.cmp(y)));
        modules.dedup();
        let mut owner: BTreeMap<String, String> = BTreeMap::new();
        for m in &modules {
            let mut stack = vec![m.clone()];
            let mut seen = BTreeSet::new();
            while let Some(x) = stack.pop() {
                if !seen.insert(x.clone()) {
                    continue;
                }
                for y in contains.get(&x).into_iter().flatten() {
                    owner.entry(y.clone()).or_insert_with(|| m.clone());
                    stack.push(y.clone());
                }
            }
            owner.insert(m.clone(), m.clone());
        }
        let mut changed_by: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        let mut adjs: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        let mut deadline = BTreeMap::new();
        let mut period = BTreeMap::new();
        let mut absences: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for f in &facts {
            match f {
                Fact::Prop { subject, adj } => {
                    adjs.entry(subject.clone()).or_default().insert(adj.clone());
                }
                Fact::Absent { subject, role } => absences.entry(subject.clone()).or_default().push(role.clone()),
                _ => {}
            }
            if matches!(f.rel(), "a.changes" | "a.moves") {
                if let Some(o) = f.obj() {
                    changed_by.entry(o.to_string()).or_default().insert(f.subject().to_string());
                }
            }
            if f.rel() == "a.times_out_after" {
                if let Some(l) = f.literal() {
                    deadline.insert(f.subject().to_string(), l.to_string());
                }
            }
            if f.rel() == "a.repeats_every" {
                if let Some(l) = f.literal() {
                    period.entry(f.subject().to_string()).or_insert_with(|| l.to_string());
                }
            }
        }
        for v in absences.values_mut() {
            v.sort();
            v.dedup();
        }
        let mut with_state: BTreeSet<String> =
            adjs.iter().filter(|(_, s)| s.contains("adj.mutable") || s.contains("adj.stateful")).map(|(e, _)| e.clone()).collect();
        with_state.extend(a.entities.values().filter(|e| e.kind == "p.state_machine").map(|e| e.id.clone()));
        with_state.extend(changed_by.keys().cloned());
        let invs = a
            .invariants
            .iter()
            .filter(|i| !a.derived.contains(&i.json().compact()))
            .map(|i| {
                let touches = i.when.iter().flatten().chain(i.then.iter()).flat_map(|f| f.mentions()).map(String::from).collect();
                Inv { text: canon::invariant_en(a, i).join(" "), touches }
            })
            .collect();
        let rules = a.rules.iter().filter(|r| !views.contains(&r.scope)).map(|r| ViewRule { text: canon::rule_en(a, r), scope: r.scope.clone() }).collect();
        let mut gaps: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for l in &a.gaps {
            gaps.entry(l.entity.clone()).or_default().push(l.role.clone());
        }
        let mut warnings: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for v in &a.warnings {
            warnings.entry(v.entity.clone()).or_default().push(v.text.clone());
        }
        // notes: the canonical English sentence of each fact, invariant and rule leads to the
        // entities it mentions
        let mut sentences: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for f in &a.facts {
            sentences.insert(canon::fact_en(a, f), f.mentions().into_iter().map(String::from).collect());
        }
        for i in &a.invariants {
            let ents: BTreeSet<String> = i.when.iter().flatten().chain(i.then.iter()).flat_map(|f| f.mentions()).map(String::from).collect();
            for en in canon::invariant_en(a, i) {
                sentences.insert(en, ents.clone());
            }
        }
        for r in &a.rules {
            let mut ents: BTreeSet<String> = r.then.mentions().into_iter().skip(1).map(String::from).collect();
            ents.insert(r.scope.clone());
            sentences.insert(canon::rule_en(a, r), ents);
        }
        let notes = a
            .notes
            .iter()
            .map(|n| match n.sentence.as_ref().and_then(|f| sentences.get(f)) {
                Some(ents) => ViewNote { text: n.text.clone(), sentence: n.sentence.clone(), ents: ents.clone() },
                None => ViewNote { text: n.text.clone(), sentence: None, ents: BTreeSet::new() },
            })
            .collect();
        Model {
            a,
            facts,
            contains,
            structural,
            program,
            modules,
            owner,
            changed_by,
            adjs,
            with_state,
            invs,
            rules,
            deadline,
            period,
            gaps,
            warnings,
            absences,
            notes,
            views,
        }
    }

    pub fn name(&self, e: &str) -> &str {
        self.a.name(e)
    }

    pub fn kind(&self, e: &str) -> &str {
        self.a.entities.get(e).map(|x| x.kind.as_str()).unwrap_or("")
    }

    /// Label of the type, its English form ("tcp endpoint").
    pub fn label_kind(&self, e: &str) -> &str {
        self.a.lex.kind(self.kind(e)).and_then(|t| t.en.first().map(String::as_str)).unwrap_or("?")
    }

    pub fn verb_pt(&self, rel: &str) -> &str {
        self.a.lex.verb(rel).and_then(|v| v.en.first().map(String::as_str)).unwrap_or("?")
    }

    /// `type “name”`, how entities are written outside the boxes.
    pub fn np(&self, e: &str) -> String {
        format!("{} “{}”", self.label_kind(e), self.name(e))
    }

    /// Sorts entities by name (ties by id).
    pub fn by_name(&self, v: &mut [String]) {
        v.sort_by(|x, y| self.name(x).cmp(self.name(y)).then(x.cmp(y)));
    }

    /// Component that contains `e`; or `e` itself, if it is a structural external.
    pub fn no(&self, e: &str) -> Option<String> {
        if let Some(d) = self.owner.get(e) {
            return Some(d.clone());
        }
        self.structural.contains(e).then(|| e.to_string())
    }

    /// Execution contexts `e` runs in.
    pub fn context(&self, e: &str) -> Vec<String> {
        let mut v: Vec<String> = self
            .facts
            .iter()
            .filter(|f| f.rel() == "a.runs_in" && f.subject() == e)
            .filter_map(|f| f.obj())
            .filter(|o| self.kind(o) == "p.execution_context")
            .map(|o| self.name(o).to_string())
            .collect();
        v.sort();
        v
    }

    /// Numbers (1..) of the invariants that touch some entity for which `f` holds.
    pub fn invs_of(&self, f: impl Fn(&str) -> bool) -> Vec<usize> {
        self.invs.iter().enumerate().filter(|(_, i)| i.touches.iter().any(|t| f(t))).map(|(k, _)| k + 1).collect()
    }

    /// Relations between components and externals: (origin, destination, verbs in Portuguese).
    /// Target = roles to/into/from/on and the object when it is itself structural (or when there
    /// is no to/into). State and primitive-role relations stay in the other views.
    pub fn relations(&self) -> Vec<(String, String, String)> {
        let mut edges: BTreeMap<(String, String), BTreeSet<&str>> = BTreeMap::new();
        for f in &self.facts {
            if !matches!(f, Fact::Rel { .. }) || IGNORE.contains(&f.rel()) || f.mode().is_some_and(|m| m != "exception") {
                continue;
            }
            let Some(o) = self.no(f.subject()) else { continue };
            let Fact::Rel { roles, .. } = f else { continue };
            let mut targets: Vec<&str> = roles.iter().filter(|(k, _)| matches!(k.as_str(), "to" | "into" | "from" | "on")).map(|(_, v)| v.as_str()).collect();
            if let Some(ob) = f.obj() {
                if self.structural.contains(ob) || !(roles.contains_key("to") || roles.contains_key("into")) {
                    targets.push(ob);
                }
            }
            for target in targets {
                if let Some(d) = self.no(target) {
                    if o != d {
                        edges.entry((o.clone(), d)).or_default().insert(self.verb_pt(f.rel()));
                    }
                }
            }
        }
        edges.into_iter().map(|((o, d), r)| (o, d, r.into_iter().collect::<Vec<_>>().join(", "))).collect()
    }

    /// Depth of each layer in `is above` order (those at the top have 0).
    pub fn prof_layers(&self) -> BTreeMap<String, usize> {
        let mut above: Vec<(&str, &str)> = Vec::new();
        for f in &self.facts {
            if let (true, Some(o)) = (f.rel() == "a.above" && f.mode().is_none(), f.obj()) {
                above.push((f.subject(), o));
            }
        }
        let mut prof: BTreeMap<&str, usize> = BTreeMap::new();
        for &(x, y) in &above {
            prof.entry(x).or_insert(0);
            prof.entry(y).or_insert(0);
        }
        for _ in 0..=prof.len() {
            for &(x, y) in &above {
                let px = prof[x];
                if prof[y] < px + 1 && px < prof.len() {
                    prof.insert(y, px + 1);
                }
            }
        }
        prof.into_iter().map(|(k, v)| (k.to_string(), v)).collect()
    }

    /// Rows of the structure panel. With declared layers, one row per layer, from top to bottom,
    /// and the components with no layer at the end; otherwise, ordering by the longest path
    /// after removing the back edges.
    pub fn lines(&self, rels: &[(String, String, String)]) -> Lines {
        let edges: Vec<(String, String)> = rels.iter().map(|(o, d, _)| (o.clone(), d.clone())).collect();
        let with_layer: Vec<&String> = self.modules.iter().filter(|m| self.a.layers.contains_key(*m)).collect();
        if with_layer.is_empty() {
            let (level, back) = layers(&self.modules, &edges, |n| self.name(n).to_string());
            let mut grouped: BTreeMap<usize, Vec<String>> = BTreeMap::new();
            for n in &self.modules {
                grouped.entry(level[n]).or_default().push(n.clone());
            }
            return Lines { declared: false, lines: grouped.into_values().map(|v| (None, v)).collect(), back };
        }
        let prof = self.prof_layers();
        let layers_used: BTreeSet<&str> = with_layer.iter().map(|m| self.a.layers[*m].as_str()).collect();
        let mut order: Vec<&str> = layers_used.iter().copied().collect();
        order.sort_by(|x, y| prof.get(*x).unwrap_or(&0).cmp(prof.get(*y).unwrap_or(&0)).then(self.name(x).cmp(self.name(y))));
        let mut lines: Vec<(Option<String>, Vec<String>)> = Vec::new();
        let mut level = BTreeMap::new();
        for c in &order {
            let v: Vec<String> = self.modules.iter().filter(|m| self.a.layers.get(*m).map(String::as_str) == Some(*c)).cloned().collect();
            for m in &v {
                level.insert(m.clone(), lines.len());
            }
            lines.push((Some(format!("layer “{}”", self.name(c))), v));
        }
        let without_layer: Vec<String> = self.modules.iter().filter(|m| !self.a.layers.contains_key(*m)).cloned().collect();
        if !without_layer.is_empty() {
            for m in &without_layer {
                level.insert(m.clone(), lines.len());
            }
            lines.push((Some("no layer".into()), without_layer));
        }
        // only a relation with a declared layer on both sides goes up a layer: "no layer" does not count
        let back = edges
            .iter()
            .filter(|(o, d)| self.a.layers.contains_key(o) && self.a.layers.contains_key(d) && level.get(o).zip(level.get(d)).is_some_and(|(a, b)| a > b))
            .cloned()
            .collect();
        Lines { declared: true, lines, back }
    }

    /// Primitives inside the component, with marks: ● state, ? gap, ∅ declared absence,
    /// ⚠ warning, ⏱ deadline, ⟳ period.
    pub fn lines_of_module(&self, n: &str) -> Vec<Item> {
        let mut items = Vec::new();
        let mut children: Vec<String> = Vec::new();
        let mut stack: Vec<String> = self.contains.get(n).cloned().unwrap_or_default();
        let mut seen = BTreeSet::new();
        while let Some(e) = stack.pop() {
            if !seen.insert(e.clone()) {
                continue;
            }
            stack.extend(self.contains.get(&e).cloned().unwrap_or_default());
            children.push(e);
        }
        let ctx = self.context(n);
        if !ctx.is_empty() {
            items.push(Item { text: format!("runs in {}", ctx.join(", ")), color: svg::MUTED, sub: false });
        }
        let mut commands: Vec<String> = children.iter().filter(|e| self.kind(e) == "e.command").cloned().collect();
        self.by_name(&mut commands);
        if !commands.is_empty() {
            let ns: Vec<&str> = commands.iter().map(|c| self.name(c)).collect();
            items.push(Item { text: format!("commands {}", ns.join(", ")), color: svg::TEXT, sub: false });
        }
        let mut fields: Vec<String> = children.iter().filter(|e| self.kind(e) == "e.field").cloned().collect();
        self.by_name(&mut fields);
        fields.dedup();
        let mut others: Vec<String> = children
            .iter()
            .filter(|e| !matches!(self.kind(e), "e.field" | "p.state" | "e.value" | "e.command") && !self.structural.contains(*e))
            .cloned()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        others.sort_by(|x, y| (self.kind(x), self.name(x)).cmp(&(self.kind(y), self.name(y))));
        for e in &others {
            let kind = self.label_kind(e).split(' ').next().unwrap_or("");
            let mut mark = String::new();
            if self.with_state.contains(e) {
                mark.push_str(" ●");
            } else if self.adjs.get(e).is_some_and(|s| s.contains("adj.immutable")) {
                mark.push_str(" (immutable)");
            }
            if let Some(p) = self.deadline.get(e) {
                mark.push_str(&format!(" ⏱{p}"));
            }
            if let Some(p) = self.period.get(e) {
                mark.push_str(&format!(" ⟳{p}"));
            }
            if self.gaps.contains_key(e) {
                mark.push_str(" ?");
            }
            if self.absences.contains_key(e) {
                mark.push_str(" ∅");
            }
            let color = if self.with_state.contains(e) {
                svg::STATE
            } else if self.gaps.contains_key(e) {
                svg::GAP
            } else {
                svg::TEXT
            };
            items.push(Item { text: format!("{} · {kind}{mark}", self.name(e)), color, sub: false });
            let mut changers: Vec<&str> = self.changed_by.get(e).into_iter().flatten().filter(|x| *x != e).map(|x| self.name(x)).collect();
            changers.sort();
            if !changers.is_empty() {
                items.push(Item { text: format!("   ← changed by {}", changers.join(", ")), color: svg::STATE, sub: true });
            }
        }
        if !fields.is_empty() {
            let (mut marked, mut absent) = (Vec::new(), Vec::new());
            for c in &fields {
                if self.warnings.contains_key(c) {
                    marked.push(format!("{} ⚠", self.name(c)));
                } else if self.gaps.contains_key(c) {
                    marked.push(format!("{} ?", self.name(c)));
                }
                if self.absences.contains_key(c) {
                    absent.push(format!("{} ∅", self.name(c)));
                }
            }
            items.push(Item { text: format!("{} fields", fields.len()), color: svg::TEXT, sub: false });
            if !marked.is_empty() {
                items.push(Item { text: format!("   {}", marked.join(", ")), color: svg::WARNING, sub: true });
            }
            if !absent.is_empty() {
                items.push(Item { text: format!("   {}", absent.join(", ")), color: svg::ABSENT, sub: true });
            }
        }
        items
    }

    /// Limits and equalities of the component's fields: `floors ≥2 ≤36`, by field name.
    pub fn limits_of_fields(&self, n: &str) -> Vec<String> {
        let mut stack: Vec<String> = self.contains.get(n).cloned().unwrap_or_default();
        let mut seen = BTreeSet::new();
        let mut fields: Vec<String> = Vec::new();
        while let Some(e) = stack.pop() {
            if !seen.insert(e.clone()) {
                continue;
            }
            stack.extend(self.contains.get(&e).cloned().unwrap_or_default());
            if self.kind(&e) == "e.field" {
                fields.push(e);
            }
        }
        self.by_name(&mut fields);
        let mut out = Vec::new();
        for c in fields {
            let mut ops: Vec<String> = Vec::new();
            for f in &self.facts {
                if let Fact::Cmp { subject, op, value } = f {
                    if *subject == c {
                        let v = match value {
                            Value::Num(x) => number(*x),
                            Value::Ent(o) => self.name(o).to_string(),
                        };
                        let s = match op.as_str() {
                            ">=" => "≥",
                            "<=" => "≤",
                            "==" => "=",
                            "!=" => "≠",
                            o => o,
                        };
                        ops.push(format!("{s}{v}"));
                    }
                }
            }
            ops.sort();
            ops.dedup();
            if !ops.is_empty() {
                out.push(format!("{} {}", self.name(&c), ops.join(" ")));
            }
        }
        out
    }

    /// Externals linked to the components (what appears in the bottom band).
    pub fn external(&self, rels: &[(String, String, String)]) -> Vec<String> {
        let mods: BTreeSet<&String> = self.modules.iter().collect();
        let mut v: Vec<String> = rels.iter().flat_map(|(o, d, _)| [o, d]).filter(|e| !mods.contains(e)).cloned().collect::<BTreeSet<_>>().into_iter().collect();
        self.by_name(&mut v);
        v
    }

    /// State machines, by name.
    pub fn machines(&self) -> Vec<Machine> {
        let mut ms: Vec<String> = self.a.entities.values().filter(|e| e.kind == "p.state_machine").map(|e| e.id.clone()).collect();
        self.by_name(&mut ms);
        ms.into_iter().map(|m| self.machine(&m)).collect()
    }

    pub fn machine(&self, m: &str) -> Machine {
        let mut states: Vec<String> = self.facts.iter().filter(|f| f.rel() == "a.has_states" && f.subject() == m).filter_map(|f| f.obj()).map(String::from).collect();
        states.sort();
        states.dedup();
        let mut transitions: Vec<(String, String, String)> = self
            .facts
            .iter()
            .filter(|f| f.rel() == "a.moves" && f.obj() == Some(m))
            .filter_map(|f| Some((f.role("from")?.to_string(), f.role("to")?.to_string(), f.subject().to_string())))
            .collect();
        transitions.sort();
        transitions.dedup();
        let initial = self.facts.iter().filter(|f| f.rel() == "a.starts_in" && f.subject() == m).filter_map(|f| f.obj()).map(String::from).min();
        Machine { id: m.to_string(), states, initial, transitions }
    }

    /// Threads, by name.
    pub fn threads(&self) -> Vec<String> {
        let mut v: Vec<String> = self.a.entities.values().filter(|e| e.kind == "p.thread").map(|e| e.id.clone()).collect();
        self.by_name(&mut v);
        v
    }

    /// Who touches each primitive: target -> thread -> verbs in Portuguese.
    pub fn touch_map(&self, threads: &[String]) -> BTreeMap<String, BTreeMap<String, BTreeSet<String>>> {
        let mut t: BTreeMap<String, BTreeMap<String, BTreeSet<String>>> = BTreeMap::new();
        for f in &self.facts {
            let Some(o) = f.obj() else { continue };
            if threads.iter().any(|x| x == f.subject()) && f.rel() != "a.contains" {
                t.entry(o.to_string()).or_default().entry(f.subject().to_string()).or_default().insert(self.verb_pt(f.rel()).to_string());
            }
            if let Fact::Rel { roles, .. } = f {
                for v in roles.values() {
                    if threads.contains(v) {
                        t.entry(o.to_string()).or_default().entry(v.clone()).or_default().insert("runs here".into());
                    }
                }
            }
        }
        t
    }

    /// Rules of a thread that appear in the header: "never <verb>" (intransitive negation).
    pub fn rules_of_thread(&self, t: &str) -> Vec<String> {
        let mut v = Vec::new();
        for i in &self.a.invariants {
            for f in &i.then {
                if i.when.is_none() && f.subject() == t && f.mode() == Some("not") && f.obj().is_none() {
                    v.push(format!("never {}", self.verb_pt(f.rel())));
                }
            }
        }
        v
    }

    /// Locks that protect `e`.
    pub fn protected(&self, e: &str) -> Vec<String> {
        self.facts.iter().filter(|f| f.rel() == "a.protected_by" && f.subject() == e).filter_map(|f| f.obj()).map(|o| self.name(o).to_string()).collect()
    }

    /// Mutable state: primitive -> (who writes, who reads), by name.
    pub fn spelling_read(&self) -> (BTreeMap<String, BTreeSet<String>>, BTreeMap<String, BTreeSet<String>>) {
        let (mut emit, mut reads): (BTreeMap<String, BTreeSet<String>>, BTreeMap<String, BTreeSet<String>>) = Default::default();
        for f in &self.facts {
            let Some(o) = f.obj() else { continue };
            if f.mode().is_some() {
                continue;
            }
            let who = self.name(f.subject()).to_string();
            if matches!(f.rel(), "a.fills" | "a.changes" | "a.overwrites" | "a.moves" | "a.publishes" | "a.pushes" | "a.pops" | "a.adds" | "a.removes") {
                emit.entry(o.to_string()).or_default().insert(who.clone());
            }
            if matches!(f.rel(), "a.consumes" | "a.reads" | "a.uses") {
                reads.entry(o.to_string()).or_default().insert(who);
            }
        }
        (emit, reads)
    }

    /// Cards of the primitives that have presupposed roles in the lexicon (fewer fields and machines,
    /// which have their own view): entity and (role, value).
    pub fn cards(&self) -> Vec<(String, Vec<(String, Role)>)> {
        let lex = self.a.lex;
        let mut ents: Vec<&String> = self.a.entities.keys().filter(|e| !self.views.contains(*e)).collect();
        ents.sort_by(|x, y| (self.kind(x), self.name(x)).cmp(&(self.kind(y), self.name(y))));
        let mut out = Vec::new();
        for e in ents {
            let reqs: Vec<_> = lex.ancestors(self.kind(e)).iter().filter_map(|t| lex.kind(t)).flat_map(|t| t.requires.iter()).collect();
            if reqs.is_empty() || matches!(self.kind(e), "e.field" | "p.state_machine") {
                continue;
            }
            let mut lines = Vec::new();
            for r in reqs {
                let inv = r.extra.as_deref() == Some("inverse");
                let mut vals: BTreeSet<String> = BTreeSet::new();
                for f in &self.facts {
                    if f.rel() != r.verb {
                        continue;
                    }
                    if inv {
                        if f.obj() == Some(e.as_str()) {
                            vals.insert(self.name(f.subject()).to_string());
                        }
                        continue;
                    }
                    if f.subject() != e {
                        continue;
                    }
                    match f {
                        Fact::Cmp { op, value, .. } => {
                            let v = match value {
                                Value::Num(x) => number(*x),
                                Value::Ent(o) => self.name(o).to_string(),
                            };
                            vals.insert(format!("{} {v}", ops_pt(op)));
                        }
                        Fact::Prop { adj, .. } => {
                            if r.extra.as_deref().unwrap_or("").split(',').any(|x| x == adj) {
                                if let Some(a) = lex.adjective(adj) {
                                    vals.insert(a.en[0].clone());
                                }
                            }
                        }
                        _ => {
                            let v = match (f.literal(), f.obj()) {
                                (Some(l), _) => l.to_string(),
                                (None, Some(o)) => self.name(o).to_string(),
                                _ => match f {
                                    Fact::Rel { roles, .. } => roles.values().map(|v| self.name(v)).collect::<Vec<_>>().join(", "),
                                    _ => String::new(),
                                },
                            };
                            vals.insert(v);
                        }
                    }
                }
                let role = if !vals.is_empty() {
                    Role::Value(vals.into_iter().collect::<Vec<_>>().join(", "))
                } else if self.absences.get(e).is_some_and(|v| v.contains(&r.role)) {
                    Role::Absent
                } else {
                    Role::Gap
                };
                if !lines.iter().any(|(p, _): &(String, Role)| *p == r.role) {
                    lines.push((r.role.clone(), role));
                }
            }
            out.push((e.clone(), lines));
        }
        out
    }

    /// Notes whose sentences cite some entity for which `f` holds (those of the file, when
    /// `of_file`).
    pub fn notes_of(&self, of_file: bool, f: impl Fn(&str) -> bool) -> Vec<&ViewNote> {
        self.notes
            .iter()
            .filter(|n| !n.ents.iter().any(|e| self.views.contains(e)))
            .filter(|n| if n.sentence.is_none() { of_file } else { n.ents.iter().any(|e| f(e)) })
            .collect()
    }
}

pub fn ops_pt(op: &str) -> &'static str {
    match op {
        ">=" => "at least",
        ">" => "greater than",
        "<=" => "at most",
        "<" => "less than",
        "==" => "equal to",
        "!=" => "different from",
        _ => "?",
    }
}

/// Layers by longest path, after removing the back edges (DFS in fixed order:
/// first whoever receives no edge, then the largest edge balance, then the name).
pub fn layers(nodes: &[String], edges: &[(String, String)], name: impl Fn(&str) -> String) -> (BTreeMap<String, usize>, BTreeSet<(String, String)>) {
    let scope: BTreeSet<&String> = nodes.iter().collect();
    let mut adj: BTreeMap<&String, Vec<&String>> = BTreeMap::new();
    for n in nodes {
        let ds: BTreeSet<&String> = edges.iter().filter(|(o, d)| o == n && scope.contains(d)).map(|(_, d)| d).collect();
        adj.insert(n, ds.into_iter().collect());
    }
    let enters: BTreeSet<&String> = edges.iter().filter(|(o, d)| scope.contains(o) && scope.contains(d)).map(|(_, d)| d).collect();
    let balance = |n: &String| -> i64 { edges.iter().filter(|(o, _)| o == n).count() as i64 - edges.iter().filter(|(_, d)| d == n).count() as i64 };
    let mut order: Vec<&String> = nodes.iter().collect();
    order.sort_by(|x, y| (enters.contains(x), -balance(x), name(x)).cmp(&(enters.contains(y), -balance(y), name(y))));
    let mut state: BTreeMap<&String, u8> = BTreeMap::new();
    let mut back: BTreeSet<(String, String)> = BTreeSet::new();
    fn dfs<'s>(n: &'s String, adj: &BTreeMap<&'s String, Vec<&'s String>>, state: &mut BTreeMap<&'s String, u8>, back: &mut BTreeSet<(String, String)>) {
        state.insert(n, 1);
        for d in &adj[n] {
            match state.get(d) {
                Some(1) => {
                    back.insert((n.clone(), (*d).clone()));
                }
                None => dfs(d, adj, state, back),
                _ => {}
            }
        }
        state.insert(n, 2);
    }
    for n in order {
        if !state.contains_key(n) {
            dfs(n, &adj, &mut state, &mut back);
        }
    }
    let mut level: BTreeMap<String, usize> = nodes.iter().map(|n| (n.clone(), 0)).collect();
    let dag: Vec<&(String, String)> = edges.iter().filter(|(o, d)| scope.contains(o) && scope.contains(d) && !back.contains(&(o.clone(), d.clone()))).collect();
    for _ in nodes {
        for (o, d) in &dag {
            let v = level[o] + 1;
            if level[d] < v {
                level.insert(d.clone(), v);
            }
        }
    }
    (level, back)
}
