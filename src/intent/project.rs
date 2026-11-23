//! Anchoring of the `.intent` in the code: which files each component implements, what it
//! contains, what relates it to the rest, and the checks against the project's files.
//!
//! A component is any entity that some sentence says `is implemented in "glob"`. The hierarchy
//! comes from the `contains` facts (transitive, between any entities): a file matched by
//! two components goes to the more contained one; with no containment between them it is a warning.

use super::analysis::{Analysis, Fact, Invariant};
use super::canon;
use crate::symbol::{check_anchor, hint_glob};
use crate::glob::Glob;
use crate::symbol::part_anchor;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt::Write;
use crate::vfs::Fs;
use std::path::Path;

pub struct GlobDe {
    pub text: String,
    pub glob: Glob,
    pub label: String,
}

/// File diagnostic: `label` is the one of the originating sentence (empty when there is not a single one).
pub struct Diag {
    pub error: bool,
    pub label: String,
    pub msg: String,
}

impl Diag {
    pub fn line(&self) -> String {
        let level = if self.error { "error" } else { "warning" };
        if self.label.is_empty() {
            format!("{level} {}", self.msg)
        } else {
            format!("{level} {}: {}", self.label, self.msg)
        }
    }
}

/// Who owns each file of the project.
pub struct Coverage {
    pub files: Vec<String>,
    pub owner: Vec<Option<usize>>,
    /// tied (no containment between them) per file; only filled when there is more than one
    pub ties: Vec<Vec<usize>>,
    pub excl: Vec<Option<usize>>,
}

impl Coverage {
    pub fn covered(&self) -> usize {
        (0..self.files.len()).filter(|&i| self.owner[i].is_some() || self.excl[i].is_some()).count()
    }

    /// Whole percentage, rounded; with no files it counts as 100.
    pub fn pct(&self) -> usize {
        let n = self.files.len();
        if n == 0 {
            100
        } else {
            (self.covered() * 100 + n / 2) / n
        }
    }
}

pub struct Project<'a> {
    pub a: &'a Analysis<'a>,
    /// entity id by index (the order of the analysis' `BTreeMap`)
    pub ids: Vec<String>,
    idx: HashMap<String, usize>,
    pub globs: Vec<Vec<GlobDe>>,
    /// (index of the excluder, glob)
    pub excls: Vec<(usize, GlobDe)>,
    pub children: Vec<Vec<usize>>,
    /// ancestors by containment, from nearest to farthest
    pub anchor: Vec<Vec<usize>>,
    edges: Vec<(usize, usize)>,
}

fn mentions<'f>(f: &'f Fact, out: &mut Vec<&'f str>) {
    out.extend(f.mentions());
}

impl<'a> Project<'a> {
    pub fn new(a: &'a Analysis<'a>) -> Project<'a> {
        let ids: Vec<String> = a.entities.keys().cloned().collect();
        let idx: HashMap<String, usize> = ids.iter().enumerate().map(|(i, e)| (e.clone(), i)).collect();
        let n = ids.len();
        let mut globs: Vec<Vec<GlobDe>> = (0..n).map(|_| Vec::new()).collect();
        let mut excls: Vec<(usize, GlobDe)> = Vec::new();
        for o in &a.origins {
            for f in &o.facts {
                let (Some(l), Some(&c)) = (f.literal(), idx.get(f.subject())) else { continue };
                let g = || GlobDe { text: l.to_string(), glob: Glob::new(l), label: o.label.clone() };
                match f.rel() {
                    "a.implemented_in" if !globs[c].iter().any(|x| x.text == l) => globs[c].push(g()),
                    "a.excludes" if !excls.iter().any(|(q, x)| *q == c && x.text == l) => excls.push((c, g())),
                    _ => {}
                }
            }
        }
        let mut parents: Vec<Vec<usize>> = vec![Vec::new(); n];
        let mut children: Vec<Vec<usize>> = vec![Vec::new(); n];
        for f in &a.facts {
            if f.rel() != "a.contains" {
                continue;
            }
            if let (Some(&p), Some(&c)) = (idx.get(f.subject()), f.obj().and_then(|o| idx.get(o))) {
                if p != c && !parents[c].contains(&p) {
                    parents[c].push(p);
                    children[p].push(c);
                }
            }
        }
        // stable order: by name (the order of the ids is the hash's)
        let by_name = |v: &mut Vec<usize>| v.sort_by(|&x, &y| (a.name(&ids[x]), &ids[x]).cmp(&(a.name(&ids[y]), &ids[y])));
        parents.iter_mut().for_each(by_name);
        children.iter_mut().for_each(by_name);
        // ancestors breadth-first: from nearest to farthest, without repeating or coming back to itself
        let anchor: Vec<Vec<usize>> = (0..n)
            .map(|c| {
                let mut v: Vec<usize> = Vec::new();
                let mut queue: Vec<usize> = parents[c].clone();
                let mut i = 0;
                while i < queue.len() {
                    let x = queue[i];
                    i += 1;
                    if x == c || v.contains(&x) {
                        continue;
                    }
                    v.push(x);
                    queue.extend(parents[x].iter().copied());
                }
                v
            })
            .collect();
        // relations: any verb except `contains`, without the negated ones; the role (from, to, ...)
        // also links
        let mut set: BTreeSet<(usize, usize)> = BTreeSet::new();
        let mut is_linked = |f: &Fact| {
            if f.rel() == "a.contains" || f.mode() == Some("not") {
                return;
            }
            let Fact::Rel { subject, .. } = f else { return };
            let Some(&from) = idx.get(subject) else { return };
            let mut targets: Vec<&str> = Vec::new();
            mentions(f, &mut targets);
            for al in targets.into_iter().skip(1) {
                if let Some(&to) = idx.get(al) {
                    if to != from {
                        set.insert((from, to));
                    }
                }
            }
        };
        for f in &a.facts {
            is_linked(f);
        }
        for i in &a.invariants {
            i.then.iter().for_each(&mut is_linked);
        }
        Project { a, ids, idx, globs, excls, children, anchor, edges: set.into_iter().collect() }
    }

    /// There is something to check against the files: glob, exclusion or anchor.
    pub fn anchored(&self) -> bool {
        self.globs.iter().any(|g| !g.is_empty()) || !self.excls.is_empty() || self.a.origins.iter().any(|o| !o.anchors.is_empty())
    }

    /// There are components or exclusions (file coverage only applies then).
    pub fn has_files(&self) -> bool {
        self.globs.iter().any(|g| !g.is_empty()) || !self.excls.is_empty()
    }

    pub fn index(&self, eid: &str) -> Option<usize> {
        self.idx.get(eid).copied()
    }

    pub fn name(&self, c: usize) -> &str {
        self.a.name(&self.ids[c])
    }

    pub fn kind_id(&self, c: usize) -> &str {
        self.a.entities.get(&self.ids[c]).map(|e| e.kind.as_str()).unwrap_or("")
    }

    /// English word of the kind (`module`).
    pub fn kind_en(&self, c: usize) -> &str {
        self.a.lex.kind(self.kind_id(c)).and_then(|t| t.en.first()).map(String::as_str).unwrap_or("?")
    }

    /// First component with that name (prefers those that have files).
    pub fn by_name(&self, name: &str) -> Option<usize> {
        let all = (0..self.ids.len()).filter(|&c| self.name(c) == name);
        all.clone().find(|&c| !self.globs[c].is_empty()).or_else(|| all.clone().next())
    }

    pub fn and_component(&self, c: usize) -> bool {
        !self.globs[c].is_empty()
    }

    pub fn contained_in(&self, c: usize, a: usize) -> bool {
        c == a || self.anchor[c].contains(&a)
    }

    /// Strictly inside `a`.
    fn descends_of(&self, c: usize, a: usize) -> bool {
        c != a && self.anchor[c].contains(&a)
    }

    /// All those contained in `c`, directly or indirectly.
    pub fn descendants(&self, c: usize) -> Vec<usize> {
        (0..self.ids.len()).filter(|&x| self.descends_of(x, c)).collect()
    }

    /// Smallest common ancestor-or-self of all; `None` if there is none.
    fn smaller_common(&self, cs: &[usize]) -> Option<usize> {
        let (&p, rest) = cs.split_first()?;
        std::iter::once(p).chain(self.anchor[p].iter().copied()).find(|&a| rest.iter().all(|&c| self.contained_in(c, a)))
    }

    /// Component (with files) that represents the entity: itself or the nearest container.
    pub fn component_of(&self, c: usize) -> Option<usize> {
        std::iter::once(c).chain(self.anchor[c].iter().copied()).find(|&x| self.and_component(x))
    }

    /// Owner of the file and the tied ones: among the components whose glob matches, those that contain
    /// another of the matched ones drop out (the more contained one wins); the ones left, with no containment,
    /// tie and the first by name gets the file.
    pub fn owner_and_tie(&self, file: &str) -> (Option<usize>, Vec<usize>) {
        let matched: Vec<usize> =
            (0..self.ids.len()).filter(|&c| self.globs[c].iter().any(|g| g.glob.matches(file))).collect();
        let mut stays: Vec<usize> = matched.iter().copied().filter(|&c| !matched.iter().any(|&d| self.descends_of(d, c))).collect();
        if stays.is_empty() {
            stays = matched;
        }
        stays.sort_by(|&x, &y| (self.name(x), &self.ids[x]).cmp(&(self.name(y), &self.ids[y])));
        (stays.first().copied(), stays)
    }

    pub fn excluded_by(&self, file: &str) -> Option<usize> {
        self.excls.iter().position(|(_, g)| g.glob.matches(file))
    }

    pub fn coverage(&self, files: Vec<String>) -> Coverage {
        let mut owner = Vec::new();
        let mut ties = Vec::new();
        let mut excl = Vec::new();
        for f in &files {
            let (o, t) = self.owner_and_tie(f);
            owner.push(o);
            ties.push(if t.len() > 1 { t } else { Vec::new() });
            excl.push(self.excluded_by(f));
        }
        Coverage { files, owner, ties, excl }
    }

    // ---------------------------------------------------------------- check

    /// File errors and warnings: globs and exclusions without a file, anchors, ties, files
    /// without a component (with the percentage), anchor outside the component.
    pub fn diagnostics(&self, cov: &Coverage, fs: &dyn Fs, root: &Path) -> Vec<Diag> {
        let mut d: Vec<Diag> = Vec::new();
        let error = |label: &str, msg: String| Diag { error: true, label: label.to_string(), msg };
        let warning = |label: &str, msg: String| Diag { error: false, label: label.to_string(), msg };
        let matched = |g: &GlobDe| cov.files.iter().any(|f| g.glob.matches(f));
        for c in 0..self.ids.len() {
            for g in &self.globs[c] {
                if !matched(g) {
                    d.push(error(&g.label, format!("the glob of \"{}\" matches no file: {}{}", self.name(c), g.text, hint_glob(&g.text, fs, root))));
                }
            }
        }
        for (c, g) in &self.excls {
            if !matched(g) {
                d.push(error(&g.label, format!("the exclusion of \"{}\" matches no file: {}{}", self.name(*c), g.text, hint_glob(&g.text, fs, root))));
            }
        }
        // anchors: each one must match a file and, with a symbol, define the symbol
        for o in &self.a.origins {
            let Some(subject) = o.facts.first().map(Fact::subject).or_else(|| o.invariants.first().and_then(|i| i.then.first()).map(Fact::subject)) else {
                continue;
            };
            let mut owners: Vec<usize> = Vec::new();
            let mut inside = false;
            let mut outside: Option<(usize, usize)> = None;
            let target = self.index(subject).and_then(|c| self.component_of(c));
            for anchor in &o.anchors {
                let (matched, msgs) = check_anchor(anchor, &cov.files, fs, root);
                for (e, msg) in msgs {
                    d.push(if e { error(&o.label, msg) } else { warning(&o.label, msg) });
                }
                for i in matched {
                    let Some(owner) = cov.owner[i] else { continue };
                    if !owners.contains(&owner) {
                        owners.push(owner);
                    }
                    if target.is_some_and(|a| self.contained_in(owner, a)) {
                        inside = true;
                    } else if outside.is_none() {
                        outside = Some((i, owner));
                    }
                }
            }
            if let (Some(_), false, Some((i, owner))) = (target, inside, outside) {
                let locate = match self.smaller_common(&owners) {
                    Some(x) => format!("in \"{}\"", self.name(x)),
                    None => "in no common component".to_string(),
                };
                d.push(warning(
                    &o.label,
                    format!("anchor outside the component: {} belongs to \"{}\"; the sentence would fit {locate}", cov.files[i], self.name(owner)),
                ));
            }
        }
        if !self.has_files() {
            return d;
        }
        // files in a component and in an exclusion; ties with no containment
        let mut groups: BTreeMap<Vec<usize>, Vec<usize>> = BTreeMap::new();
        let mut loose: Vec<&str> = Vec::new();
        for i in 0..cov.files.len() {
            match (cov.owner[i], cov.excl[i]) {
                (None, None) => loose.push(&cov.files[i]),
                (Some(o), Some(x)) => {
                    let (q, g) = &self.excls[x];
                    d.push(warning(
                        &g.label,
                        format!("file in a component and in an exclusion: {} (component \"{}\", exclusion of \"{}\": {})", cov.files[i], self.name(o), self.name(*q), g.text),
                    ));
                }
                _ => {}
            }
            if !cov.ties[i].is_empty() {
                groups.entry(cov.ties[i].clone()).or_default().push(i);
            }
        }
        for (ids, fs) in groups {
            let names: Vec<String> = ids.iter().map(|&c| format!("\"{}\"", self.name(c))).collect();
            let ex: Vec<&str> = fs.iter().take(3).map(|&i| cov.files[i].as_str()).collect();
            let more = if fs.len() > 3 { ", ..." } else { "" };
            let label = self.globs[ids[0]].first().map(|g| g.label.as_str()).unwrap_or("");
            d.push(warning(
                label,
                format!(
                    "components with no containment relation match the same file ({} files: {}{more}); it goes to {}, tied: {}",
                    fs.len(),
                    ex.join(", "),
                    names[0],
                    names.join(", ")
                ),
            ));
        }
        if !loose.is_empty() {
            let ex: Vec<&str> = loose.iter().take(20).copied().collect();
            let more = if loose.len() > ex.len() { ", ..." } else { "" };
            d.push(warning(
                "",
                format!(
                    "{} of {} files with no component and no exclusion (coverage {}%): {}{more}",
                    loose.len(),
                    cov.files.len(),
                    cov.pct(),
                    ex.join(", ")
                ),
            ));
        }
        d
    }

    // ---------------------------------------------------------------- show

    /// Sentences (facts and invariants) that mention entities of `inside`, in English or Portuguese.
    /// Each sentence comes with the free notes attached to it (the notes hold the canonical English sentence).
    fn sentences_of(&self, inside: &BTreeSet<usize>, direct: &BTreeSet<usize>, file: &str, pt: bool) -> (Vec<(String, Vec<String>)>, Vec<(String, Vec<String>)>) {
        let a = self.a;
        let anchor_match = |anchor: &[String]| anchor.iter().any(|x| Glob::new(part_anchor(x).0).matches(file));
        let suffix = |anchor: &[String]| anchor.iter().map(|x| format!(" ^{x}")).collect::<String>();
        let ids = |f: &Fact| {
            let mut m = Vec::new();
            mentions(f, &mut m);
            m.into_iter().filter_map(|e| self.index(e)).collect::<Vec<usize>>()
        };
        let notes = |en: &str| -> Vec<String> { a.notes.iter().filter(|n| n.sentence.as_deref() == Some(en)).map(|n| n.text.clone()).collect() };
        let (mut sentences, mut invs) = (BTreeMap::new(), BTreeMap::new());
        for f in &a.facts {
            let m = ids(f);
            let cites = m.iter().any(|c| direct.contains(c));
            let cross = m.iter().any(|c| inside.contains(c)) && m.iter().any(|c| !inside.contains(c));
            let anchor = a.anchors_of_fact(f);
            // "X contains Y" with Y inside the component only places it in the hierarchy (goes in `inside`)
            let placed = f.rel() == "a.contains" && f.obj().and_then(|o| self.index(o)).is_some_and(|o| inside.contains(&o));
            if (cites || cross) && !placed || anchor_match(&anchor) {
                let en = canon::fact_en(a, f);
                let t = if pt { canon::fact_pt(a, f) } else { en.clone() };
                sentences.entry(format!("{t}{}", suffix(&anchor))).or_insert_with(Vec::new).extend(notes(&en));
            }
        }
        for i in &a.invariants {
            let m: Vec<usize> = i.when.iter().flatten().chain(i.then.iter()).flat_map(ids).collect();
            let anchor = a.anchors_of_invariant(i);
            if m.iter().any(|c| inside.contains(c)) || anchor_match(&anchor) {
                let en_forms = canon::invariant_en(a, i);
                let ns: Vec<String> = en_forms.iter().flat_map(|en| notes(en)).collect();
                let ts = if pt { vec![canon::invariant_sentence_pt(a, i)] } else { en_forms };
                for t in ts {
                    invs.entry(format!("{t}{}", suffix(&anchor))).or_insert_with(Vec::new).extend(ns.iter().cloned());
                }
            }
        }
        (sentences.into_iter().collect(), invs.into_iter().collect())
    }

    /// `show`: component that owns the file, what it is part of, sentences that cite it,
    /// invariants and gaps. Returns (output, exit code).
    pub fn show(&self, rel: &str, pt: bool) -> (String, i32) {
        let (owner, tie) = self.owner_and_tie(rel);
        let Some(owner) = owner else {
            if let Some(x) = self.excluded_by(rel) {
                let (q, g) = &self.excls[x];
                return (format!("{rel}: excluded by \"{}\" of \"{}\" ({})\n", g.text, self.name(*q), g.label), 0);
            }
            // no component, but there may be a sentence anchored in the file
            let empty = BTreeSet::new();
            let (fs, is) = self.sentences_of(&empty, &empty, rel, pt);
            let mut o = format!("{rel}: file with no component\n");
            write_sentences(&mut o, &fs, &is);
            return (o, 1);
        };
        let comps = if tie.len() > 1 { tie } else { vec![owner] };
        let mut o = String::new();
        let mut inside: BTreeSet<usize> = BTreeSet::new();
        let mut direct: BTreeSet<usize> = BTreeSet::new();
        for &c in &comps {
            let labels: Vec<&str> = self.globs[c].iter().map(|g| g.label.as_str()).collect::<BTreeSet<_>>().into_iter().collect();
            let mark = if c == owner && comps.len() > 1 { " (tie: this one gets it)" } else if comps.len() > 1 { " (tie)" } else { "" };
            let _ = writeln!(o, "component \"{}\" ({}) {}{mark}", self.name(c), self.kind_en(c), labels.join(" "));
            let gs: Vec<&str> = self.globs[c].iter().map(|g| g.text.as_str()).collect();
            let _ = writeln!(o, "files {}", gs.join(" "));
            if let Some(layer) = self.a.layers.get(&self.ids[c]) {
                let _ = writeln!(o, "layer \"{}\"", self.a.name(layer));
            }
            for &x in &self.anchor[c] {
                let _ = writeln!(o, "inside \"{}\" ({})", self.name(x), self.kind_en(x));
            }
            for &x in &self.children[c] {
                let _ = writeln!(o, "contains \"{}\" ({})", self.name(x), self.kind_en(x));
            }
            direct.insert(c);
            inside.insert(c);
            inside.extend(self.descendants(c));
        }
        let (fs, is) = self.sentences_of(&inside, &direct, rel, pt);
        write_sentences(&mut o, &fs, &is);
        for l in &self.a.gaps {
            if let Some(c) = self.index(&l.entity).filter(|c| inside.contains(c)) {
                let _ = writeln!(o, "gap {} of \"{}\" ({})", l.role, self.name(c), self.kind_en(c));
            }
        }
        (o, 0)
    }

    // ---------------------------------------------------------------- stale

    /// Whose sentence an origin is: the component of the subject of the first fact or invariant
    /// (the subject itself, if no component contains it).
    fn who(&self, facts: &[Fact], invs: &[Invariant]) -> String {
        let eid = facts.first().map(Fact::subject).or_else(|| invs.first().and_then(|i| i.then.first()).map(Fact::subject));
        let Some(eid) = eid else { return String::new() };
        match self.index(eid).and_then(|c| self.component_of(c)) {
            Some(c) => self.name(c).to_string(),
            None => self.a.name(eid).to_string(),
        }
    }

    /// One line per sentence with an anchor in a changed file: `<who> | <sentence> | <file>`.
    pub fn sentences_review(&self, changed: &BTreeSet<String>, ignore: &[String], truncate_line: &dyn Fn(&str) -> String) -> Vec<String> {
        let mut v = Vec::new();
        for o in &self.a.origins {
            let finding = o.anchors.iter().find_map(|anchor| {
                let g = Glob::new(part_anchor(anchor).0);
                changed.iter().find(|f| !ignore.contains(f) && g.matches(f))
            });
            let Some(f) = finding else { continue };
            let sentence = o
                .facts
                .first()
                .map(|f| canon::fact_en(self.a, f))
                .or_else(|| o.invariants.first().and_then(|i| canon::invariant_en(self.a, i).into_iter().next()))
                .unwrap_or_default();
            v.push(format!("{} | {} | {f}", self.who(&o.facts, &o.invariants), truncate_line(&sentence)));
        }
        v
    }

    // ---------------------------------------------------------------- suggestions

    /// Ready-to-paste English sentence that covers a relation from `x` to `y`.
    pub fn suggestion_relation(&self, x: usize, verb: &str, y: usize) -> String {
        format!("The {} \"{}\" {verb} the {} \"{}\".", self.kind_en(x), self.name(x), self.kind_en(y), self.name(y))
    }

    pub fn suggestion_external(&self, x: usize, verb: &str, kind: &str, name: &str) -> String {
        format!("The {} \"{}\" {verb} the {kind} \"{name}\".", self.kind_en(x), self.name(x))
    }
}

/// `sentence`/`invariant` lines of `show`, each followed by its free notes (`note`,
/// informal text: not part of the IR).
fn write_sentences(o: &mut String, fs: &[(String, Vec<String>)], is: &[(String, Vec<String>)]) {
    for (label, v) in [("sentence", fs), ("invariant", is)] {
        for (t, ns) in v {
            let _ = writeln!(o, "{label} {t}");
            let mut views: Vec<&String> = Vec::new();
            for n in ns {
                if !views.contains(&n) {
                    views.push(n);
                    let _ = writeln!(o, "note (informal) {n}");
                }
            }
        }
    }
}

/// What `facts` and `measure` need to know about the project: numbered components, owner of each
/// file, containment and relations between components.
impl Project<'_> {
    pub fn n(&self) -> usize {
        self.ids.len()
    }

    pub fn id(&self, c: usize) -> &str {
        self.name(c)
    }

    pub fn owner(&self, file: &str) -> Option<usize> {
        self.owner_and_tie(file).0
    }

    pub fn inside(&self, c: usize, a: usize) -> bool {
        self.contained_in(c, a)
    }

    pub fn edges(&self) -> Vec<(usize, usize)> {
        self.edges.clone()
    }

    /// No files or children, of structural kind or program: an outside service, disk or program.
    pub fn external(&self, c: usize) -> bool {
        if !self.globs[c].is_empty() || !self.children[c].is_empty() {
            return false;
        }
        let t = self.kind_id(c);
        t == "e.program" || self.a.lex.ancestors(t).iter().any(|x| self.a.lex.kind(x).is_some_and(|t| t.structural))
    }

    /// External, or only globs without `**` and without a code extension, or a group whose children are all data.
    pub fn datum(&self, c: usize) -> bool {
        fn rec(p: &Project, c: usize, seen: &mut Vec<usize>) -> bool {
            const CODE: [&str; 8] = [".rs", ".ts", ".tsx", ".js", ".mjs", ".py", ".svelte", ".sh"];
            if p.external(c) {
                return true;
            }
            if seen.contains(&c) {
                return false;
            }
            seen.push(c);
            if !p.globs[c].is_empty() {
                return p.globs[c].iter().all(|g| !g.text.contains("**") && !CODE.iter().any(|x| g.text.contains(x)));
            }
            !p.children[c].is_empty() && p.children[c].iter().all(|&f| rec(p, f, seen))
        }
        rec(self, c, &mut Vec::new())
    }
}
