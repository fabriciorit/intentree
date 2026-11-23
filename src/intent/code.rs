//! `.intent` commands that look at the project code: the file part of `check`, `show`, `stale`,
//! `facts` and `coverage`. The facts extracted from the code come from `facts.rs`; here they are
//! checked against the sentences.

use super::analysis::Analysis;
use super::project::{Coverage, Diag, Project};
use super::Source;
use crate::facts::{self, pair_covered, program_covered, Agg, Kind};
use crate::measure::{self, lines_of_map, prefix2, has_word, Missing};
use crate::ctx::Ctx;
use crate::vfs::Fs;
use crate::{errln, err, out, stale};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};

/// Path of `f` relative to the root, with `/`, without `./`.
fn rel_of_root(root: &Path, f: &str) -> String {
    let p = Path::new(f);
    let p = if p.is_absolute() { p.strip_prefix(root).unwrap_or(p).to_path_buf() } else { p.to_path_buf() };
    let v: Vec<String> = p.components().filter(|c| !matches!(c, Component::CurDir)).map(|c| c.as_os_str().to_string_lossy().to_string()).collect();
    v.join("/")
}

/// Project files (respecting `.gitignore`), without the `.intent` ones.
fn list_files(fs: &dyn Fs, root: &Path, intents: &[String]) -> Vec<String> {
    let skip: Vec<String> = intents.iter().map(|f| rel_of_root(root, f)).collect();
    fs.project_files().into_iter().filter(|f| !skip.contains(f)).collect()
}

fn root_canonical(fs: &dyn Fs) -> PathBuf {
    fs.canonicalize(fs.root())
}

/// What `check` adds when there are globs, exclusions or anchors.
#[derive(Default)]
pub struct Extras {
    pub lines: Vec<String>,
    pub errors: usize,
    pub warnings: usize,
    /// `files C/M covered (P%)`, only when there are components or exclusions
    pub coverage: Option<String>,
}

pub fn check(cx: &Ctx, f: &Source, a: &Analysis) -> Extras {
    let p = Project::new(a);
    if !p.anchored() {
        return Extras::default();
    }
    let root = root_canonical(cx.fs);
    let cov = p.coverage(list_files(cx.fs, &root, &f.files));
    let mut ds: Vec<Diag> = p.diagnostics(&cov, cx.fs, &root);
    if p.has_files() && a.errors.is_empty() {
        let aggs = facts::aggregate(&p, &facts::collect(&p, &cov.files, cx.fs, &root));
        for (c, msg) in facts::warnings_c(&p, &aggs) {
            let label = p.globs[c].first().map(|g| g.label.clone()).unwrap_or_default();
            ds.push(Diag { error: false, label, msg: msg.replacen("fact without relation:", "fact without sentence:", 1) });
        }
    }
    let errors = ds.iter().filter(|d| d.error).count();
    let coverage = p.has_files().then(|| format!("files {}/{} covered ({}%)", cov.covered(), cov.files.len(), cov.pct()));
    Extras { lines: ds.iter().map(Diag::line).collect(), errors, warnings: ds.len() - errors, coverage }
}

/// File given on the command line, relative to the project root; `None` if outside it.
pub fn rel_in_root(fs: &dyn Fs, root: &Path, arg: &str) -> Option<String> {
    let p = Path::new(arg);
    let abs = if p.is_absolute() { p.to_path_buf() } else { root.join(p) };
    let mut norm = PathBuf::new();
    for c in abs.components() {
        match c {
            Component::ParentDir => {
                norm.pop();
            }
            Component::CurDir => {}
            o => norm.push(o),
        }
    }
    let norm = fs.canonicalize(&norm);
    let r = norm.strip_prefix(root).ok()?;
    Some(r.components().map(|x| x.as_os_str().to_string_lossy().to_string()).collect::<Vec<_>>().join("/"))
}

pub fn show(cx: &Ctx, a: &Analysis, arg: &str, pt: bool) -> i32 {
    let root = root_canonical(cx.fs);
    let Some(rel) = rel_in_root(cx.fs, &root, arg) else {
        errln!(cx, "file outside the project root: {arg}");
        return 2;
    };
    let (o, code) = Project::new(a).show(&rel, pt);
    out!(cx, "{o}");
    code
}

pub fn stale(cx: &Ctx, f: &Source, a: &Analysis) -> i32 {
    if cx.fs.disk_root().is_none() {
        err!(cx, "{}", stale::MSG_NOT_ON_DISK);
        return 2;
    }
    let root = root_canonical(cx.fs);
    if !stale::has_git(&root) {
        err!(cx, "{}", stale::MSG_NO_GIT);
        return 2;
    }
    // the description changed when the most recent .intent changed
    let Some(ts) = f.files.iter().filter_map(|n| stale::instant_of_map(&root.join(n))).max() else {
        errln!(cx, "could not read the date of the .intent files");
        return 2;
    };
    let Some(alt) = stale::changed(&root, ts) else {
        err!(cx, "{}", stale::MSG_NO_GIT);
        return 2;
    };
    let intents: Vec<String> = f.files.iter().map(|n| rel_of_root(&root, n)).collect();
    let p = Project::new(a);
    let rules = p.sentences_review(&alt, &intents, &stale::truncate_line);
    let mut grouped: BTreeMap<usize, Vec<String>> = BTreeMap::new();
    let mut loose = Vec::new();
    for file in alt {
        if intents.contains(&file) {
            continue;
        }
        match p.owner_and_tie(&file).0 {
            Some(c) => grouped.entry(c).or_default().push(file),
            None if p.excluded_by(&file).is_some() => {}
            None => loose.push(file),
        }
    }
    let groups = grouped.into_iter().map(|(c, fs)| (p.name(c).to_string(), fs)).collect();
    out!(cx, "{}", stale::format_output(groups, loose, rules));
    0
}

/// Ready-to-paste sentence covering the aggregated fact, if it has no sentence yet.
fn suggestion(p: &Project, agg: &Agg, lines: &[&str]) -> Option<String> {
    let from = agg.from?;
    match agg.kind {
        Kind::Dep | Kind::Embed | Kind::Http => {
            let to = agg.to?;
            if from == to || p.inside(from, to) || p.inside(to, from) || pair_covered(p, from, to) {
                return None;
            }
            Some(p.suggestion_relation(from, if agg.kind == Kind::Http { "calls" } else { "uses" }, to))
        }
        Kind::Invoke => (!program_covered(p, &agg.target)).then(|| p.suggestion_external(from, "launches", "program", &agg.target)),
        Kind::Env => (!lines.iter().any(|l| has_word(l, &agg.target))).then(|| p.suggestion_external(from, "reads", "environment variable", &agg.target)),
        Kind::Device => {
            let pre = prefix2(&agg.target);
            (!lines.iter().any(|l| l.contains(agg.target.as_str()) || l.contains(&pre))).then(|| p.suggestion_external(from, "reads", "device", &agg.target))
        }
        Kind::Write => {
            let is_linked = |x: usize| p.inside(x, from) || p.inside(from, x);
            let rel = p.edges().iter().any(|&(x, y)| (is_linked(x) && p.datum(y)) || (is_linked(y) && p.datum(x)));
            (!rel).then(|| p.suggestion_external(from, "writes", "file", "<file>"))
        }
        Kind::Route => None,
    }
}

fn facts_and_project<'f>(fs: &dyn Fs, f: &Source, p: &Project<'f>) -> (Vec<Agg>, PathBuf) {
    let root = root_canonical(fs);
    let files = list_files(fs, &root, &f.files);
    let cov: Coverage = p.coverage(files);
    (facts::aggregate(p, &facts::collect(p, &cov.files, fs, &root)), root)
}

pub fn facts(cx: &Ctx, f: &Source, a: &Analysis, json: bool) -> i32 {
    let p = Project::new(a);
    let (aggs, _) = facts_and_project(cx.fs, f, &p);
    let lines = lines_of_map(&f.text);
    let mut o = String::new();
    if json {
        o.push('[');
    }
    for (i, agg) in aggs.iter().enumerate() {
        let sug = suggestion(&p, agg, &lines);
        if json {
            let um = facts::json(&p, std::slice::from_ref(agg));
            let um = um.trim_end().trim_start_matches('[').trim_end_matches(']').trim_end_matches('}');
            if i > 0 {
                o.push(',');
            }
            let s = sug.as_deref().map(super::json::text).unwrap_or_else(|| "null".to_string());
            o.push_str(&format!("{um},\"suggestion\":{s}}}"));
        } else {
            o.push_str(&facts::text(&p, std::slice::from_ref(agg)));
            if let Some(s) = sug {
                o.push_str(&format!("  no sentence: {s}\n"));
            }
        }
    }
    if json {
        o.push_str("]\n");
    }
    out!(cx, "{o}");
    0
}

pub fn coverage(cx: &Ctx, f: &Source, a: &Analysis, json: bool) -> i32 {
    let p = Project::new(a);
    let (aggs, _) = facts_and_project(cx.fs, f, &p);
    let r = measure::measures(&p, &aggs, &f.text);
    let one = |from: &str, verb: &str, to: &str| -> Option<String> {
        Some(p.suggestion_relation(p.by_name(from)?, verb, p.by_name(to)?))
    };
    let mut suggestions: Vec<String> = Vec::new();
    for missing in &r.missing {
        let s = match missing {
            Missing::Dep { from, to, .. } => one(from, "uses", to),
            Missing::Program { name, comps, .. } => comps.first().and_then(|c| p.by_name(c)).map(|c| p.suggestion_external(c, "launches", "program", name)),
            Missing::Env { name, comps, .. } => comps.first().and_then(|c| p.by_name(c)).map(|c| p.suggestion_external(c, "reads", "environment variable", name)),
            Missing::Device { path, comps, .. } => comps.first().and_then(|c| p.by_name(c)).map(|c| p.suggestion_external(c, "reads", "device", path)),
        };
        suggestions.extend(s);
    }
    let dedup: BTreeSet<String> = suggestions.iter().cloned().collect();
    let mut o = if json { measure::json(&r) } else { measure::text(&r) };
    if json {
        let list: Vec<String> = dedup.iter().map(|s| super::json::text(s)).collect();
        o = format!("{},\"suggestions\":[{}]}}\n", o.trim_end().trim_end_matches('}'), list.join(","));
    } else if !dedup.is_empty() {
        o.push_str("suggested sentences\n");
        for s in &dedup {
            o.push_str(&format!("  {s}\n"));
        }
    }
    out!(cx, "{o}");
    0
}
