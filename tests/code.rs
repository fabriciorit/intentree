//! The `.intent` anchored in code (globs, exclusions, anchors) through the command line:
//! check, show, stale, facts and coverage.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn project(name: &str, file_pairs: &[(&str, &str)]) -> PathBuf {
    let d = std::env::temp_dir().join(format!("intentree-code-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    for (p, c) in file_pairs {
        let f = d.join(p);
        fs::create_dir_all(f.parent().unwrap()).unwrap();
        fs::write(f, c).unwrap();
    }
    d
}

fn exec(d: &Path, args: &[&str]) -> (i32, String, String) {
    let mut a = vec!["-C", d.to_str().unwrap()];
    a.extend_from_slice(args);
    let o = Command::new(env!("CARGO_BIN_EXE_intentree")).args(a).output().unwrap();
    (o.status.code().unwrap(), String::from_utf8_lossy(&o.stdout).to_string(), String::from_utf8_lossy(&o.stderr).to_string())
}

fn git(d: &Path, args: &[&str], data: Option<&str>) -> bool {
    let mut c = Command::new("git");
    c.current_dir(d).args(["-c", "user.name=t", "-c", "user.email=t@t"]).args(args);
    if let Some(x) = data {
        c.env("GIT_COMMITTER_DATE", x).env("GIT_AUTHOR_DATE", x);
    }
    c.output().is_ok_and(|o| o.status.success())
}

/// Toy code: a -> b -> c, with an environment variable, a device and an external program.
const CODE: [(&str, &str); 6] = [
    ("a/x.ts", "import '../b/y.ts';\nexport function f() {}\nconst k = process.env.NVR_A;\nconst d = '/dev/dri/renderD128';\n"),
    ("b/y.ts", "import '../c/z.ts';\nexport function g() {}\n"),
    ("c/z.ts", "export const z = 1;\n"),
    ("c/install", "#!/bin/bash\nssh host\n"),
    ("docs/readme.md", "text\n"),
    ("loose.txt", "s\n"),
];

fn with_intent(name: &str, intent: &str) -> PathBuf {
    let mut v: Vec<(&str, &str)> = CODE.to_vec();
    v.push(("system.intent", intent));
    project(name, &v)
}

const BASE: &str = "\
The program \"app\" contains the module \"a\", the module \"b\" and the module \"c\".
The module \"a\" is implemented in \"a/**\".
The module \"b\" is implemented in \"b/**\".
The module \"c\" is implemented in \"c/**\".
The program \"app\" excludes \"docs/**\" and \"loose.txt\".
The module \"a\" uses the module \"b\".
The module \"b\" uses the module \"c\".
";

#[test]
fn layers_in_check_and_show() {
    let layers = "The layer \"ui\" is above the layer \"base\".\nThe module \"a\" is in the layer \"ui\".\nThe module \"b\" is in the layer \"base\".\nThe module \"c\" is in the layer \"base\".\n";
    let d = with_intent("layers", &format!("{BASE}{layers}"));
    let (code, o, _) = exec(&d, &["check"]);
    assert_eq!(code, 0, "{o}");
    let (_, o, _) = exec(&d, &["show", "a/x.ts"]);
    assert!(o.contains("\nlayer \"ui\"\n"), "{o}");
    let _ = fs::remove_dir_all(&d);
    // a relation that goes up: an error with the line; the declared exception passes
    let d = with_intent("layers-up", &format!("{BASE}{layers}The module \"c\" calls the module \"a\".\n"));
    let (code, o, _) = exec(&d, &["check"]);
    assert_eq!(code, 1, "{o}");
    assert!(o.contains("error system.intent:12: relation goes up a layer: 'c' (layer 'base') a.calls 'a' (layer 'ui')"), "{o}");
    let _ = fs::remove_dir_all(&d);
    let d = with_intent("layers-exc", &format!("{BASE}{layers}The module \"c\" exceptionally calls the module \"a\".\n"));
    let (code, o, _) = exec(&d, &["check"]);
    assert_eq!(code, 0, "{o}");
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn check_full_file_coverage() {
    let d = with_intent("covers", &format!("{BASE}The module \"a\" reads the value \"NVR_A\".\nThe module \"a\" reads the device \"/dev/dri\".\nThe module \"c\" launches the program \"ssh\".\n"));
    let (code, o, e) = exec(&d, &["check"]);
    assert_eq!(code, 0, "{o}{e}");
    assert!(o.contains("files 6/6 covered (100%)\n"), "{o}");
    assert!(o.ends_with("0 errors, 7 entities, 13 facts, 0 invariants, 0 rules, 0 gaps, 0 warnings\n"), "{o}");
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn check_loose_file_empty_glob_and_tie() {
    let intent = format!(
        "{BASE}The module \"zz\" is implemented in \"nothing/**\".\nThe module \"bx\" is implemented in \"b/y.ts\".\nThe program \"app\" excludes \"ghost.md\".\nThe module \"a\" reads the value \"NVR_A\".\nThe module \"a\" reads the device \"/dev/dri\".\nThe module \"c\" launches the program \"ssh\".\n"
    );
    let d = with_intent("problems", &intent);
    fs::write(d.join("new.py"), "x = 1\n").unwrap();
    let (code, o, _) = exec(&d, &["check"]);
    assert_eq!(code, 1, "{o}");
    assert!(o.contains("error system.intent:8: the glob of \"zz\" matches no file: nothing/**\n"), "{o}");
    assert!(o.contains("error system.intent:10: the exclusion of \"app\" matches no file: ghost.md\n"), "{o}");
    // b/y.ts belongs to "b" and "bx", which do not contain each other: a warning, and it goes to the first by name
    assert!(
        o.contains("warning system.intent:3: components with no containment relation match the same file (1 files: b/y.ts); it goes to \"b\", tied: \"b\", \"bx\"\n"),
        "{o}"
    );
    // new.py has no owner or exclusion: 1 of 7, coverage of 6/7
    assert!(o.contains("warning 1 of 7 files with no component and no exclusion (coverage 86%): new.py\n"), "{o}");
    assert!(o.contains("files 6/7 covered (86%)\n"), "{o}");
    assert!(o.ends_with("2 errors, 9 entities, 16 facts, 0 invariants, 0 rules, 0 gaps, 2 warnings\n"), "{o}");
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn containment_resolves_the_file_without_a_warning() {
    // "b.y" is contained in "b": the file goes to the most contained one, with no tie
    let intent = format!("{BASE}The module \"b\" contains the module \"b.y\".\nThe module \"b.y\" is implemented in \"b/y.ts\".\n");
    let d = with_intent("contains", &intent);
    let (_, o, _) = exec(&d, &["check"]);
    assert!(!o.contains("no containment relation"), "{o}");
    let (code, o, _) = exec(&d, &["show", "b/y.ts"]);
    assert_eq!(code, 0);
    assert!(o.starts_with("component \"b.y\" (module) system.intent:9\nfiles b/y.ts\ninside \"b\" (module)\ninside \"app\" (program)\n"), "{o}");
    // a file that the glob of "b" matches but "b.y" does not goes to "b"
    let (_, o, _) = exec(&d, &["show", "b/other.ts"]);
    assert!(o.starts_with("component \"b\" (module) system.intent:3\n"), "{o}");
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn check_anchors_error_and_warning() {
    let intent = format!(
        "{BASE}\
The module \"a\" reads the value \"NVR_A\".\nThe module \"a\" reads the device \"/dev/dri\".\nThe module \"c\" launches the program \"ssh\".\n\
The module \"a\" is deterministic. ^a/x.ts#f\n\
The module \"a\" is idempotent. ^a/x.ts#does_not_exist\n\
The module \"a\" is immutable. ^zz/q.ts\n\
The module \"a\" is mutable. ^b/y.ts#g\n\
The module \"b\" is stateful. ^a/x.ts#f ^b/y.ts\n"
    );
    let d = with_intent("anchors", &intent);
    let (code, o, _) = exec(&d, &["check"]);
    assert_eq!(code, 1, "{o}");
    assert!(!o.contains("system.intent:11"), "a valid anchor produces nothing: {o}");
    assert!(o.contains("error system.intent:12: anchor: symbol does_not_exist does not exist in a/x.ts\n"), "{o}");
    assert!(o.contains("error system.intent:13: anchor matches no file: zz/q.ts\n"), "{o}");
    assert!(
        o.contains("warning system.intent:14: anchor outside the component: b/y.ts belongs to \"b\"; the sentence would fit in \"b\"\n"),
        "{o}"
    );
    // one anchor inside the component is enough
    assert!(!o.contains("system.intent:15"), "{o}");
    // the IR carries the field only where there is an anchor
    let (_, ir, _) = exec(&d, &["ir"]);
    assert_eq!(ir.matches("\"anchors\"").count(), 5, "{ir}");
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn show_component_sentences_invariants_and_gaps() {
    let intent = format!(
        "{BASE}\
The module \"a\" reads the value \"NVR_A\".\nThe module \"a\" reads the device \"/dev/dri\".\nThe module \"c\" launches the program \"ssh\".\n\
The module \"a\" contains the http endpoint \"GET /x\".\n\
If the http endpoint \"GET /x\" times out, the module \"a\" returns the value \"504\". ^a/x.ts#f\n\
The module \"c\" never calls the module \"a\".\n\
The module \"c\" is deterministic. ^a/x.ts\n"
    );
    let d = with_intent("show", &intent);
    let (code, o, _) = exec(&d, &["show", "a/x.ts"]);
    assert_eq!(code, 0, "{o}");
    let expected = "\
component \"a\" (module) system.intent:2
files a/**
inside \"app\" (program)
contains \"GET /x\" (http endpoint)
sentence The module \"a\" is implemented in \"a/**\".
sentence The module \"a\" reads the device \"/dev/dri\".
sentence The module \"a\" reads the value \"NVR_A\".
sentence The module \"a\" uses the module \"b\".
sentence The module \"c\" is deterministic. ^a/x.ts
invariant If the http endpoint \"GET /x\" times out, the module \"a\" returns the value \"504\". ^a/x.ts#f
invariant The module \"c\" does not call the module \"a\".
gap address of \"GET /x\" (http endpoint)
gap authentication of \"GET /x\" (http endpoint)
";
    assert_eq!(o, expected);
    // in Portuguese
    let (_, pt, _) = exec(&d, &["show", "--pt", "a/x.ts"]);
    assert!(pt.contains("sentence O módulo “a” lê o valor “NVR_A”.\n"), "{pt}");
    assert!(pt.contains("invariant O módulo “c” não chama o módulo “a”.\n"), "{pt}");
    // a file with no component and a sentence anchored in it
    let d2 = with_intent("show2", &format!("{BASE}The module \"c\" is deterministic. ^extra.txt\n"));
    fs::write(d2.join("extra.txt"), "e\n").unwrap();
    let (code, o, _) = exec(&d2, &["show", "extra.txt"]);
    assert_eq!(code, 1);
    assert_eq!(o, "extra.txt: file with no component\nsentence The module \"c\" is deterministic. ^extra.txt\n");
    // excluded: code 0, says why
    let (code, o, _) = exec(&d, &["show", "docs/readme.md"]);
    assert_eq!((code, o.as_str()), (0, "docs/readme.md: excluded by \"docs/**\" of \"app\" (system.intent:5)\n"));
    // outside the root
    let (code, _, e) = exec(&d, &["show", "/etc/passwd"]);
    assert_eq!(code, 2);
    assert!(e.contains("outside the project root"), "{e}");
    let (code, _, e) = exec(&d, &["show"]);
    assert_eq!(code, 2);
    assert!(e.contains("show requires a file"), "{e}");
    let _ = fs::remove_dir_all(&d);
    let _ = fs::remove_dir_all(&d2);
}

#[test]
fn show_of_a_group_file_lists_children_and_what_crosses() {
    let d = with_intent("group", &format!("{BASE}The module \"a\" is deterministic. ^a/x.ts\n"));
    // c/install belongs to "c"; the b -> c relation mentions "c" directly
    let (_, o, _) = exec(&d, &["show", "c/install"]);
    assert!(o.contains("sentence The module \"b\" uses the module \"c\".\n"), "{o}");
    assert!(!o.contains("The module \"a\" uses the module \"b\"."), "{o}");
    assert!(!o.contains("contains the module \"c\""), "the hierarchy goes in `inside`: {o}");
    let (_, o, _) = exec(&d, &["show", "loose.txt"]);
    assert!(o.starts_with("loose.txt: excluded by \"loose.txt\""), "{o}");
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn stale_with_a_temporary_git() {
    let intent = format!("{BASE}The module \"a\" is deterministic. ^a/x.ts#f\nThe module \"b\" is stateful. ^b/y.ts#g\n");
    let d = with_intent("stale", &intent);
    if !git(&d, &["init", "-q"], None) {
        return; // no git
    }
    assert!(git(&d, &["add", "-A"], None));
    assert!(git(&d, &["commit", "-q", "-m", "base"], Some("2020-01-01T00:00:00")));
    // no change after the description: nothing to say
    let (code, o, _) = exec(&d, &["stale"]);
    assert_eq!((code, o.as_str()), (0, ""));
    // later commit: touches b/y.ts and c/z.ts
    fs::write(d.join("b/y.ts"), "import '../c/z.ts';\nexport function g() { 1; }\n").unwrap();
    fs::write(d.join("c/z.ts"), "export const z = 2;\n").unwrap();
    assert!(git(&d, &["commit", "-q", "-am", "touches b and c"], Some("2021-01-01T00:00:00")));
    // uncommitted: a/x.ts modified, a new file with no owner and a touched excluded file
    fs::write(d.join("a/x.ts"), "import '../b/y.ts';\nexport function f() { 1; }\n").unwrap();
    fs::write(d.join("new.py"), "x = 1\n").unwrap();
    fs::write(d.join("docs/readme.md"), "other\n").unwrap();
    let (code, o, _) = exec(&d, &["stale"]);
    assert_eq!(code, 0, "{o}");
    let expected = "\
a 1 e.g. a/x.ts
b 1 e.g. b/y.ts
c 1 e.g. c/z.ts
no component new.py
sentences to review
a | The module \"a\" is deterministic. | a/x.ts
b | The module \"b\" is stateful. | b/y.ts
";
    assert_eq!(o, expected);
    // recommitting the .intent resets what had been committed before; the uncommitted stays
    fs::write(d.join("system.intent"), format!("{intent}# reviewed\n")).unwrap();
    assert!(git(&d, &["add", "system.intent"], None));
    assert!(git(&d, &["commit", "-q", "-m", "intent reviewed"], Some("2022-01-01T00:00:00")));
    let (_, o, _) = exec(&d, &["stale"]);
    assert_eq!(o, "a 1 e.g. a/x.ts\nno component new.py\nsentences to review\na | The module \"a\" is deterministic. | a/x.ts\n");
    let _ = fs::remove_dir_all(&d);
    // no git: exits with 2
    let v = with_intent("stale-no-git", BASE);
    let (code, _, e) = exec(&v, &["stale"]);
    if code == 2 {
        assert!(e.contains("git"), "{e}");
    }
    let _ = fs::remove_dir_all(&v);
}

#[test]
fn facts_suggest_the_sentence_and_coverage_measures() {
    // a -> b exists in the description; b -> c, the program ssh, NVR_A and /dev/dri do not
    let intent = "\
The module \"a\" is implemented in \"a/**\".
The module \"b\" is implemented in \"b/**\".
The module \"c\" is implemented in \"c/**\".
The program \"app\" excludes \"docs/**\" and \"loose.txt\".
The module \"a\" uses the module \"b\".
";
    let d = with_intent("facts", intent);
    let (code, o, e) = exec(&d, &["facts"]);
    assert_eq!(code, 0, "{e}");
    let expected = "\
dep a -> b (1) e.g. a/x.ts:1
dep b -> c (1) e.g. b/y.ts:1
  no sentence: The module \"b\" uses the module \"c\".
invoke c ssh (1) e.g. c/install:2
  no sentence: The module \"c\" launches the program \"ssh\".
env a NVR_A e.g. a/x.ts:3
  no sentence: The module \"a\" reads the environment variable \"NVR_A\".
device a /dev/dri/renderD128 e.g. a/x.ts:4
  no sentence: The module \"a\" reads the device \"/dev/dri/renderD128\".
";
    assert_eq!(o, expected);
    let (_, j, _) = exec(&d, &["facts", "--format", "json"]);
    assert!(j.starts_with("[{\"kind\":\"dep\",\"from\":\"a\",\"to\":\"b\",\"target\":null,\"n\":1,\"examples\":[\"a/x.ts:1\"],\"suggestion\":null},"), "{j}");
    assert!(j.contains("\"suggestion\":\"The module \\\"b\\\" uses the module \\\"c\\\".\"}"), "{j}");
    assert!(j.ends_with("}]\n"), "{j}");

    let (code, o, _) = exec(&d, &["coverage"]);
    assert_eq!(code, 0);
    let expected = "\
dependencies 1/2 50%
programs 0/1 0%
environment 0/1 0%
devices 0/1 0%
total 1/5 20%
missing
  dep b -> c e.g. b/y.ts:1
  program ssh (c) e.g. c/install:2
  env NVR_A (a) e.g. a/x.ts:3
  device /dev/dri/renderD128 (a) e.g. a/x.ts:4
suggested sentences
  The module \"a\" reads the device \"/dev/dri/renderD128\".
  The module \"a\" reads the environment variable \"NVR_A\".
  The module \"b\" uses the module \"c\".
  The module \"c\" launches the program \"ssh\".
";
    assert_eq!(o, expected);
    let (_, j, _) = exec(&d, &["coverage", "--format", "json"]);
    assert!(j.contains("\"total\":{\"covered\":1,\"total\":5,\"pct\":20}") && j.contains("\"suggestions\":[\"The module \\\"a\\\" reads the device"), "{j}");
    assert!(j.ends_with("]}\n"), "{j}");
    // check warns of the same, in sentence language
    let (code, o, _) = exec(&d, &["check"]);
    assert_eq!(code, 0, "{o}");
    assert!(o.contains("warning system.intent:2: fact without sentence: b -> c (dep, 1 occurrences, e.g. b/y.ts:1)\n"), "{o}");
    assert!(o.contains("warning system.intent:3: program invoked without an external component: ssh (c, e.g. c/install:2)\n"), "{o}");
    // wrong options
    assert_eq!(exec(&d, &["facts", "--format", "xml"]).0, 2);
    assert_eq!(exec(&d, &["stale", "--x"]).0, 2);
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn facts_covered_by_relations_of_contained_and_ancestors() {
    // a relation between a function of "a" and "b.sub" covers a -> b; "ssh" enters as an external program
    let intent = "\
The module \"a\" is implemented in \"a/**\".
The module \"b\" is implemented in \"b/**\".
The module \"c\" is implemented in \"c/**\".
The program \"app\" excludes \"docs/**\" and \"loose.txt\".
The module \"a\" contains the function \"f\".
The module \"c\" contains the module \"c.sub\".
The function \"f\" calls the module \"b\".
The module \"b\" uses the module \"c.sub\".
The module \"c\" launches the program \"ssh\".
The module \"a\" reads the value \"NVR_A\".
The module \"a\" reads the device \"/dev/dri\".
";
    let d = with_intent("covered", intent);
    let (_, o, _) = exec(&d, &["coverage"]);
    assert!(o.starts_with("dependencies 2/2 100%\nprograms 1/1 100%\nenvironment 1/1 100%\ndevices 1/1 100%\ntotal 5/5 100%\n"), "{o}");
    assert!(!o.contains("missing"), "{o}");
    let (_, o, _) = exec(&d, &["facts"]);
    assert!(!o.contains("no sentence"), "{o}");
    let _ = fs::remove_dir_all(&d);
}

// ---------------------------------------------------------------- examples

fn examples() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples")
}

fn first_line(o: &str) -> String {
    o.lines().next().unwrap_or("").to_string()
}

/// Name of the component on the first line of `show`, or EXCLUDED / NONE.
fn owner_in_show(o: &str) -> String {
    let l = first_line(o);
    if l.contains(": excluded by") {
        return format!("{}: EXCLUDED", l.split(':').next().unwrap());
    }
    if l.contains(": file with no component") {
        return format!("{}: NONE", l.split(':').next().unwrap());
    }
    let r = l.strip_prefix("component ").unwrap();
    let r = r.strip_prefix('"').map(|x| x.split('"').next().unwrap()).unwrap_or_else(|| r.split(' ').next().unwrap());
    r.to_string()
}

#[test]
fn examples_close_total_coverage_and_every_file_has_an_owner() {
    for name in ["shortener", "minimake", "elevator"] {
        let dir = examples().join(name);
        let (code, new_check, e) = exec(&dir, &["check"]);
        // show prints the layer of the component
        let (_, o, _) = exec(&dir, &["show", &format!("{name}/cli.py")]);
        if name != "shortener" {
            assert!(o.contains("\nlayer \"interface\"\n"), "{name}: {o}");
        }
        assert_eq!(code, 0, "{name}: {new_check}{e}");
        assert!(new_check.contains("covered (100%)") && new_check.contains(", 0 warnings\n"), "{name}: {new_check}");
        // the code reveals what the .intent says
        let (_, new, _) = exec(&dir, &["coverage"]);
        assert!(new.contains("total "), "{name}: coverage {new}");
        // every file has an owner (or is excluded)
        let list = Command::new("git").arg("-C").arg(&dir).args(["ls-files", "-co", "--exclude-standard"]).output();
        let Ok(list) = list.map(|o| String::from_utf8_lossy(&o.stdout).to_string()) else { continue };
        let files: Vec<&str> = list.lines().filter(|f| *f != "system.intent").collect();
        assert!(files.len() > 10, "{name}: {files:?}");
        for f in &files {
            let (_, n, _) = exec(&dir, &["show", f]);
            assert!(!owner_in_show(&n).ends_with(": NONE"), "{name}: {f}: {n}");
        }
    }
}

#[test]
fn show_prints_the_notes_of_the_sentence_as_informal() {
    let intent = format!("{BASE}The module \"a\" reads the value \"k\".\nNote: reads at startup, only once.\n");
    let d = with_intent("show-notes", &intent);
    let (c, o, e) = exec(&d, &["show", "a/x.ts"]);
    assert_eq!(c, 0, "{e}");
    assert!(o.contains("sentence The module \"a\" reads the value \"k\".\nnote (informal) reads at startup, only once.\n"), "{o}");
}
