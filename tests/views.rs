//! The `view` and `views` commands of the `.intent` format, through the command line.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/intent")
}

fn exec(args: &[&str]) -> (i32, String, String) {
    let o = Command::new(env!("CARGO_BIN_EXE_intentree")).args(args).output().unwrap();
    (o.status.code().unwrap(), String::from_utf8_lossy(&o.stdout).to_string(), String::from_utf8_lossy(&o.stderr).to_string())
}

fn project(name: &str, file_pairs: &[(&str, &str)]) -> PathBuf {
    let d = std::env::temp_dir().join(format!("intentree-views-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    for (p, c) in file_pairs {
        fs::write(d.join(p), c).unwrap();
    }
    d
}

#[test]
fn view_without_a_name_is_the_structure_and_matches_the_golden() {
    let d = dir();
    let d = d.to_str().unwrap();
    for c in ["elevator", "wall_panel"] {
        let f = format!("corpora/{c}.intent");
        let (code, o, _) = exec(&["-C", d, "view", "--file", &f]);
        assert_eq!(code, 0);
        assert_eq!(o, fs::read_to_string(dir().join(format!("golden/{c}.view"))).unwrap());
        let (_, o2, _) = exec(&["-C", d, "view", "structure", "--format", "text", "--file", &f]);
        assert_eq!(o, o2);
    }
}

#[test]
fn view_in_the_three_formats_and_names_with_accents() {
    let d = dir();
    let d = d.to_str().unwrap();
    let f = "corpora/wall_panel.intent";
    let (code, o, _) = exec(&["-C", d, "view", "Deployment", "--format", "svg", "--file", f]);
    assert!(code == 0 && o.starts_with("<svg ") && o.contains("machine “server host”"), "{o}");
    let (code, o, _) = exec(&["-C", d, "view", "concurrency", "--format", "json", "--file", f]);
    assert!(code == 0 && o.starts_with("{\n") && o.contains("\"threads\""), "{o}");
    let (code, o, _) = exec(&["-C", d, "view", "machine:connéction", "--file", f]);
    assert!(code == 0 && o.contains("disconnected → connected: paneld"), "{o}");
    let (code, _, e) = exec(&["-C", d, "view", "nothing", "--file", f]);
    assert_eq!(code, 2);
    assert!(e.contains("unknown view"), "{e}");
    let (code, _, e) = exec(&["-C", d, "view", "--format", "pdf", "--file", f]);
    assert_eq!(code, 2);
    assert!(e.contains("unknown format"), "{e}");
}

#[test]
fn views_lists_built_in_suggested_and_declared() {
    let t = format!(
        "{}\nThe view \"energy\" shows the key-value store \"Store\".\nThe view \"energy\" shows every \"thread\".\nThe view \"energy\" has the form \"matrix\".\n",
        fs::read_to_string(dir().join("corpora/wall_panel.intent")).unwrap()
    );
    let p = project("list", &[("panel.intent", &t)]);
    let (code, o, _) = exec(&["-C", p.to_str().unwrap(), "views"]);
    assert_eq!(code, 0);
    assert!(o.contains("built-in:\n") && o.contains("suggested:\n  structure — always\n  deployment — 3 machines\n  concurrency — 7 threads\n"), "{o}");
    assert!(o.contains("declared:\n  energy — shows Store; every thread; form matrix\n"), "{o}");
    let (code, o, _) = exec(&["-C", p.to_str().unwrap(), "views", "--format", "json"]);
    assert!(code == 0 && o.contains("\"suggested\"") && o.contains("\"view\": \"energy\""), "{o}");
    // the declared view also comes out of the project, without --file
    let (code, o, _) = exec(&["-C", p.to_str().unwrap(), "view", "energy"]);
    assert!(code == 0 && o.starts_with("view energy · declared:"), "{o}");
}

#[test]
fn view_with_an_error_in_the_description_stops_with_2() {
    let p = project("error", &[("a.intent", "The program \"p\" contains the module \"m\".\nThe view \"v\" shows every \"gizmo\".\n")]);
    let (code, o, _) = exec(&["-C", p.to_str().unwrap(), "view"]);
    assert_eq!(code, 2);
    assert!(o.contains("'gizmo' is not a lexicon type"), "{o}");
}

#[test]
fn without_intent_files_the_commands_say_there_is_no_file() {
    let q = project("no-intent", &[("a.py", "x=1\n")]);
    let d = q.to_str().unwrap();
    for cmd in ["view", "views", "check", "ir", "canon", "facts", "coverage", "stale"] {
        let (code, _, e) = exec(&["-C", d, cmd]);
        assert_eq!(code, 2, "{cmd}: {e}");
        assert!(e.contains("no .intent file"), "{cmd}: {e}");
    }
    // the old format is not read: -m does not exist
    let (code, _, e) = exec(&["-C", d, "-m", "intent.map", "check"]);
    assert_eq!(code, 2);
    assert!(e.contains("usage:"), "{e}");
}

#[test]
fn file_works_in_all_the_commands_that_read_the_project() {
    let p = project("file", &[("x.intent", "The program \"p\" contains the module \"m\".\nThe module \"m\" is implemented in \"m.py\".\n"), ("m.py", "x = 1\n")]);
    let d = p.to_str().unwrap();
    // no *.intent at the root for the command to find on its own: here the file goes in a subfolder
    std::fs::create_dir_all(p.join("sub")).unwrap();
    std::fs::rename(p.join("x.intent"), p.join("sub/y.txt")).unwrap();
    for args in [
        vec!["check"],
        vec!["show", "m.py"],
        vec!["stale"],
        vec!["facts"],
        vec!["coverage"],
        vec!["view"],
        vec!["views"],
        vec!["ir"],
        vec!["canon"],
    ] {
        let mut a = vec!["-C", d];
        a.extend(args.iter().copied());
        a.extend(["--file", "sub/y.txt"]);
        let (code, o, e) = exec(&a);
        // stale without git exits with 2; the others read the given file
        assert!(code == 0 || (args[0] == "stale" && code == 2), "{args:?}: {code} {o}{e}");
        assert!(!e.contains("no .intent file") && !e.contains("usage:"), "{args:?}: {e}");
    }
}
