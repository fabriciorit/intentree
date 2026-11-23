//! Architecture layers and the verifiable rule they bring.
//!
//! `The module "cli" is in the layer "interface".` puts the entity in a layer (one without a
//! layer inherits that of the nearest container, through the `contains` facts). `The layer
//! "interface" is above the layer "domain".` orders the layers; the order is the transitive
//! closure, and a cycle is an error. A relation (any verb except contains, membership and layer
//! order, and not the negated ones) that goes from a layer to one above it is an error, unless
//! the sentence itself declares the exception with `exceptionally`:
//! `The module "config" exceptionally uses the module "cli".`

use super::analysis::{Analysis, Warning, Fact};
use std::collections::{BTreeMap, BTreeSet};

const LAYER: &str = "e.layer";

/// Result: effective layer per entity (id -> layer id), errors and warnings.
#[derive(Default)]
pub struct Layers {
    pub from: BTreeMap<String, String>,
    pub errors: Vec<String>,
    pub warnings: Vec<Warning>,
}

/// Label of the sentence that produced the fact (so the error can cite the line), if any.
fn label_of(a: &Analysis, f: &Fact) -> Option<String> {
    a.origins.iter().find(|o| o.facts.contains(f) || o.invariants.iter().any(|i| i.then.contains(f))).map(|o| o.label.clone())
}

pub fn check(a: &Analysis) -> Layers {
    let mut r = Layers::default();
    let and_layer = |e: &str| a.entities.get(e).is_some_and(|x| a.lex.is_a(&x.kind, LAYER));
    // order: transitive closure of 'is above'
    let mut above: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for f in &a.facts {
        if f.rel() == "a.above" && f.mode().is_none() {
            if let Some(o) = f.obj() {
                above.entry(f.subject()).or_default().insert(o);
            }
        }
    }
    let mut closing: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for &c in above.keys() {
        let mut seen: BTreeSet<&str> = BTreeSet::new();
        let mut stack: Vec<&str> = above[c].iter().copied().collect();
        while let Some(x) = stack.pop() {
            if seen.insert(x) {
                stack.extend(above.get(x).into_iter().flatten().copied());
            }
        }
        closing.insert(c, seen);
    }
    let mut in_cycle: Vec<&str> = closing.iter().filter(|(c, s)| s.contains(*c)).map(|(c, _)| *c).collect();
    in_cycle.sort_by_key(|c| a.name(c));
    if !in_cycle.is_empty() {
        let names: Vec<String> = in_cycle.iter().map(|c| format!("'{}'", a.name(c))).collect();
        r.errors.push(format!("cycle in the layer order: {}", names.join(", ")));
    }
    let rises = |from: &str, to: &str| closing.get(to).is_some_and(|s| s.contains(from));
    // declared layer of each entity
    let mut own: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for f in &a.facts {
        if f.rel() == "a.is_in" && f.mode().is_none() {
            if let Some(o) = f.obj().filter(|o| and_layer(o)) {
                own.entry(f.subject()).or_default().insert(o);
            }
        }
    }
    for (e, cs) in &own {
        if and_layer(e) {
            r.errors.push(format!("the layer '{}' cannot be in another layer", a.name(e)));
        }
        if cs.len() > 1 {
            let mut names: Vec<&str> = cs.iter().map(|c| a.name(c)).collect();
            names.sort();
            r.errors.push(format!("'{}' is in more than one layer: {}", a.name(e), names.join(", ")));
        }
    }
    // inheritance from the nearest container (breadth-first; ties by name)
    let mut parents: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for f in &a.facts {
        if f.rel() == "a.contains" && f.mode().is_none() {
            if let Some(o) = f.obj() {
                parents.entry(o).or_default().push(f.subject());
            }
        }
    }
    for v in parents.values_mut() {
        v.sort_by_key(|p| (a.name(p), *p));
    }
    for e in a.entities.keys() {
        let mut queue: Vec<&str> = vec![e];
        let mut seen: BTreeSet<&str> = BTreeSet::new();
        let mut i = 0;
        while i < queue.len() {
            let x = queue[i];
            i += 1;
            if !seen.insert(x) {
                continue;
            }
            if let Some(c) = own.get(x).and_then(|cs| cs.iter().next()) {
                r.from.insert(e.clone(), c.to_string());
                break;
            }
            queue.extend(parents.get(x).into_iter().flatten().copied());
        }
    }
    // relations that go up a layer
    let mut rels: Vec<&Fact> = a.facts.iter().collect();
    rels.extend(a.invariants.iter().flat_map(|i| i.then.iter()));
    let mut errors = BTreeSet::new();
    for f in rels {
        if matches!(f.rel(), "a.contains" | "a.is_in" | "a.above") || f.mode() == Some("not") || !matches!(f, Fact::Rel { .. }) {
            continue;
        }
        let from = f.subject();
        let exception = f.mode() == Some("exception");
        let mut rose = false;
        for to in f.mentions().into_iter().skip(1) {
            let (Some(cd), Some(cp)) = (r.from.get(from), r.from.get(to)) else { continue };
            if !rises(cd, cp) {
                continue;
            }
            rose = true;
            if !exception {
                let locate = label_of(a, f).map(|x| format!("{x}: ")).unwrap_or_default();
                errors.insert(format!(
                    "{locate}relation goes up a layer: '{}' (layer '{}') {} '{}' (layer '{}'); if it is on purpose, write 'exceptionally' before the verb",
                    a.name(from),
                    a.name(cd),
                    f.rel(),
                    a.name(to),
                    a.name(cp)
                ));
            }
        }
        if exception && !rose {
            r.warnings.push(Warning { entity: from.to_string(), text: format!("layer exception without a relation that goes up: {} '{}'", f.rel(), a.name(f.obj().unwrap_or(""))) });
        }
    }
    r.errors.extend(errors);
    r
}
