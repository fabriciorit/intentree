//! Coverage of what the code reveals: how much of the dependencies, programs, environment
//! variables and devices the map says. A deterministic measure, with no interpretation.

use crate::facts::{pair_covered, program_covered, programs, Agg, Kind};
use crate::intent::json::text as jstr;
use crate::intent::project::Project;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Part {
    pub covered: usize,
    pub total: usize,
}

impl Part {
    /// Whole percentage, rounded; with nothing to cover it counts as 100.
    pub fn pct(self) -> usize {
        if self.total == 0 {
            100
        } else {
            (self.covered * 100 + self.total / 2) / self.total
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Missing {
    Dep { from: String, to: String, ex: String },
    Program { name: String, comps: Vec<String>, ex: String },
    Env { name: String, comps: Vec<String>, ex: String },
    Device { path: String, comps: Vec<String>, ex: String },
}

#[derive(Debug, Default)]
pub struct Measure {
    pub dep: Part,
    pub prog: Part,
    pub env: Part,
    pub dev: Part,
    pub missing: Vec<Missing>,
}

impl Measure {
    pub fn total(&self) -> Part {
        let ps = [self.dep, self.prog, self.env, self.dev];
        Part { covered: ps.iter().map(|p| p.covered).sum(), total: ps.iter().map(|p| p.total).sum() }
    }
}

/// Lines of the map that are not comments (text, invariants, glossary, dictionary).
pub fn lines_of_map(text: &str) -> Vec<&str> {
    text.lines().filter(|l| !l.trim_start().starts_with('#')).collect()
}

fn ident(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// `name` appears as a whole word (case-sensitive).
pub fn has_word(line: &str, name: &str) -> bool {
    line.match_indices(name).any(|(i, _)| {
        let before = line[..i].chars().next_back().is_none_or(|c| !ident(c));
        let after = line[i + name.len()..].chars().next().is_none_or(|c| !ident(c));
        before && after
    })
}

/// The path up to the second segment (`/dev/dri/renderD128` -> `/dev/dri`).
pub fn prefix2(path: &str) -> String {
    let segs: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    format!("/{}", segs.iter().take(2).copied().collect::<Vec<_>>().join("/"))
}

pub fn measures(m: &Project, aggs: &[Agg], text_of_map: &str) -> Measure {
    let lines = lines_of_map(text_of_map);
    let mut r = Measure::default();

    // dependencies: unique pairs with no ancestor relation between them
    let mut pairs: BTreeMap<(usize, usize), String> = BTreeMap::new();
    for a in aggs.iter().filter(|a| matches!(a.kind, Kind::Dep | Kind::Embed | Kind::Http)) {
        let (Some(x), Some(y)) = (a.from, a.to) else { continue };
        if x == y || m.inside(x, y) || m.inside(y, x) {
            continue;
        }
        pairs.entry((x, y)).or_insert_with(|| a.ex.first().cloned().unwrap_or_default());
    }
    for ((x, y), ex) in pairs {
        r.dep.total += 1;
        if pair_covered(m, x, y) {
            r.dep.covered += 1;
        } else {
            r.missing.push(Missing::Dep { from: m.id(x).to_string(), to: m.id(y).to_string(), ex });
        }
    }

    // programs
    for (name, (comps, ex, _)) in programs(m, aggs) {
        r.prog.total += 1;
        if program_covered(m, name) {
            r.prog.covered += 1;
        } else {
            r.missing.push(Missing::Program { name: name.to_string(), comps: comps.into_iter().collect(), ex });
        }
    }

    // environment and devices: per distinct target, with the components that use it
    let by_target = |kind: Kind| {
        let mut v: BTreeMap<&str, (BTreeSet<String>, String)> = BTreeMap::new();
        for a in aggs.iter().filter(|a| a.kind == kind && a.from.is_some()) {
            let e = v.entry(a.target.as_str()).or_insert_with(|| (BTreeSet::new(), a.ex.first().cloned().unwrap_or_default()));
            e.0.insert(a.from.map(|c| m.id(c).to_string()).unwrap_or_default());
        }
        v
    };
    for (name, (comps, ex)) in by_target(Kind::Env) {
        r.env.total += 1;
        if lines.iter().any(|l| has_word(l, name)) {
            r.env.covered += 1;
        } else {
            r.missing.push(Missing::Env { name: name.to_string(), comps: comps.into_iter().collect(), ex });
        }
    }
    for (path, (comps, ex)) in by_target(Kind::Device) {
        r.dev.total += 1;
        let pre = prefix2(path);
        if lines.iter().any(|l| l.contains(path) || l.contains(&pre)) {
            r.dev.covered += 1;
        } else {
            r.missing.push(Missing::Device { path: path.to_string(), comps: comps.into_iter().collect(), ex });
        }
    }
    r
}

pub fn text(r: &Measure) -> String {
    let mut o = String::new();
    for (n, p) in [("dependencies", r.dep), ("programs", r.prog), ("environment", r.env), ("devices", r.dev), ("total", r.total())] {
        let _ = writeln!(o, "{n} {}/{} {}%", p.covered, p.total, p.pct());
    }
    if !r.missing.is_empty() {
        o.push_str("missing\n");
    }
    for f in &r.missing {
        let _ = match f {
            Missing::Dep { from, to, ex } => writeln!(o, "  dep {from} -> {to} e.g. {ex}"),
            Missing::Program { name, comps, ex } => writeln!(o, "  program {name} ({}) e.g. {ex}", comps.join(",")),
            Missing::Env { name, comps, ex } => writeln!(o, "  env {name} ({}) e.g. {ex}", comps.join(",")),
            Missing::Device { path, comps, ex } => writeln!(o, "  device {path} ({}) e.g. {ex}", comps.join(",")),
        };
    }
    o
}

pub fn json(r: &Measure) -> String {
    let part = |p: Part| format!("{{\"covered\":{},\"total\":{},\"pct\":{}}}", p.covered, p.total, p.pct());
    let list = |c: &[String]| format!("[{}]", c.iter().map(|x| jstr(x)).collect::<Vec<_>>().join(","));
    let missing: Vec<String> = r
        .missing
        .iter()
        .map(|f| match f {
            Missing::Dep { from, to, ex } => format!("{{\"kind\":\"dep\",\"from\":{},\"to\":{},\"example\":{}}}", jstr(from), jstr(to), jstr(ex)),
            Missing::Program { name, comps, ex } => {
                format!("{{\"kind\":\"program\",\"target\":{},\"components\":{},\"example\":{}}}", jstr(name), list(comps), jstr(ex))
            }
            Missing::Env { name, comps, ex } => format!("{{\"kind\":\"env\",\"target\":{},\"components\":{},\"example\":{}}}", jstr(name), list(comps), jstr(ex)),
            Missing::Device { path, comps, ex } => {
                format!("{{\"kind\":\"device\",\"target\":{},\"components\":{},\"example\":{}}}", jstr(path), list(comps), jstr(ex))
            }
        })
        .collect();
    format!(
        "{{\"dependencies\":{},\"programs\":{},\"environment\":{},\"devices\":{},\"total\":{},\"missing\":[{}]}}\n",
        part(r.dep),
        part(r.prog),
        part(r.env),
        part(r.dev),
        part(r.total()),
        missing.join(",")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::facts::{aggregate, collect_facts};
    use crate::intent::analysis::Analysis;
    use crate::intent::lexicon::Lexicon;

    #[test]
    fn words_and_prefixes() {
        assert!(has_word("usa NVR_A e mais", "NVR_A") && has_word("NVR_A", "NVR_A") && has_word("(NVR_A)", "NVR_A"));
        assert!(!has_word("usa NVR_AB", "NVR_A") && !has_word("usa nvr_a", "NVR_A") && !has_word("X_NVR_A", "NVR_A"));
        assert_eq!(prefix2("/dev/dri/renderD128"), "/dev/dri");
        assert_eq!(prefix2("/sys/class/thermal/x"), "/sys/class");
        assert_eq!(prefix2("/dev/null"), "/dev/null");
        assert_eq!(Part { covered: 41, total: 45 }.pct(), 91);
        assert_eq!(Part { covered: 0, total: 0 }.pct(), 100);
    }

    const MAP: &str = "The module \"a\" is implemented in \"a/**\". ^a/x.ts\n\
Note: uses NVR_A and accesses /dev/dri directly.\n\
The module \"b\" is implemented in \"b/**\".\n\
The module \"c\" is implemented in \"c/**\".\n\
# comment with NOT_DECLARED and /proc/stat\n\
The module \"a\" uses the module \"b\".\n\
The module \"c\" launches the program \"ffmpeg\".\n";

    /// Project from an `.intent` text (without errors) handed to the test.
    fn with_project<R>(text: &str, f: impl FnOnce(&Project) -> R) -> R {
        let lex = Lexicon::embedded().unwrap();
        let a = Analysis::text(text, &lex);
        assert!(a.errors.is_empty(), "{:?}", a.lines_of_error());
        f(&Project::new(&a))
    }

    #[test]
    fn measures_each_category() {
        let file_pairs = [
            ("a/x.ts", "import '../b/y.ts';\nconst k = process.env.NVR_A + process.env.NOT_DECLARED;\nconst d = '/dev/dri/renderD128'; const t = '/sys/class/thermal/t'; const p = '/proc/stat';\n"),
            ("b/y.ts", "import '../c/z.ts';\n"),
            ("c/z.ts", "export const z = 1;\n"),
            ("c/i.sh", "#!/bin/sh\nffmpeg -i x\nssh host\n"),
        ];
        let files: Vec<String> = file_pairs.iter().map(|a| a.0.to_string()).collect();
        let l = |f: &str| file_pairs.iter().find(|a| a.0 == f).map(|a| a.1.to_string());
        with_project(MAP, |m| {
            let aggs = aggregate(m, &collect_facts(m, &files, &l));
            let r = measures(m, &aggs, MAP);
            assert_eq!((r.dep.covered, r.dep.total), (1, 2)); // a->b ok; b->c missing
            assert_eq!((r.prog.covered, r.prog.total), (1, 2)); // ffmpeg ok; ssh missing
            assert_eq!((r.env.covered, r.env.total), (1, 2)); // NVR_A in the text; a comment does not count
            assert_eq!((r.dev.covered, r.dev.total), (1, 3)); // /dev/dri by prefix
            let t = text(&r);
            assert!(t.starts_with("dependencies 1/2 50%\nprograms 1/2 50%\nenvironment 1/2 50%\ndevices 1/3 33%\ntotal 4/9 44%\nmissing\n"), "{t}");
            for s in [
                "  dep b -> c e.g. b/y.ts:1\n",
                "  program ssh (c) e.g. c/i.sh:3\n",
                "  env NOT_DECLARED (a) e.g. a/x.ts:2\n",
                "  device /sys/class/thermal/t (a) e.g. a/x.ts:3\n",
                "  device /proc/stat (a) e.g. a/x.ts:3\n",
            ] {
                assert!(t.contains(s), "missing {s:?} in\n{t}");
            }
            let j = json(&r);
            assert!(j.starts_with("{\"dependencies\":{\"covered\":1,\"total\":2,\"pct\":50}"), "{j}");
            assert!(j.contains("\"total\":{\"covered\":4,\"total\":9,\"pct\":44}"), "{j}");
            assert!(j.contains("{\"kind\":\"dep\",\"from\":\"b\",\"to\":\"c\",\"example\":\"b/y.ts:1\"}"), "{j}");
            assert!(j.contains("{\"kind\":\"program\",\"target\":\"ssh\",\"components\":[\"c\"],\"example\":\"c/i.sh:3\"}"), "{j}");
        });
    }

    #[test]
    fn facts_of_a_file_without_component_do_not_count() {
        with_project("The module \"a\" is implemented in \"a/**\".\n", |m| {
            let l = |_: &str| Some("process.env.X; '/dev/y'; fetch('/z')".to_string());
            let r = measures(m, &aggregate(m, &collect_facts(m, &["fora/q.ts".to_string()], &l)), "");
            assert_eq!(r.total(), Part { covered: 0, total: 0 });
            assert!(text(&r).ends_with("total 0/0 100%\n"));
        });
    }
}
