//! `where`: quick orientation for whoever is about to change the code. Given words about the
//! change (or a file path), it returns a few ranked components, with files, anchored symbols,
//! the sentences and invariants that mention the terms and the one-hop neighbors (names only).
//! Compact text (2 KB target) made to decide what to open, not to understand the whole system
//! (that is what the views are for). Fully deterministic: only `BTreeMap`/`BTreeSet`, ties
//! broken by name and by id.

use super::analysis::{Analysis, Fact};
use super::canon;
use super::code::rel_in_root;
use super::json::J;
use super::project::Project;
use super::views::declared::norm;
use crate::glob::Glob;
use crate::symbol::part_anchor;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;
use crate::ctx::Ctx;
use crate::{errln, out, outln};

/// Default size of the text output, in bytes.
pub const MAX_DEFAULT: usize = 2560;

/// Fraction (in %) of the size reserved for "nearby rules"; what it does not use goes back to
/// the components, and the components never get less than the rest.
const RESERVE_NEARBY: usize = 35;

/// How many of the first ranked components have their neighbors scanned.
const SOURCES_NEARBY: usize = 5;

/// Cap on the "nearby rules" lines in the JSON.
const MAX_NEARBY_JSON: usize = 60;

/// Verbs that read or write state: whoever uses them on the same object is a neighbor.
const STATE: [&str; 8] = ["a.reads", "a.writes", "a.changes", "a.overwrites", "a.fills", "a.consumes", "a.removes", "a.invalidates"];

/// Words with no search value (articles, prepositions and connectives).
const EMPTY: [&str; 19] = ["a", "an", "the", "of", "to", "in", "on", "for", "and", "or", "is", "are", "with", "from", "by", "at", "as", "it", "be"];

pub struct Options {
    pub terms: Vec<String>,
    pub max: usize,
    pub pt: bool,
    pub json: bool,
}

/// Search term: a word (already without accents or case) or a project file.
enum Term {
    Word(String),
    File(String),
}

/// Words of a text: split on non-alphanumerics and on case boundaries (`SlotDetection`,
/// `requires_mtls`); without accents and lowercase.
fn word_set(t: &str) -> BTreeSet<String> {
    fn close(current: &mut String, out: &mut BTreeSet<String>) {
        if !current.is_empty() {
            out.insert(norm(current));
            current.clear();
        }
    }
    let mut out = BTreeSet::new();
    let mut current = String::new();
    let mut previous_lowercase = false;
    for ch in t.chars() {
        if ch.is_alphanumeric() {
            if ch.is_uppercase() && previous_lowercase {
                close(&mut current, &mut out);
            }
            current.push(ch);
            previous_lowercase = ch.is_lowercase();
        } else {
            close(&mut current, &mut out);
            previous_lowercase = false;
        }
    }
    close(&mut current, &mut out);
    out
}

/// The term matches some word, whole or by prefix; a simple plural (`routes`)
/// also counts.
fn matches_words(t: &str, ws: &BTreeSet<String>) -> bool {
    let singular = t.strip_suffix('s').filter(|x| x.len() >= 3);
    let serve = |v: &str| ws.iter().any(|w| w == v || (v.len() >= 2 && w.starts_with(v)));
    serve(t) || singular.is_some_and(serve)
}

fn truncate_line(t: &str, n: usize) -> String {
    if t.chars().count() <= n {
        return t.to_string();
    }
    let c: String = t.chars().take(n.saturating_sub(1)).collect();
    format!("{}…", c.trim_end())
}

/// Sentence (fact or invariant) of a component, with what the search needs.
#[derive(Clone)]
struct SentenceRec {
    en: String,
    pt: String,
    inv: bool,
    anchors: Vec<String>,
    words: BTreeSet<String>,
    note_words: BTreeSet<String>,
    /// notes attached to the sentence, as the text of each one
    notes: Vec<String>,
}

impl SentenceRec {
    /// The sentence with its anchors; any cut is in the body of the sentence, never in the anchors.
    fn text(&self, pt: bool, limit: usize) -> String {
        let t = if pt { &self.pt } else { &self.en };
        let anchor: String = self.anchors.iter().map(|x| format!(" ^{x}")).collect();
        let body = limit.saturating_sub(anchor.chars().count()).max(60);
        format!("{}{anchor}", truncate_line(t, body))
    }
}

/// Ranking unit: a component (an entity with files) or an entity that owns sentences without
/// being inside any component.
#[derive(Default)]
struct Unit {
    name_words: BTreeSet<String>,
    strict: BTreeSet<String>,
    wide: BTreeSet<String>,
    symbols: BTreeSet<(String, String)>,
    sentences: Vec<SentenceRec>,
    /// notes of the structural sentences (`is implemented in`, layer): the component's description
    description: Vec<String>,
    desc_words: BTreeSet<String>,
}

/// Result of a unit for the search.
struct Finding {
    c: usize,
    points: u64,
    /// indices of the matching sentences, with the number of matched terms
    sentences: Vec<(usize, usize)>,
    symbols: Vec<(String, String)>,
}

struct Item {
    name: String,
    description: Option<String>,
    kind: String,
    layer: Option<String>,
    files: Vec<String>,
    symbols: Vec<(String, String)>,
    /// invariants first
    sentences: Vec<SentenceRec>,
    uses: Vec<String>,
    used_by: Vec<String>,
    state: Vec<String>,
    gaps: Vec<String>,
    absences: Vec<String>,
}

/// Neighbor worth citing: a component or a structural external (program, file, folder,
/// device, endpoint); loose values, codes and routes are not.
fn neighbor_useful(p: &Project, c: usize) -> bool {
    p.and_component(c) || p.external(c)
}

fn list_short(v: &[String], n: usize) -> String {
    if v.len() <= n {
        v.join(", ")
    } else {
        format!("{}, +{}", v[..n].join(", "), v.len() - n)
    }
}

/// A rule of a one-hop neighbor of the ranked components.
struct Nearby {
    /// the neighbor the rule comes from
    nbr: String,
    /// the ranked components it is a neighbor of, best to worst
    from: Vec<String>,
    /// position of the best of those components in the ranking
    rank: usize,
    class: Class,
    /// position among the lines of the same neighbor and the same level (the output order interleaves)
    ordinal: usize,
    text: String,
    anchors: Vec<String>,
}

/// Kind of line, from the one that matters most to the one that matters least.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Class {
    InvariantAnchored,
    Invariant,
    Rule,
    Note,
    Absence,
    Gap,
    Warning,
    Description,
}

impl Class {
    fn label(self) -> &'static str {
        match self {
            Class::InvariantAnchored | Class::Invariant => "invariant",
            Class::Rule => "rule",
            Class::Note | Class::Description => "note",
            Class::Absence => "absence",
            Class::Gap => "gap",
            Class::Warning => "warning",
        }
    }

    /// Importance level: the anchored invariants, then the other rules and notes, finally what
    /// only warns.
    fn level(self) -> u8 {
        match self {
            Class::InvariantAnchored => 0,
            Class::Invariant | Class::Rule | Class::Note | Class::Absence => 1,
            _ => 2,
        }
    }
}

impl Nearby {
    /// The text line: `- [neighbor] ...`; the cut falls in the body, never in the anchors.
    fn line(&self, limit: usize) -> String {
        let anchor: String = self.anchors.iter().map(|x| format!(" ^{x}")).collect();
        let label = match self.class {
            Class::InvariantAnchored | Class::Invariant => String::new(),
            c => format!("{}: ", c.label()),
        };
        let prefix = format!("- [{}] {label}", self.nbr);
        let body = limit.saturating_sub(prefix.chars().count() + anchor.chars().count()).max(40);
        format!("{prefix}{}{anchor}\n", truncate_line(&self.text, body))
    }
}

/// Entity the rule belongs to: the component that owns the anchor's file when the entity is not
/// inside any component.
fn owner_of(p: &Project, c: usize, anchors: &[String]) -> usize {
    let u = p.component_of(c).unwrap_or(c);
    if p.and_component(u) {
        return u;
    }
    anchors.iter().find_map(|x| p.owner(part_anchor(x).0)).unwrap_or(u)
}

/// Rules nearby: invariants, ∀ rules, notes, gaps, warnings and absences of the one-hop
/// neighbors (relation in either direction, shared state, direct containment) of the first
/// ranked components, already in output order (deterministic). Does not repeat what the
/// components section already shows (the sentences that match the terms).
fn rules_by_nearby(
    p: &Project,
    un: &BTreeMap<usize, Unit>,
    findings: &[Finding],
    adj: &BTreeMap<usize, BTreeSet<usize>>,
    pt: bool,
) -> Vec<Nearby> {
    let a = p.a;
    let unit_of = |x: usize| p.component_of(x).unwrap_or(x);
    let sources: Vec<usize> = findings.iter().take(SOURCES_NEARBY).map(|x| x.c).collect();
    // neighbor -> positions (in the ranking) of the sources it is a neighbor of
    let mut nbr: BTreeMap<usize, BTreeSet<usize>> = BTreeMap::new();
    for (r, c) in sources.iter().enumerate() {
        for &y in adj.get(c).into_iter().flatten() {
            if y != *c {
                nbr.entry(y).or_default().insert(r);
            }
        }
    }
    // sentences that already come out with the components (they match the terms)
    let shown: BTreeSet<(usize, String, bool)> =
        findings.iter().take(SOURCES_NEARBY).flat_map(|hit| hit.sentences.iter().map(move |&(k, _)| (hit.c, un[&hit.c].sentences[k].en.clone(), un[&hit.c].sentences[k].inv))).collect();

    let mut output: Vec<Nearby> = Vec::new();
    for (&y, rs) in &nbr {
        let best = *rs.iter().next().expect("at least one source");
        let from: Vec<String> = rs.iter().map(|&r| p.name(sources[r]).to_string()).collect();
        let mut lines: Vec<(Class, String, Vec<String>)> = Vec::new();
        if let Some(u) = un.get(&y) {
            for sent in &u.sentences {
                let txt = if pt { &sent.pt } else { &sent.en };
                if sent.inv && !shown.contains(&(y, sent.en.clone(), true)) {
                    let cl = if sent.anchors.is_empty() { Class::Invariant } else { Class::InvariantAnchored };
                    lines.push((cl, txt.clone(), sent.anchors.clone()));
                }
                for n in &sent.notes {
                    lines.push((Class::Note, n.clone(), Vec::new()));
                }
            }
            for d in &u.description {
                lines.push((Class::Description, d.clone(), Vec::new()));
            }
        }
        for r in &a.rules {
            let Some(c) = p.index(&r.scope) else { continue };
            let anchor = a.anchors_of_rule(r);
            if owner_of(p, c, &anchor) != y {
                continue;
            }
            let txt = if pt { canon::rule_pt(a, r) } else { canon::rule_en(a, r) };
            let cl = if anchor.is_empty() { Class::Rule } else { Class::InvariantAnchored };
            lines.push((cl, txt, anchor));
            for n in a.notes.iter().filter(|x| x.sentence.as_deref() == Some(canon::rule_en(a, r).as_str())) {
                lines.push((Class::Note, n.text.clone(), Vec::new()));
            }
        }
        for l in a.gaps.iter().filter(|l| p.index(&l.entity).is_some_and(|x| unit_of(x) == y)) {
            lines.push((Class::Gap, format!("{} of \"{}\"", l.role, a.name(&l.entity)), Vec::new()));
        }
        for warns in a.warnings.iter().filter(|v| p.index(&v.entity).is_some_and(|x| unit_of(x) == y)) {
            lines.push((Class::Warning, warns.text.clone(), Vec::new()));
        }
        for f in &a.facts {
            if let Fact::Absent { subject, role } = f {
                if p.index(subject).is_some_and(|x| owner_of(p, x, &a.anchors_of_fact(f)) == y) {
                    lines.push((Class::Absence, format!("{role} of \"{}\"", a.name(subject)), Vec::new()));
                }
            }
        }
        lines.sort();
        lines.dedup_by(|x, y| x.0 == y.0 && x.1 == y.1);
        // ordinal within the level, in class order
        let mut count: BTreeMap<u8, usize> = BTreeMap::new();
        for (class, text, anchors) in lines {
            let o = count.entry(class.level()).or_default();
            output.push(Nearby { nbr: p.name(y).to_string(), from: from.clone(), rank: best, class, ordinal: *o, text, anchors });
            *o += 1;
        }
    }
    // per level, each neighbor giving one line at a time (best ranked first)
    output.sort_by(|x, y| (x.class.level(), x.ordinal, x.rank, &x.nbr, x.class, &x.text).cmp(&(y.class.level(), y.ordinal, y.rank, &y.nbr, y.class, &y.text)));
    // the output interleaves two anchored invariants for each note or other rule, and leaves
    // what only warns for the end; without one of the levels, the other applies
    let mut levels: [std::collections::VecDeque<Nearby>; 3] = Default::default();
    for r in output {
        levels[r.class.level() as usize].push_back(r);
    }
    let mut order: Vec<Nearby> = Vec::new();
    loop {
        let before = order.len();
        for _ in 0..2 {
            order.extend(levels[0].pop_front());
        }
        order.extend(levels[1].pop_front());
        if order.len() == before {
            break;
        }
    }
    order.extend(levels[2].drain(..));
    order
}

pub fn run(cx: &Ctx, a: &Analysis, op: &Options) -> i32 {
    let p = Project::new(a);
    let root = cx.fs.canonicalize(cx.fs.root());

    // terms: project file or words (without the empty ones, without repeats)
    let mut terms: Vec<Term> = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    for arg in &op.terms {
        let looks_path = arg.contains('/') || arg.contains('.');
        if looks_path {
            if let Some(rel) = rel_in_root(cx.fs, &root, arg) {
                if cx.fs.is_file(&root.join(&rel)) || p.owner(&rel).is_some() || p.excluded_by(&rel).is_some() {
                    if seen.insert(format!("file:{rel}")) {
                        terms.push(Term::File(rel));
                    }
                    continue;
                }
            }
        }
        for w in word_set(arg) {
            if !EMPTY.contains(&w.as_str()) && seen.insert(w.clone()) {
                terms.push(Term::Word(w));
            }
        }
    }
    if terms.is_empty() {
        errln!(cx, "where: no search term (words about the change, or a file)");
        return 2;
    }

    // units and their sentences
    let n = p.n();
    let unit_of = |x: usize| p.component_of(x).unwrap_or(x);
    let mut un: BTreeMap<usize, Unit> = BTreeMap::new();
    let create = |c: usize, un: &mut BTreeMap<usize, Unit>| {
        un.entry(c).or_insert_with(|| {
            let mut u = Unit { name_words: word_set(p.name(c)), ..Default::default() };
            for g in &p.globs[c] {
                let target = if g.text.contains("**") { &mut u.wide } else { &mut u.strict };
                target.extend(word_set(&g.text));
            }
            u
        });
    };
    for c in 0..n {
        if p.and_component(c) {
            create(c, &mut un);
        }
    }
    let notes_of = |en: &str| -> BTreeSet<String> { a.notes.iter().filter(|x| x.sentence.as_deref() == Some(en)).flat_map(|x| word_set(&x.text)).collect() };
    let notes_texts = |en: &str| -> Vec<String> {
        let mut v: Vec<String> = Vec::new();
        for n in a.notes.iter().filter(|x| x.sentence.as_deref() == Some(en)) {
            if !v.contains(&n.text) {
                v.push(n.text.clone());
            }
        }
        v
    };
    let add = |c: usize, mut sent: SentenceRec, un: &mut BTreeMap<usize, Unit>| {
        create(c, un);
        let u = un.get_mut(&c).expect("unit created");
        for x in &sent.anchors {
            let (file, sym) = part_anchor(x);
            let target = if file.contains("**") { &mut u.wide } else { &mut u.strict };
            target.extend(word_set(file));
            if let Some(s) = sym {
                u.symbols.insert((file.to_string(), s.to_string()));
            }
        }
        if let Some(old) = u.sentences.iter_mut().find(|x| x.en == sent.en && x.inv == sent.inv) {
            for x in sent.anchors.drain(..) {
                if !old.anchors.contains(&x) {
                    old.anchors.push(x);
                }
            }
            old.anchors.sort();
            return;
        }
        u.sentences.push(sent);
    };
    // the subject's own name does not count as a citation of the term (the sentence is its own)
    let collect_facts = |en: String, pt: String, inv: bool, anchors: Vec<String>, subject: &str| -> SentenceRec {
        let strip_subject = |t: &str| t.replace(&format!("\"{subject}\""), "").replace(&format!("“{subject}”"), "");
        let mut words = word_set(&strip_subject(&en));
        words.extend(word_set(&strip_subject(&pt)));
        for x in &anchors {
            words.extend(word_set(x));
        }
        let note_words = notes_of(&en);
        let notes = notes_texts(&en);
        SentenceRec { en, pt, inv, anchors, words, note_words, notes }
    };
    // a loose entity sentence (route, table, option...) goes to the owner of the anchor's file
    let owner_of_sentence = |c: usize, anchors: &[String]| -> usize {
        let u = unit_of(c);
        if p.and_component(u) {
            return u;
        }
        anchors.iter().find_map(|x| p.owner(part_anchor(x).0)).unwrap_or(u)
    };
    for f in &a.facts {
        let structural = matches!(f.rel(), "a.implemented_in" | "a.excludes") || (f.rel() == "a.contains" && f.obj().and_then(|o| p.index(o)).is_some_and(|o| p.and_component(o)));
        if structural {
            // the sentence does not enter, but its notes describe the subject
            if let Some(c) = p.index(f.subject()) {
                let c = unit_of(c);
                create(c, &mut un);
                let u = un.get_mut(&c).expect("unit created");
                for n in a.notes.iter().filter(|x| x.sentence.as_deref() == Some(canon::fact_en(a, f).as_str())) {
                    if !u.description.contains(&n.text) {
                        u.description.push(n.text.clone());
                        u.desc_words.extend(word_set(&n.text));
                    }
                }
            }
            continue;
        }
        let Some(c) = p.index(f.subject()) else { continue };
        let anchor = a.anchors_of_fact(f);
        let owner = owner_of_sentence(c, &anchor);
        let sent = collect_facts(canon::fact_en(a, f), canon::fact_pt(a, f), false, anchor, a.name(f.subject()));
        add(owner, sent, &mut un);
    }
    for i in &a.invariants {
        let Some(c) = i.then.first().and_then(|f| p.index(f.subject())) else { continue };
        let anchor = a.anchors_of_invariant(i);
        let pt = canon::invariant_sentence_pt(a, i);
        let owner = owner_of_sentence(c, &anchor);
        let subject = a.name(i.then[0].subject());
        for en in canon::invariant_en(a, i) {
            add(owner, collect_facts(en, pt.clone(), true, anchor.clone(), subject), &mut un);
        }
    }

    // requested files: owners and glob of the anchor
    let mut owners: BTreeMap<String, BTreeSet<usize>> = BTreeMap::new();
    for t in &terms {
        if let Term::File(rel) = t {
            let (_, tie) = p.owner_and_tie(rel);
            owners.insert(rel.clone(), tie.into_iter().collect());
        }
    }
    let anchor_match = |sent: &SentenceRec, rel: &str| sent.anchors.iter().any(|x| Glob::new(part_anchor(x).0).matches(rel));

    // how many units each word reaches (rarity of the term)
    let reach = |t: &str, u: &Unit| {
        matches_words(t, &u.name_words)
            || matches_words(t, &u.strict)
            || matches_words(t, &u.wide)
            || matches_words(t, &u.desc_words)
            || u.symbols.iter().any(|(_, s)| matches_words(t, &word_set(s)))
            || u.sentences.iter().any(|f| matches_words(t, &f.words) || matches_words(t, &f.note_words))
    };
    let total = un.len().max(1) as u64;
    let mut weight: BTreeMap<String, u64> = BTreeMap::new();
    for t in &terms {
        if let Term::Word(w) = t {
            let df = un.values().filter(|u| reach(w, u)).count() as u64;
            weight.insert(w.clone(), 1 + 3 * (total - df.min(total)) / total);
        }
    }

    // scoring
    let mut findings: Vec<Finding> = Vec::new();
    for (&c, u) in &un {
        let mut points: u64 = 0;
        let mut matched: BTreeMap<usize, usize> = BTreeMap::new();
        let mut syms: BTreeSet<(String, String)> = BTreeSet::new();
        let mut distinct = 0;
        for t in &terms {
            match t {
                Term::File(rel) => {
                    let mut gain = 0;
                    if owners.get(rel).is_some_and(|d| d.contains(&c)) {
                        gain += 40;
                    }
                    for (k, f) in u.sentences.iter().enumerate() {
                        if anchor_match(f, rel) {
                            *matched.entry(k).or_default() += 1;
                            gain += if f.inv { 6 } else { 4 };
                        }
                    }
                    for (file, s) in &u.symbols {
                        if Glob::new(file).matches(rel) {
                            syms.insert((file.clone(), s.clone()));
                        }
                    }
                    points += gain;
                    distinct += (gain > 0) as u64;
                }
                Term::Word(w) => {
                    let mut s: u64 = 0;
                    if matches_words(w, &u.name_words) {
                        s += 8;
                    }
                    let hits: Vec<&(String, String)> = u.symbols.iter().filter(|(_, x)| matches_words(w, &word_set(x))).collect();
                    if !hits.is_empty() {
                        s += 6;
                        syms.extend(hits.into_iter().cloned());
                    }
                    if matches_words(w, &u.desc_words) {
                        s += 4;
                    }
                    if matches_words(w, &u.strict) {
                        s += 5;
                    } else if matches_words(w, &u.wide) {
                        s += 2;
                    }
                    let (mut ni, mut nf, mut nn) = (0u64, 0u64, 0u64);
                    for (k, f) in u.sentences.iter().enumerate() {
                        if matches_words(w, &f.words) {
                            *matched.entry(k).or_default() += 1;
                            if f.inv {
                                ni += 1;
                            } else {
                                nf += 1;
                            }
                        } else if matches_words(w, &f.note_words) {
                            nn += 1;
                        }
                    }
                    if ni > 0 {
                        s += 4 + (ni - 1).min(2);
                    }
                    if nf > 0 {
                        s += 3 + (nf - 1).min(2);
                    }
                    if nn > 0 {
                        s += 2;
                    }
                    points += s * weight.get(w).copied().unwrap_or(1);
                    distinct += (s > 0) as u64;
                }
            }
        }
        if points == 0 {
            continue;
        }
        if distinct as usize == terms.len() && terms.len() > 1 {
            points += 6;
        }
        if p.and_component(c) {
            points += 2;
        }
        let mut sentences: Vec<(usize, usize)> = matched.into_iter().collect();
        sentences.sort_by(|x, y| {
            let (fx, fy) = (&u.sentences[x.0], &u.sentences[y.0]);
            (!fx.inv, std::cmp::Reverse(x.1), &fx.en).cmp(&(!fy.inv, std::cmp::Reverse(y.1), &fy.en))
        });
        findings.push(Finding { c, points, sentences, symbols: syms.into_iter().collect() });
    }
    findings.sort_by(|x, y| y.points.cmp(&x.points).then_with(|| (p.name(x.c), &p.ids[x.c]).cmp(&(p.name(y.c), &p.ids[y.c]))));

    let legend = truncate_line(&op.terms.join(" "), 60);
    if findings.is_empty() {
        let file_pairs: Vec<&String> = owners.keys().collect();
        if let Some(rel) = file_pairs.first() {
            let suffix = p.excluded_by(rel).map(|_| " (excluded from the description)").unwrap_or(" (no component)");
            outln!(cx, "where: {legend}: no component matches; file {rel}{suffix}");
        } else {
            outln!(cx, "where: {legend}: no component matches");
        }
        return 1;
    }

    // one-hop neighbors
    let mut uses: BTreeMap<usize, BTreeSet<usize>> = BTreeMap::new();
    let mut used: BTreeMap<usize, BTreeSet<usize>> = BTreeMap::new();
    for (from, to) in p.edges() {
        let (x, y) = (unit_of(from), unit_of(to));
        if x == y || p.contained_in(x, y) || p.contained_in(y, x) || !neighbor_useful(&p, x) || !neighbor_useful(&p, y) {
            continue;
        }
        uses.entry(x).or_default().insert(y);
        used.entry(y).or_default().insert(x);
    }
    let mut accesses: BTreeMap<usize, BTreeSet<usize>> = BTreeMap::new();
    for f in &a.facts {
        if let (Fact::Rel { rel, subject, .. }, Some(o)) = (f, f.obj()) {
            if STATE.contains(&rel.as_str()) && f.mode().is_none() {
                if let (Some(x), Some(oi)) = (p.index(subject), p.index(o)) {
                    accesses.entry(oi).or_default().insert(unit_of(x));
                }
            }
        }
    }
    let names = |s: Option<&BTreeSet<usize>>| -> Vec<String> {
        let mut v: Vec<String> = s.into_iter().flatten().map(|&c| p.name(c).to_string()).collect();
        v.sort();
        v.dedup();
        v
    };

    // full neighborhood for "rules nearby": relations in either direction (also between
    // container and contained), direct containment and shared state
    let mut adj: BTreeMap<usize, BTreeSet<usize>> = BTreeMap::new();
    let mut is_linked = |x: usize, y: usize| {
        if x != y {
            adj.entry(x).or_default().insert(y);
            adj.entry(y).or_default().insert(x);
        }
    };
    for (from, to) in p.edges() {
        let (x, y) = (unit_of(from), unit_of(to));
        if neighbor_useful(&p, x) && neighbor_useful(&p, y) {
            is_linked(x, y);
        }
    }
    for parent in 0..n {
        for &child in &p.children[parent] {
            is_linked(unit_of(parent), unit_of(child));
        }
    }
    for who in accesses.values() {
        for &x in who {
            for &y in who {
                is_linked(x, y);
            }
        }
    }
    let nearby = rules_by_nearby(&p, &un, &findings, &adj, op.pt);

    let max_hits = findings.len().min(8);
    let mut items: Vec<Item> = Vec::new();
    for hit in findings.iter().take(max_hits) {
        let c = hit.c;
        let u = &un[&c];
        let mut state: BTreeSet<String> = BTreeSet::new();
        for (&o, who) in &accesses {
            if !who.contains(&c) {
                continue;
            }
            for &d in who {
                if d != c && !p.contained_in(d, c) && !p.contained_in(c, d) {
                    state.insert(format!("{} ({})", p.name(d), p.name(o)));
                }
            }
        }
        let mut symbols = hit.symbols.clone();
        for s in &u.symbols {
            if !symbols.contains(s) {
                symbols.push(s.clone());
            }
        }
        let gaps = a
            .gaps
            .iter()
            .filter(|l| p.index(&l.entity).is_some_and(|x| unit_of(x) == c))
            .map(|l| format!("{} of \"{}\"", l.role, a.name(&l.entity)))
            .collect();
        let mut absences: Vec<String> = a
            .facts
            .iter()
            .filter_map(|f| match f {
                Fact::Absent { subject, role } if p.index(subject).is_some_and(|x| owner_of_sentence(x, &a.anchors_of_fact(f)) == c) => Some(format!("{role} of \"{}\"", a.name(subject))),
                _ => None,
            })
            .collect();
        absences.sort();
        absences.dedup();
        items.push(Item {
            name: p.name(c).to_string(),
            description: u.description.first().cloned(),
            kind: if op.pt { a.lex.kind(p.kind_id(c)).map(|t| t.label_pt().to_string()).unwrap_or_default() } else { p.kind_en(c).to_string() },
            layer: a.layers.get(&p.ids[c]).map(|l| a.name(l).to_string()),
            files: p.globs[c].iter().map(|g| g.text.clone()).collect(),
            symbols,
            sentences: hit.sentences.iter().map(|&(k, _)| u.sentences[k].clone()).collect(),
            uses: names(uses.get(&c)),
            used_by: names(used.get(&c)),
            state: state.into_iter().collect(),
            gaps,
            absences,
        });
    }
    let remaining: Vec<String> = findings.iter().skip(max_hits).map(|x| p.name(x.c).to_string()).collect();

    if op.json {
        outln!(cx, "{}", json(&op.terms, &items, &findings, &remaining, &nearby, op.pt).indented());
        return 0;
    }
    out!(cx, "{}", text(&legend, &items, &remaining, &nearby, findings.len(), op.max, op.pt));
    0
}

fn json(terms: &[String], items: &[Item], findings: &[Finding], remaining: &[String], nearby: &[Nearby], pt: bool) -> J {
    let list = |v: &[String]| J::Arr(v.iter().map(|x| J::s(x)).collect());
    let comps = items
        .iter()
        .zip(findings)
        .map(|(i, hit)| {
            J::obj(vec![
                ("name", J::s(&i.name)),
                ("type", J::s(&i.kind)),
                ("description", i.description.as_deref().map(J::s).unwrap_or(J::Null)),
                ("layer", i.layer.as_deref().map(J::s).unwrap_or(J::Null)),
                ("points", J::Num(hit.points as f64)),
                ("files", list(&i.files)),
                ("symbols", J::Arr(i.symbols.iter().map(|(f, s)| J::obj(vec![("file", J::s(f)), ("symbol", J::s(s))])).collect())),
                ("sentences", list(&i.sentences.iter().map(|f| f.text(pt, 400)).collect::<Vec<_>>())),
                ("uses", list(&i.uses)),
                ("used_by", list(&i.used_by)),
                ("same_state", list(&i.state)),
                ("gaps", list(&i.gaps)),
                ("absences", list(&i.absences)),
            ])
        })
        .collect();
    let rules = nearby
        .iter()
        .take(MAX_NEARBY_JSON)
        .map(|r| {
            J::obj(vec![
                ("neighbor", J::s(&r.nbr)),
                ("near", list(&r.from)),
                ("type", J::s(r.class.label())),
                ("text", J::s(&r.text)),
                ("anchors", list(&r.anchors)),
            ])
        })
        .collect();
    J::obj(vec![("terms", list(terms)), ("components", J::Arr(comps)), ("nearby_rules", J::Arr(rules)), ("others", list(remaining))])
}

/// A labeled list line, or nothing if the list is empty.
fn label(o: &mut String, ind: &str, r: &str, v: &[String], n: usize) {
    if !v.is_empty() {
        let _ = writeln!(o, "{ind}{r} {}", list_short(v, n));
    }
}

/// Body of the output: the first `k` components, each at its level of detail (the first at the
/// fullest, the following ones increasingly lean; `reduz` slims them all).
fn render(legend: &str, items: &[Item], total: usize, k: usize, reduce: usize, pt: bool) -> String {
    let k = k.min(items.len());
    let mut o = String::new();
    let _ = writeln!(o, "where: {legend} ({k} of {total})");
    for (i, it) in items.iter().take(k).enumerate() {
        let tier = (if i == 0 { 0 } else if i <= 2 { 1 } else { 2 }) + reduce;
        let layer_label = it.layer.as_ref().map(|c| format!(" · layer {c}")).unwrap_or_default();
        let _ = writeln!(o, "{} {} · {}{layer_label}", i + 1, it.name, it.kind);
        if tier >= 3 {
            if let Some(f) = it.files.first() {
                let _ = writeln!(o, "  files {}", truncate_line(f, 70));
            }
            continue;
        }
        let (lim_file, n_sym, n_sent, n_nbr) = [(160, 8, 4, 8), (110, 4, 1, 4), (80, 3, 0, 0)][tier];
        if let (Some(d), true) = (&it.description, tier <= 1) {
            let _ = writeln!(o, "  note {}", truncate_line(d, if tier == 0 { 110 } else { 80 }));
        }
        if !it.files.is_empty() {
            let _ = writeln!(o, "  files {}", truncate_line(&it.files.join(" "), lim_file));
        }
        let mut by_file: Vec<(&str, Vec<&str>)> = Vec::new();
        for (f, s) in it.symbols.iter().take(n_sym) {
            match by_file.iter_mut().find(|(x, _)| *x == f) {
                Some((_, v)) => v.push(s),
                None => by_file.push((f, vec![s])),
            }
        }
        for (f, v) in by_file {
            let _ = writeln!(o, "  symbols {f}: {}", v.join(", "));
        }
        for sent in it.sentences.iter().take(n_sent) {
            let _ = writeln!(o, "  - {}", sent.text(pt, if tier == 0 { 170 } else { 120 }));
        }
        if n_nbr > 0 {
            label(&mut o, "  ", "uses", &it.uses, n_nbr.min(6));
            label(&mut o, "  ", "used by", &it.used_by, n_nbr.min(6));
            label(&mut o, "  ", "same state", &it.state, n_nbr.min(4));
        }
        if tier == 0 {
            label(&mut o, "  ", "gaps", &it.gaps, 3);
            label(&mut o, "  ", "absences", &it.absences, 3);
        }
    }
    o
}

/// Line `more a, b, +N` with as many names as fit in `space` bytes (at most 30), and how many
/// names it carries; with no room for anything, `None`.
fn line_more(names: &[String], space: usize) -> Option<(String, usize)> {
    if names.is_empty() {
        return Some((String::new(), 0));
    }
    for n in (0..=names.len().min(30)).rev() {
        let l = match (n, names.len() - n) {
            (0, r) => format!("+{r} components\n"),
            (n, 0) => format!("more {}\n", names[..n].join(", ")),
            (n, r) => format!("more {}, +{r}\n", names[..n].join(", ")),
        };
        if l.len() <= space {
            return Some((l, n));
        }
    }
    None
}

/// Header of the "nearby rules" section.
const HDR_NEARBY: &str = "nearby rules\n";

/// The "nearby rules" section with the lines that fit in `space` bytes, in the order received;
/// the last line shrinks to fit, if useful space is left. Empty if nothing fits.
fn section_nearby(nearby: &[Nearby], space: usize) -> String {
    let mut o = String::new();
    if space < HDR_NEARBY.len() {
        return o;
    }
    let mut rest = space - HDR_NEARBY.len();
    let mut n = 0;
    for r in nearby {
        let l = r.line(150);
        if l.len() <= rest {
            rest -= l.len();
            o.push_str(&l);
            n += 1;
            continue;
        }
        // last attempt: the shortest line that keeps the prefix and part of the body
        for lim in [120, 90, 70] {
            let l = r.line(lim);
            if l.len() <= rest && rest >= 70 {
                o.push_str(&l);
                n += 1;
                break;
            }
        }
        break;
    }
    if n == 0 {
        return String::new();
    }
    format!("{HDR_NEARBY}{o}")
}

fn text(legend: &str, items: &[Item], remaining: &[String], nearby: &[Nearby], total: usize, max: usize, pt: bool) -> String {
    // "nearby rules" reserves a part of the size; the components get the rest, and what the
    // section does not use goes back to them (and what they do not use goes back to it)
    let reserve = section_nearby(nearby, max * RESERVE_NEARBY / 100);
    let body = components(legend, items, remaining, total, max - reserve.len(), pt);
    let slack = max.saturating_sub(body.len());
    let nearby_final = if slack > reserve.len() { section_nearby(nearby, slack) } else { reserve };
    body + &nearby_final
}

fn components(legend: &str, items: &[Item], remaining: &[String], total: usize, max: usize, pt: bool) -> String {
    // from the most detailed to the leanest: the first output that fits, with the line for the
    // rest (the line for the rest has to list at least 8 names, or all of them)
    let attempts = [(5, 0), (4, 0), (3, 0), (5, 1), (4, 1), (3, 1), (2, 1), (4, 2), (3, 2), (2, 2), (3, 3), (2, 3), (1, 3)];
    let mut last = String::new();
    for (k, r) in attempts {
        let k = k.min(items.len());
        let extra: Vec<String> = items.iter().skip(k).map(|i| i.name.clone()).chain(remaining.iter().cloned()).collect();
        let body = render(legend, items, total, k, r, pt);
        if body.len() <= max {
            if let Some((more, n)) = line_more(&extra, max - body.len()) {
                if n >= extra.len().min(8) || (k, r) == (1, 3) {
                    return body + &more;
                }
            }
        }
        last = body;
    }
    // not even the leanest fits: cut at the limit, on a character boundary
    let mut end = max.saturating_sub(4).min(last.len());
    while !last.is_char_boundary(end) {
        end -= 1;
    }
    let mut o = last[..end].trim_end().to_string();
    o.push_str("…\n");
    o
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ws(t: &str) -> BTreeSet<String> {
        word_set(t)
    }

    #[test]
    fn words_without_accents_or_case_with_camel_and_snake() {
        assert_eq!(ws("Recording of SlotDetection in requires_mtls"), ["detection", "in", "of", "mtls", "recording", "requires", "slot"].map(String::from).into());
        assert_eq!(ws("crates/nvr-server/src/{api,tls}.rs"), ["api", "crates", "nvr", "rs", "server", "src", "tls"].map(String::from).into());
    }

    #[test]
    fn matches_by_word_prefix_and_simple_plural() {
        let w = ws("route of management, routes");
        assert!(matches_words("route", &w) && matches_words("routes", &w) && matches_words("manage", &w) && matches_words("management", &w));
        assert!(!matches_words("manager", &w) && !matches_words("ot", &w));
    }

    #[test]
    fn truncate_respects_the_limit_and_accents() {
        assert_eq!(truncate_line("sugar and café", 20), "sugar and café");
        assert_eq!(truncate_line("sugar and café with milk", 10), "sugar and…");
    }
}
