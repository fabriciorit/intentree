//! Commands of the `.intent` format through the command line: ir, canon, lexicon and check.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/intent")
}

fn read_text(rel: &str) -> String {
    fs::read_to_string(dir().join(rel)).unwrap()
}

fn exec(args: &[&str]) -> (i32, String, String) {
    let o = Command::new(env!("CARGO_BIN_EXE_intentree")).args(args).output().unwrap();
    (o.status.code().unwrap(), String::from_utf8_lossy(&o.stdout).to_string(), String::from_utf8_lossy(&o.stderr).to_string())
}

fn project(name: &str, file_pairs: &[(&str, &str)]) -> PathBuf {
    let d = std::env::temp_dir().join(format!("intentree-intent-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    for (p, c) in file_pairs {
        let f = d.join(p);
        fs::create_dir_all(f.parent().unwrap()).unwrap();
        fs::write(f, c).unwrap();
    }
    d
}

#[test]
fn file_matches_the_goldens() {
    let d = dir();
    let d = d.to_str().unwrap();
    for c in ["elevator", "wall_panel"] {
        let f = format!("corpora/{c}.intent");
        let (code, o, _) = exec(&["-C", d, "ir", "--file", &f]);
        assert_eq!((code, o), (0, read_text(&format!("golden/{c}.json"))));
        let (_, o, _) = exec(&["-C", d, "canon", "--pt", "--file", &f]);
        assert_eq!(o, read_text(&format!("golden/{c}.pt")));
        let (_, o, _) = exec(&["-C", d, "canon", "--file", &f]);
        assert_eq!(o, read_text(&format!("golden/{c}.en")));
        let (code, o, _) = exec(&["-C", d, "check", "--file", &f]);
        assert_eq!((code, o), (0, read_text(&format!("golden/{c}.check"))));
    }
    let (code, o, _) = exec(&["-C", d, "check", "--file", "cases/semantics.intent"]);
    assert_eq!((code, o), (1, read_text("golden/semantics.check")));
}

#[test]
fn project_joins_root_and_intent_folder() {
    let elev = read_text("corpora/elevator.intent");
    let lines: Vec<&str> = elev.lines().collect();
    let (a, b) = lines.split_at(lines.len() / 2);
    let d = project("union", &[("b.intent", &a.join("\n")), ("intent/a.intent", &b.join("\n")), ("intent/sub/x.intent", "junk")]);
    let p = d.to_str().unwrap();
    let (code, o, e) = exec(&["-C", p, "ir"]);
    assert_eq!((code, o.as_str()), (0, read_text("golden/elevator.json").as_str()), "{e}");
    let (code, o, _) = exec(&["-C", p, "check"]);
    assert_eq!((code, o), (0, read_text("golden/elevator.check")));
}

#[test]
fn errors_cite_file_and_line_in_the_project() {
    let d = project(
        "errors",
        &[
            ("a.intent", "The module \"m\" zaps the module \"n\".\nThe module \"m\" frobs.\n"),
            ("intent/lexicon.intent", "=verb a.zap | en: zaps, zap | pt: zapeia\n"),
        ],
    );
    let p = d.to_str().unwrap();
    let (code, o, _) = exec(&["-C", p, "check"]);
    assert_eq!(code, 1);
    assert_eq!(
        o,
        "error a.intent:2: word outside the lexicon: 'frobs' (after 'The module \"m\"'; expected a verb)\n  hint: no similar approved form; if none fits, propose one: intentree lexicon propose verb \"frobs\" --meaning \"<what it means>\"\n1 errors, 2 entities, 1 facts, 0 invariants, 0 rules, 0 gaps, 0 warnings\n"
    );
    let (code, o, _) = exec(&["-C", p, "canon", "--pt"]);
    assert_eq!((code, o.as_str()), (1, "O módulo “m” zapeia o módulo “n”.\n\n"));
    // the lexicon list includes the project extension
    let (code, o, _) = exec(&["-C", p, "lexicon", "list"]);
    assert!(code == 0 && o.contains("\nzaps | zap\n"), "{o}");
}

#[test]
fn a_broken_lexicon_extension_stops_everything() {
    let d = project("lexbroken", &[("a.intent", "=type x.a | en: gadget | pt: aparelho | g: m | parent: x.nothing\nThe module \"m\" uses the module \"n\".\n")]);
    let p = d.to_str().unwrap();
    let (code, o, e) = exec(&["-C", p, "ir"]);
    assert_eq!((code, o.as_str()), (2, ""));
    assert!(e.contains("a.intent:1: type x.a: unknown parent: x.nothing"), "{e}");
    let (code, _, _) = exec(&["-C", p, "lexicon", "check"]);
    assert_eq!(code, 1);
}

#[test]
fn lexicon_check_reports_phrase_overlap() {
    let d = project("lexoverlap", &[("a.intent", "=verb a.zap | en: zaps | pt: zapeia | preps: to=para\n=verb a.zapto | en: zaps to | pt: zapeia até\n")]);
    let (code, o, _) = exec(&["-C", d.to_str().unwrap(), "lexicon", "check"]);
    assert_eq!(code, 1, "{o}");
    assert!(o.contains("error overlap: 'zaps to' ('VERB', 'a.zapto') also reads 'zaps' ('VERB', 'a.zap') + 'to'"), "{o}");
}

#[test]
fn lexicon_list_and_check() {
    let empty = project("lexempty", &[]);
    let p = empty.to_str().unwrap();
    let (code, o, _) = exec(&["-C", p, "lexicon"]);
    assert_eq!((code, o), (0, read_text("golden/lexicon.list")));
    let (code, o, _) = exec(&["-C", p, "lexicon", "check"]);
    assert!(code == 0 && o.starts_with("lexicon 0.3 ok: "), "{o}");
    let (code, _, e) = exec(&["-C", p, "ir"]);
    assert!(code == 2 && e.contains("no .intent file"), "{e}");
}

#[test]
fn check_brings_what_was_understood_what_was_missing_and_the_hint_in_two_lines() {
    let d = project(
        "hints",
        &[("a.intent", "The module \"m\" opens the file \"f\".\nThe module \"m\" validates the function \"n\".\nThe module \"m\" replaces the value \"x\" to the value \"y\".\n")],
    );
    let (code, o, _) = exec(&["-C", d.to_str().unwrap(), "check"]);
    assert_eq!(code, 1);
    let l: Vec<&str> = o.lines().collect();
    assert_eq!(l[0], "error a.intent:1: word outside the lexicon: 'opens' (after 'The module \"m\"'; expected a verb)");
    assert!(l[1].starts_with("  hint: close approved forms: 'reads' (verb; the lexicon lists 'opens' as a synonym)"), "{o}");
    assert_eq!(l[2], "error a.intent:3: 'replaces' does not accept the preposition 'to' (accepted: with)");
    assert!(l[3].starts_with("  hint: verbs that accept 'to': "), "{o}");
    assert!(l[4].starts_with("error a.intent:2: 'validates' does not accept an object of type 'function'") && l[5].starts_with("  hint: "), "{o}");
    // each error has exactly one hint line
    assert_eq!(o.lines().filter(|x| x.starts_with("error ")).count(), o.lines().filter(|x| x.starts_with("  hint: ")).count());
}

const SENTENCE_WITH_NEW_WORD: &str = "The module \"m\" bounces the module \"n\".\n";

#[test]
fn lexicon_propose_writes_the_commented_block_and_pending_lists() {
    let d = project("propose", &[("a.intent", SENTENCE_WITH_NEW_WORD)]);
    let p = d.to_str().unwrap();
    let (code, o, e) = exec(&[
        "-C", p, "lexicon", "propose", "verb", "bounces", "--pt", "rebate", "--meaning", "reply with a bounce", "--evidence", "shortener/server.py",
    ]);
    assert_eq!(code, 0, "{e}");
    assert!(o.contains("proposal written to intent/lexicon-pending.intent: verb a.bounces (bounces)") && o.contains("the tool does not approve"), "{o}");
    let file = fs::read_to_string(d.join("intent/lexicon-pending.intent")).unwrap();
    assert!(file.starts_with("# Pending lexicon proposals"), "{file}");
    assert!(
        file.ends_with(
            "\n# pending: verb \"bounces\" (suggested id a.bounces)\n# meaning: reply with a bounce\n# evidence: shortener/server.py\n# =verb a.bounces | en: bounces, bounce | pt: rebate\n"
        ),
        "{file}"
    );
    // all lines are comments or empty: nothing counts yet
    assert!(file.lines().all(|l| l.is_empty() || l.starts_with('#')), "{file}");
    let (code, o, _) = exec(&["-C", p, "lexicon", "pending"]);
    assert_eq!(code, 0);
    assert!(o.starts_with("1 pending proposal(s) in intent/lexicon-pending.intent\n"), "{o}");
    assert!(o.contains("  verb \"bounces\" a.bounces: reply with a bounce [evidence: shortener/server.py]\n"), "{o}");
    assert!(o.contains("the tool does not approve"), "{o}");
    // the lexicon and the check did not change: the word is still out and the hint shows the command
    let (_, o, _) = exec(&["-C", p, "lexicon", "list"]);
    assert!(!o.contains("bounces"), "{o}");
    let (code, o, _) = exec(&["-C", p, "check"]);
    assert_eq!(code, 1);
    assert!(o.contains("word outside the lexicon: 'bounces'"), "{o}");
}

#[test]
fn a_proposal_only_counts_when_the_human_removes_the_comment() {
    let d = project("approve", &[("a.intent", SENTENCE_WITH_NEW_WORD)]);
    let p = d.to_str().unwrap();
    exec(&["-C", p, "lexicon", "propose", "verb", "bounces", "--pt", "rebate", "--meaning", "x"]);
    // no command approves: neither check, nor lexicon check, nor pending
    for c in [&["check"][..], &["lexicon", "check"], &["lexicon", "pending"], &["canon"]] {
        let mut a = vec!["-C", p];
        a.extend(c);
        exec(&a);
    }
    let file = d.join("intent/lexicon-pending.intent");
    let text = fs::read_to_string(&file).unwrap();
    assert!(text.contains("# =verb a.bounces"), "{text}");
    // the human uncomments
    fs::write(&file, text.replace("# =verb", "=verb")).unwrap();
    let (code, o, _) = exec(&["-C", p, "check"]);
    assert_eq!(code, 0, "{o}");
    let (_, o, _) = exec(&["-C", p, "canon", "--pt"]);
    assert!(o.contains("rebate"), "{o}");
    // a proposal approved by hand leaves the pending list
    let (_, o, _) = exec(&["-C", p, "lexicon", "pending"]);
    assert!(o.starts_with("no pending proposals (1 already approved by hand"), "{o}");
}

#[test]
fn lexicon_propose_refuses_what_exists_and_duplicates() {
    let d = project("propose-errors", &[("a.intent", "The module \"m\" uses the module \"n\".\n")]);
    let p = d.to_str().unwrap();
    let (code, _, e) = exec(&["-C", p, "lexicon", "propose", "verb", "reads", "--pt", "lê", "--meaning", "x"]);
    assert_eq!(code, 2);
    assert!(e.contains("'reads' is already an approved lexicon form (verb a.reads)"), "{e}");
    assert!(!d.join("intent/lexicon-pending.intent").exists());
    let args = ["-C", p, "lexicon", "propose", "type", "gadget", "--pt", "engenhoca", "--meaning", "gadget", "--g", "f"];
    assert_eq!(exec(&args).0, 0);
    let (code, _, e) = exec(&args);
    assert_eq!(code, 2);
    assert!(e.contains("'gadget' is already proposed"), "{e}");
    let file = fs::read_to_string(d.join("intent/lexicon-pending.intent")).unwrap();
    assert!(file.contains("# =type e.gadget | en: gadget | pt: engenhoca | g: f | layer: element"), "{file}");
    // missing --meaning, wrong class
    assert_eq!(exec(&["-C", p, "lexicon", "propose", "type", "gizmo", "--pt", "x"]).0, 2);
    assert_eq!(exec(&["-C", p, "lexicon", "propose", "thing", "gizmo", "--pt", "x", "--meaning", "y"]).0, 2);
    // different ids for proposals whose forms slug alike
    assert_eq!(exec(&["-C", p, "lexicon", "propose", "type", "gadget/", "--meaning", "path"]).0, 0);
    let file = fs::read_to_string(d.join("intent/lexicon-pending.intent")).unwrap();
    assert!(file.contains("(suggested id e.gadget_2)"), "{file}");
}

#[test]
fn format_guide_is_short_and_its_examples_pass_the_parser() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let format_guide = fs::read_to_string(dir.join("FORMAT.md")).unwrap();
    assert!(format_guide.len() < 7000, "the format guide has {} bytes (limit: 7000)", format_guide.len());
    // the sentences of the code blocks (without the patterns with <...> and without trailing comments)
    let mut sentences = Vec::new();
    let mut inside = false;
    for l in format_guide.lines() {
        if l.starts_with("```") {
            inside = !inside;
        } else if inside && !l.trim().is_empty() && !l.contains('<') {
            sentences.push(l.split("   ").next().unwrap().trim_end().to_string());
        }
    }
    assert!(sentences.len() >= 15, "{sentences:?}");
    let d = project("format-md", &[("examples.intent", &(sentences.join("\n") + "\n"))]);
    let (code, o, e) = exec(&["-C", d.to_str().unwrap(), "ir"]);
    assert_eq!(code, 0, "{e}\n{o}");
    assert!(o.contains("\"errors\": []"), "{o}");
}

#[test]
fn lexicon_propose_with_file_writes_next_to_the_file_and_the_hint_carries_the_file() {
    let d = project("propose-file", &[("sub/x.intent", SENTENCE_WITH_NEW_WORD)]);
    let p = d.to_str().unwrap();
    // the check hint repeats the --file
    let (code, o, _) = exec(&["-C", p, "check", "--file", "sub/x.intent"]);
    assert_eq!(code, 1);
    assert!(o.contains("--meaning \"<what it means>\" --file sub/x.intent\n"), "{o}");
    // propose with --file writes to <dir of X>/intent/, not in the project
    let (code, o, e) = exec(&["-C", p, "lexicon", "propose", "verb", "bounces", "--pt", "rebate", "--meaning", "x", "--file", "sub/x.intent"]);
    assert_eq!(code, 0, "{e}");
    assert!(o.contains("intent/lexicon-pending.intent: verb a.bounces"), "{o}");
    assert!(d.join("sub/intent/lexicon-pending.intent").exists());
    assert!(!d.join("intent/lexicon-pending.intent").exists());
    // pending reads from there with --file, and from the project without it
    let (_, o, _) = exec(&["-C", p, "lexicon", "pending", "--file", "sub/x.intent"]);
    assert!(o.starts_with("1 pending proposal(s) in "), "{o}");
    let (_, o, _) = exec(&["-C", p, "lexicon", "pending"]);
    assert!(o.starts_with("no pending proposals"), "{o}");
    // without --file, the hint and the writing stay in the project
    let (_, o, _) = exec(&["-C", p, "check"]);
    assert!(!o.contains("--file"), "{o}");
}

#[test]
fn check_verbose_lists_invariants_and_rules_with_their_origin() {
    let t = "The module \"a\" never writes the file \"x.txt\".\nIf the value \"v\" is empty, the module \"a\" reads the file \"y\" and the file \"z\".\nThe module \"a\" contains the function \"f\".\nEvery function in the module \"a\" is immutable.\n";
    let d = project("verbose", &[("a.intent", t)]);
    let p = d.to_str().unwrap();
    let (code, o, e) = exec(&["-C", p, "check", "--verbose"]);
    assert_eq!(code, 0, "{o}{e}");
    assert!(o.contains("invariants (3):\n  a.intent:1  The module \"a\" does not write the file \"x.txt\".\n"), "{o}");
    assert!(o.contains("  a.intent:2  If the value \"v\" is empty, the module \"a\" reads the file \"y\".\n  a.intent:2  If the value \"v\" is empty, the module \"a\" reads the file \"z\".\n"), "{o}");
    assert!(o.contains("several invariants (normal form)") && o.contains("never or does not"), "{o}");
    assert!(o.contains("rules (1):\n  a.intent:4  "), "{o}");
    // without the option, the output does not change
    let (_, short, _) = exec(&["-C", p, "check"]);
    assert!(!short.contains("invariants (") && o.starts_with(&short), "{short}");
    assert_eq!(exec(&["-C", p, "check", "-v"]).1, o);
}

#[test]
fn lexicon_propose_without_pt_falls_back_to_the_english_form_in_pt_output() {
    let d = project("propose-no-pt", &[("a.intent", "The module \"m\" bounces the module \"n\".\nThe module \"m\" is sortable.\nThe module \"m\" uses the gizmo \"g\".\n")]);
    let p = d.to_str().unwrap();
    for args in [
        &["lexicon", "propose", "verb", "bounces", "--meaning", "x"][..],
        &["lexicon", "propose", "adjective", "sortable", "--meaning", "x"],
        &["lexicon", "propose", "type", "gizmo", "--meaning", "x"],
    ] {
        let mut a = vec!["-C", p];
        a.extend(args);
        let (code, _, e) = exec(&a);
        assert_eq!(code, 0, "{e}");
    }
    let file = fs::read_to_string(d.join("intent/lexicon-pending.intent")).unwrap();
    assert!(file.contains("# =verb a.bounces | en: bounces, bounce\n") && !file.contains("pt:"), "{file}");
    // the human approves all three
    let file = file.replace("# =verb", "=verb").replace("# =adjective", "=adjective").replace("# =type", "=type");
    fs::write(d.join("intent/lexicon-pending.intent"), file).unwrap();
    let (code, o, e) = exec(&["-C", p, "check"]);
    assert_eq!(code, 0, "{o}{e}");
    let (code, o, e) = exec(&["-C", p, "canon", "--pt"]);
    assert_eq!(code, 0, "{e}");
    assert!(o.contains("O módulo “m” bounces o módulo “n”."), "{o}");
    assert!(o.contains("O módulo “m” é sortable."), "{o}");
    assert!(o.contains("O módulo “m” usa o gizmo “g”."), "{o}");
}
