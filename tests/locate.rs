//! The `where` command: quick orientation on where to change things. Goldens of two queries in
//! two projects (rewritten with `UPDATE_GOLDEN=1 cargo test`), determinism across processes and limits.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn exec(args: &[&str]) -> (i32, String, String) {
    let o = Command::new(env!("CARGO_BIN_EXE_intentree")).args(args).output().unwrap();
    (o.status.code().unwrap(), String::from_utf8_lossy(&o.stdout).to_string(), String::from_utf8_lossy(&o.stderr).to_string())
}

fn locate(dir: &str, file: &str, extra: &[&str]) -> (i32, String, String) {
    let mut a = vec!["-C", dir, "where", "--file", file];
    a.extend(extra);
    exec(&a)
}

fn check_or_write(rel: &str, got: &str) {
    let p = root().join("tests/intent/golden").join(rel);
    if std::env::var_os("UPDATE_GOLDEN").is_some_and(|v| v == "1") {
        fs::write(&p, got).unwrap();
        return;
    }
    let expected = fs::read_to_string(&p).unwrap_or_else(|e| panic!("{rel}: {e}"));
    assert!(got == expected, "{rel} differs from the golden (UPDATE_GOLDEN=1 rewrites it)\n--- expected\n{expected}\n--- got\n{got}");
}

/// Queries for each project: the terms.
const QUERIES_PANEL: [&[&str]; 2] = [&["touch", "screen", "mqtt"], &["render", "fallback"]];
const QUERIES_ELEVATOR: [&[&str]; 2] = [&["tie", "queue", "events"], &["elevator/report.py"]];

fn block(dir: &str, file: &str, queries: &[&[&str]]) -> String {
    let mut o = String::new();
    for q in queries {
        let (code, output, e) = locate(dir, file, q);
        assert_eq!(code, 0, "{q:?}: {e}");
        o.push_str(&format!("$ where {}\n{output}\n", q.join(" ")));
    }
    o
}

#[test]
fn where_matches_the_goldens_in_wall_panel_and_elevator() {
    let tests = root().join("tests/intent");
    let o = block(tests.to_str().unwrap(), "corpora/wall_panel.intent", &QUERIES_PANEL);
    check_or_write("where_wall_panel.where", &o);
    let example = root().join("examples/elevator");
    let o = block(example.to_str().unwrap(), "system.intent", &QUERIES_ELEVATOR);
    check_or_write("where_elevator.where", &o);
}

#[test]
fn where_fits_the_limit_and_is_deterministic_across_processes() {
    let tests = root().join("tests/intent");
    let d = tests.to_str().unwrap();
    for c in ["elevator", "field_bridge", "mesh_patterns", "live_captions", "wall_panel"] {
        let f = format!("corpora/{c}.intent");
        for q in [&["server", "port", "state", "screen"][..], &["thread", "queue", "time", "data"]] {
            let (code, first, _) = locate(d, &f, q);
            assert!(code == 0 || code == 1, "{c}");
            assert!(first.len() <= 2560, "{c} {q:?}: {} bytes\n{first}", first.len());
            // each process has another hash seed: the output must not change
            for _ in 0..4 {
                assert_eq!(locate(d, &f, q).1, first, "{c} {q:?}");
            }
            for max in ["400", "900", "5000"] {
                let (_, o, _) = locate(d, &f, &[q, &["--max", max]].concat());
                assert!(o.len() <= max.parse::<usize>().unwrap() + 4, "{c} --max {max}: {} bytes", o.len());
            }
            let (_, j1, _) = locate(d, &f, &[q, &["--format", "json"]].concat());
            let (_, j2, _) = locate(d, &f, &[q, &["--format", "json"]].concat());
            assert_eq!(j1, j2);
        }
    }
}

#[test]
fn where_json_has_fixed_keys_and_a_larger_max_brings_more() {
    let tests = root().join("tests/intent");
    let d = tests.to_str().unwrap();
    let f = "corpora/wall_panel.intent";
    let (code, j, _) = locate(d, f, &["touch", "screen", "--format", "json"]);
    assert_eq!(code, 0);
    assert!(j.starts_with("{\n \"terms\": [\n  \"touch\",\n  \"screen\"\n ],\n \"components\": ["), "{j}");
    for k in ["\"name\"", "\"type\"", "\"description\"", "\"layer\"", "\"points\"", "\"files\"", "\"symbols\"", "\"sentences\"", "\"uses\"", "\"used_by\"", "\"same_state\"", "\"nearby_rules\"", "\"neighbor\"", "\"near\"", "\"others\""] {
        assert!(j.contains(k), "{k}");
    }
    let (_, short, _) = locate(d, f, &["touch", "screen", "--max", "300"]);
    let (_, long, _) = locate(d, f, &["touch", "screen", "--max", "4000"]);
    assert!(short.len() <= 304 && long.len() > short.len(), "{} {}", short.len(), long.len());
}

#[test]
fn where_matches_without_accents_or_case_by_prefix_and_in_pt_and_en() {
    let tests = root().join("tests/intent");
    let d = tests.to_str().unwrap();
    let f = "corpora/wall_panel.intent";
    let base = locate(d, f, &["clock"]).1;
    // without accents, without case and by prefix give the same answer as the whole word
    assert_eq!(locate(d, f, &["CLÓCK"]).1.lines().skip(1).collect::<Vec<_>>(), base.lines().skip(1).collect::<Vec<_>>());
    assert!(locate(d, f, &["cloc"]).0 == 0);
    // a word of the canonical English sentence (`machine`) and of the Portuguese one (`máquina`)
    let (_, en, _) = locate(d, f, &["machine"]);
    let (_, pt, _) = locate(d, f, &["máquina"]);
    assert_eq!(en.lines().skip(1).collect::<Vec<_>>(), pt.lines().skip(1).collect::<Vec<_>>(), "{en}\n{pt}");
    assert!(en.contains("paneld"), "{en}");
}

#[test]
fn where_accepts_a_file_and_handles_errors() {
    let sample = root().join("examples/elevator");
    let d = sample.to_str().unwrap();
    let (code, o, _) = locate(d, "system.intent", &["elevator/events.py"]);
    assert_eq!(code, 0);
    assert!(o.contains("1 simulator") && o.contains("EventQueue"), "{o}");
    let (code, o, _) = locate(d, "system.intent", &["zzznonexistent"]);
    assert_eq!((code, o.as_str()), (1, "where: zzznonexistent: no component matches\n"));
    let (code, _, e) = exec(&["-C", d, "where", "--file", "system.intent"]);
    assert_eq!(code, 2);
    assert!(e.contains("where requires at least one term"), "{e}");
    let (code, _, e) = locate(d, "system.intent", &["queue", "--max", "10"]);
    assert_eq!(code, 2);
    assert!(e.contains("--max"), "{e}");
    let (code, _, e) = locate(d, "system.intent", &["queue", "--format", "svg"]);
    assert_eq!(code, 2);
    assert!(e.contains("unknown format"), "{e}");
    // only empty words: no search term
    let (code, _, e) = locate(d, "system.intent", &["of", "the"]);
    assert_eq!(code, 2);
    assert!(e.contains("no search term"), "{e}");
}

/// Minimal project: `reader` matches the search words; the traps are in the neighbor `writer`
/// (anchored invariant, note, absence) and in what shares state (`index`), without mentioning
/// any of the search words.
const TRAPS: &str = r#"The program "demo" contains the module "reader", the module "writer", the module "index" and the module "faraway".
The module "reader" is implemented in "src/reader.rs".
The module "writer" is implemented in "src/writer.rs".
The module "index" is implemented in "src/index.rs".
The module "faraway" is implemented in "src/faraway.rs".
The module "reader" uses the module "writer".
The module "reader" reads the file "disk.dat".
The module "writer" writes the file "disk.dat". ^src/writer.rs
Note: the time on disk is in nanoseconds, not in milliseconds.
If the value "payload" is not the value "written", the module "writer" does not write the file "disk.dat". ^src/writer.rs
The module "index" reads the file "disk.dat".
If the value "payload" is not the value "written", the module "index" does not read the file "disk.dat". ^src/index.rs
If the value "payload" is not the value "written", the module "faraway" does not read the file "faraway.dat". ^src/faraway.rs
"#;

fn trap_project() -> PathBuf {
    let d = std::env::temp_dir().join(format!("intentree-where-nearby-{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(d.join("src")).unwrap();
    for a in ["reader", "writer", "index", "faraway"] {
        fs::write(d.join(format!("src/{a}.rs")), "// nothing\n").unwrap();
    }
    fs::write(d.join("demo.intent"), TRAPS).unwrap();
    d
}

#[test]
fn where_shows_the_neighbors_rules_marking_where_each_comes_from() {
    let d = trap_project();
    let d = d.to_str().unwrap();
    let (code, o, e) = locate(d, "demo.intent", &["reader"]);
    assert_eq!(code, 0, "{e}\n{o}");
    let (comps, nearby) = o.split_once("nearby rules\n").unwrap_or_else(|| panic!("no section:\n{o}"));
    assert!(comps.starts_with("where: reader") && !comps.contains("payload"), "{o}");
    // direct neighbor (relation) and neighbor by shared state, each line with the neighbor
    assert!(nearby.contains("- [writer] If the value \"payload\" is not the value \"written\"") && nearby.contains(" ^src/writer.rs"), "{o}");
    assert!(nearby.contains("- [writer] note: the time on disk is in nanoseconds"), "{o}");
    assert!(nearby.contains("- [index] If the value \"payload\" is not the value \"written\", the module \"index\"") && nearby.contains(" ^src/index.rs"), "{o}");
    // whoever is not a neighbor does not come in
    assert!(!nearby.contains("[faraway]"), "{o}");
    // the anchored invariant comes before the note
    assert!(nearby.find("payload").unwrap() < nearby.find("nanoseconds").unwrap(), "{o}");
    // the JSON has the equivalent section
    let (_, j, _) = locate(d, "demo.intent", &["reader", "--format", "json"]);
    assert!(j.contains("\"nearby_rules\"") && j.contains("\"neighbor\": \"writer\"") && j.contains("\"type\": \"note\""), "{j}");
}

#[test]
fn where_nearby_rules_do_not_squeeze_the_components_and_fit_the_limit() {
    let d = trap_project();
    let d = d.to_str().unwrap();
    let (_, full, _) = locate(d, "demo.intent", &["reader", "--max", "6000"]);
    let (_, short, _) = locate(d, "demo.intent", &["reader", "--max", "420"]);
    assert!(short.len() <= 420 + 4 && short.contains("1 reader") && short.contains("nearby rules"), "{short}");
    // with room, the section is not cut arbitrarily: everything there is fits
    assert!(full.matches("\n- [").count() >= 3, "{full}");
    // a size that only fits the components does not lose the component to the new section
    let (_, mini, _) = locate(d, "demo.intent", &["reader", "--max", "150"]);
    assert!(mini.len() <= 154 && mini.contains("1 reader"), "{mini}");
}
