//! Commands of the `.intent` format: check, show, where, view, views, facts, coverage, stale, ir, canon and lexicon.

use super::analysis::Analysis;
use super::views::{Format, Views};
use super::{files_of_project, canon, code, assemble, locate, propose, Source};
use crate::ctx::Ctx;
use crate::{errln, out, outln};
use std::path::{Path, PathBuf};

pub const USAGE: &str = "usage: intentree [-C root] <command>
  check [-v|--verbose] [--file X.intent]  check sentences, anchors and coverage (0 = no errors)
  show <file> [--pt] [--file X.intent]    what the .intent says about the file
  where <term>... [--file X.intent] [--format text|json] [--max N] [--pt]   where to change: components, files, symbols and the neighbors' rules
  view [<name>] [--format text|svg|json] [--file X.intent]   a view (default: structure)
  views [--format text|json] [--file X.intent]               catalog of views
  facts [--format text|json] [--file X.intent]               what the code reveals
  coverage [--format text|json] [--file X.intent]            how much of it the .intent says
  stale [--file X.intent]                 sentences to review after code changes
  ir [--file X.intent]                    intermediate representation as JSON
  canon [--en|--pt] [--file X.intent]     canonical sentences
  lexicon [list|check|pending] [--file X.intent]
  lexicon propose type|verb|adjective <en form> --meaning \"<text>\" [--pt \"<label>\"] [--evidence \"<file/sentence>\"] [--g m|f] [--id <id>]
The commands read the *.intent files in the root and in intent/ (or only the one given by --file).";

pub fn usage(cx: &Ctx, m: &str) -> i32 {
    errln!(cx, "{m}\n{USAGE}");
    2
}

/// `--file X` in `rest`, if present; the remaining options go to `others`.
fn split_file(cx: &Ctx, rest: &[String]) -> Result<(Option<String>, Vec<String>), i32> {
    let mut file = None;
    let mut others = Vec::new();
    let mut it = rest.iter();
    while let Some(a) = it.next() {
        if a == "--file" {
            match it.next() {
                Some(f) => file = Some(f.clone()),
                None => return Err(usage(cx, "--file requires a file")),
            }
        } else {
            others.push(a.clone());
        }
    }
    Ok((file, others))
}

/// Where `intent/lexicon-pending.intent` lives: the directory of `--file`, or the root.
fn base_pending(root: &Path, file: Option<&str>) -> PathBuf {
    match file {
        Some(f) => match path(root, f).parent() {
            Some(d) if !d.as_os_str().is_empty() => d.to_path_buf(),
            _ => root.to_path_buf(),
        },
        None => root.to_path_buf(),
    }
}

/// With `--file`, the `lexicon propose` command in the hints carries the same `--file`.
fn hint_with_file(line: &str, file: Option<&str>) -> String {
    const END: &str = "--meaning \"<what it means>\"";
    match file {
        Some(f) if line.contains("intentree lexicon propose ") && line.ends_with(END) => {
            let f = if f.contains(' ') { format!("\"{f}\"") } else { f.to_string() };
            format!("{line} --file {f}")
        }
        _ => line.to_string(),
    }
}

fn path(root: &Path, f: &str) -> PathBuf {
    let p = Path::new(f);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        root.join(p)
    }
}

/// Reads the `.intent` files (one file with `--file`, or the project ones) and builds the lexicon and sentences.
fn load(cx: &Ctx, file: Option<&str>, requires: bool) -> Result<Source, i32> {
    load_with(cx, file, requires, 2)
}

/// Like `load`, with the exit code for a lexicon error.
fn load_with(cx: &Ctx, file: Option<&str>, requires: bool, code_lexicon: i32) -> Result<Source, i32> {
    let root = cx.fs.root();
    let (texts, single_file) = match file {
        Some(f) => {
            let p = path(root, f);
            match cx.fs.read_to_string(&p) {
                Ok(t) => (vec![(f.to_string(), t)], true),
                Err(e) => {
                    errln!(cx, "{f}: error reading: {e}");
                    return Err(2);
                }
            }
        }
        None => {
            let file_pairs = files_of_project(cx.fs);
            if file_pairs.is_empty() && requires {
                errln!(cx, "no .intent file in {} (nor in intent/)", root.display());
                return Err(2);
            }
            let mut v = Vec::new();
            for (rel, p) in file_pairs {
                match cx.fs.read_to_string(&p) {
                    Ok(t) => v.push((rel, t)),
                    Err(e) => {
                        errln!(cx, "{rel}: error reading: {e}");
                        return Err(2);
                    }
                }
            }
            (v, false)
        }
    };
    assemble(&texts, single_file).map_err(|errors| {
        for e in errors {
            errln!(cx, "error {e}");
        }
        code_lexicon
    })
}

/// What `check --verbose` adds: the invariants and rules as the parser read them, one per
/// line, with the originating sentence (file:line).
fn invariants_verbose(a: &Analysis) -> String {
    let mut o = String::from("a sentence with several consequences becomes several invariants (normal form); a fact with never or does not counts as an invariant\n");
    let mut views = Vec::new();
    o.push_str(&format!("invariants ({}):\n", a.invariants.len()));
    for or in &a.origins {
        for i in &or.invariants {
            if !views.contains(&i) {
                views.push(i);
                for l in canon::invariant_en(a, i) {
                    o.push_str(&format!("  {}  {l}\n", or.label));
                }
            }
        }
    }
    o.push_str(&format!("rules ({}):\n", a.rules.len()));
    let mut already = Vec::new();
    for or in &a.origins {
        for r in &or.rules {
            if !already.contains(&r) {
                already.push(r);
                o.push_str(&format!("  {}  {}\n", or.label, canon::rule_en(a, r)));
            }
        }
    }
    o
}

pub fn run(cmd: &str, rest: &[String], cx: &Ctx) -> i32 {
    let root = cx.fs.root();
    let (file, others) = match split_file(cx, rest) {
        Ok(x) => x,
        Err(c) => return c,
    };
    match cmd {
        "ir" => {
            if let Some(x) = others.first() {
                return usage(cx, &format!("unknown option: {x}"));
            }
            let f = match load(cx, file.as_deref(), true) {
                Ok(f) => f,
                Err(c) => return c,
            };
            let a = Analysis::new(&f.lines, &f.lex);
            outln!(cx, "{}", a.ir().indented());
            (!a.errors.is_empty()) as i32
        }
        "canon" => {
            let mut pt = false;
            for x in &others {
                match x.as_str() {
                    "--pt" => pt = true,
                    "--en" => pt = false,
                    _ => return usage(cx, &format!("unknown option: {x}")),
                }
            }
            let f = match load(cx, file.as_deref(), true) {
                Ok(f) => f,
                Err(c) => return c,
            };
            let a = Analysis::new(&f.lines, &f.lex);
            out!(cx, "{}", if pt { canon::text_pt(&a) } else { canon::text_en(&a) });
            (!a.errors.is_empty()) as i32
        }
        "check" => {
            let mut verbose = false;
            for x in &others {
                match x.as_str() {
                    "-v" | "--verbose" => verbose = true,
                    _ => return usage(cx, &format!("unknown option: {x}")),
                }
            }
            let f = match load(cx, file.as_deref(), true) {
                Ok(f) => f,
                Err(c) => return c,
            };
            let a = Analysis::new(&f.lines, &f.lex);
            for l in a.lines_of_error() {
                outln!(cx, "{}", hint_with_file(&l, file.as_deref()));
            }
            let extras = code::check(cx, &f, &a);
            for l in &extras.lines {
                outln!(cx, "{l}");
            }
            if let Some(c) = &extras.coverage {
                outln!(cx, "{c}");
            }
            outln!(cx, "{}", a.summary_with(extras.errors, extras.warnings));
            if verbose {
                out!(cx, "{}", invariants_verbose(&a));
            }
            (a.errors.len() + extras.errors > 0) as i32
        }
        "show" => {
            let mut pt = false;
            let mut target = None;
            for x in &others {
                match x.as_str() {
                    "--pt" => pt = true,
                    "--en" => pt = false,
                    o if o.starts_with("--") => return usage(cx, &format!("unknown option: {o}")),
                    _ if target.is_none() => target = Some(x.clone()),
                    _ => return usage(cx, "show takes a single file"),
                }
            }
            let Some(target) = target else { return usage(cx, "show requires a file") };
            let f = match load(cx, file.as_deref(), true) {
                Ok(f) => f,
                Err(c) => return c,
            };
            let a = Analysis::new(&f.lines, &f.lex);
            if !a.errors.is_empty() {
                for l in a.lines_of_error() {
                    outln!(cx, "{l}");
                }
                return 2;
            }
            code::show(cx, &a, &target, pt)
        }
        "where" => {
            let mut op = locate::Options { terms: Vec::new(), max: locate::MAX_DEFAULT, pt: false, json: false };
            let mut it = others.iter();
            while let Some(x) = it.next() {
                match x.as_str() {
                    "--pt" => op.pt = true,
                    "--en" => op.pt = false,
                    "--format" => match it.next().map(String::as_str) {
                        Some("json") => op.json = true,
                        Some("text") => op.json = false,
                        Some(v) => return usage(cx, &format!("unknown format: {v} (use text or json)")),
                        None => return usage(cx, "--format requires a value"),
                    },
                    "--max" => match it.next().and_then(|v| v.parse::<usize>().ok()) {
                        Some(n) if n >= 80 => op.max = n,
                        _ => return usage(cx, "--max requires a number of bytes (at least 80)"),
                    },
                    o if o.starts_with("--") => return usage(cx, &format!("unknown option: {o}")),
                    _ => op.terms.push(x.clone()),
                }
            }
            if op.terms.is_empty() {
                return usage(cx, "where requires at least one term (words about the change, or a file)");
            }
            let f = match load(cx, file.as_deref(), true) {
                Ok(f) => f,
                Err(c) => return c,
            };
            let a = Analysis::new(&f.lines, &f.lex);
            if !a.errors.is_empty() {
                for l in a.lines_of_error() {
                    outln!(cx, "{l}");
                }
                return 2;
            }
            locate::run(cx, &a, &op)
        }
        "stale" | "facts" | "coverage" => {
            let mut json = false;
            let mut it = others.iter();
            while let Some(x) = it.next() {
                match (cmd, x.as_str()) {
                    ("facts" | "coverage", "--format") => match it.next().map(String::as_str) {
                        Some("json") => json = true,
                        Some("text") => json = false,
                        Some(v) => return usage(cx, &format!("unknown format: {v}")),
                        None => return usage(cx, "--format requires a value"),
                    },
                    _ => return usage(cx, &format!("unknown option: {x}")),
                }
            }
            let f = match load(cx, file.as_deref(), true) {
                Ok(f) => f,
                Err(c) => return c,
            };
            let a = Analysis::new(&f.lines, &f.lex);
            if !a.errors.is_empty() {
                for l in a.lines_of_error() {
                    outln!(cx, "{l}");
                }
                return 2;
            }
            match cmd {
                "stale" => code::stale(cx, &f, &a),
                "facts" => code::facts(cx, &f, &a, json),
                _ => code::coverage(cx, &f, &a, json),
            }
        }
        "view" | "views" => {
            let mut format_guide = None;
            let mut name = None;
            let mut it = others.iter();
            while let Some(x) = it.next() {
                match x.as_str() {
                    "--format" => match it.next() {
                        Some(v) => match Format::read_text(v) {
                            Some(f) => format_guide = Some(f),
                            None => return usage(cx, &format!("unknown format: {v} (use text, svg or json)")),
                        },
                        None => return usage(cx, "--format requires a value"),
                    },
                    o if o.starts_with("--") => return usage(cx, &format!("unknown option: {o}")),
                    _ if cmd == "view" && name.is_none() => name = Some(x.clone()),
                    _ => return usage(cx, &format!("extra argument: {x}")),
                }
            }
            let f = match load(cx, file.as_deref(), true) {
                Ok(f) => f,
                Err(c) => return c,
            };
            let a = Analysis::new(&f.lines, &f.lex);
            if !a.errors.is_empty() {
                for l in a.lines_of_error() {
                    outln!(cx, "{l}");
                }
                return 2;
            }
            let v = Views::new(&a);
            if cmd == "views" {
                match format_guide {
                    None | Some(Format::Text) => out!(cx, "{}", v.catalog_text()),
                    Some(Format::Json) => outln!(cx, "{}", v.catalog_json().indented()),
                    Some(Format::Svg) => return usage(cx, "views has no svg output"),
                }
                return 0;
            }
            let q = match v.resolve(name.as_deref().unwrap_or("structure")) {
                Ok(q) => q,
                Err(e) => {
                    errln!(cx, "{e}");
                    return 2;
                }
            };
            out!(cx, "{}", v.generate(&q, format_guide.unwrap_or(Format::Text)));
            0
        }
        "lexicon" => {
            if others.first().is_some_and(|s| s == "pending") {
                if let Some(x) = others.get(1) {
                    return usage(cx, &format!("unknown option: {x}"));
                }
                out!(cx, "{}", propose::pending(cx.fs, &base_pending(root, file.as_deref())));
                return 0;
            }
            if others.first().is_some_and(|s| s == "propose") {
                let f = match load_with(cx, file.as_deref(), false, 2) {
                    Ok(f) => f,
                    Err(c) => return c,
                };
                return match propose::propose(cx.fs, &base_pending(root, file.as_deref()), &f.lex, &others[1..]) {
                    Ok(m) => {
                        out!(cx, "{m}");
                        0
                    }
                    Err(e) => usage(cx, &e),
                };
            }
            let sub = match others.as_slice() {
                [] => "list",
                [s] if s == "list" || s == "check" => s.as_str(),
                [x, ..] => return usage(cx, &format!("unknown option: {x}")),
            };
            let f = match load_with(cx, file.as_deref(), false, if sub == "check" { 1 } else { 2 }) {
                Ok(f) => f,
                Err(c) => return c,
            };
            if sub == "list" {
                out!(cx, "{}", f.lex.list_entries());
            } else {
                // the static checks already ran on load; the phrase-overlap check is only here
                // (it changes tokenization, it does not prevent reading)
                let l = &f.lex;
                let overlapping = l.overlaps();
                if !overlapping.is_empty() {
                    for m in &overlapping {
                        outln!(cx, "error {m}");
                    }
                    return 1;
                }
                outln!(cx, 
                    "lexicon {} ok: {} types, {} verbs, {} adjectives, {} comparisons, {} quantities, {} units, {} phrases",
                    l.version,
                    l.kinds.len(),
                    l.verbs.len(),
                    l.adjectives.len(),
                    l.comparisons.len(),
                    l.quantities.len(),
                    l.units.len(),
                    l.sentences.len()
                );
            }
            0
        }
        x => usage(cx, &format!("unknown command: {x}")),
    }
}
