//! Tests of the views: determinism (same text, same outputs byte for byte, independent of the
//! iteration order of hash tables), text goldens, declared views and suggestions.

use super::super::analysis::Analysis;
use super::super::lexicon::Lexicon;
use super::super::assemble;
use super::super::tests::{check_or_write, read_text, CORPORA};
use super::{Format, ViewKind, Views};
use std::path::PathBuf;

const SAMPLES: [&str; 3] = ["elevator", "shortener", "minimake"];

fn lex() -> Lexicon {
    Lexicon::embedded().expect("valid embedded lexicon")
}

/// Texts to test: the five corpora and the three samples (the `=` lines are left out: the
/// samples have none).
fn sources() -> Vec<(String, String)> {
    let mut v: Vec<(String, String)> = CORPORA.iter().map(|c| (c.to_string(), read_text(&format!("corpora/{c}.intent")))).collect();
    for a in SAMPLES {
        let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("examples/{a}/system.intent"));
        v.push((format!("example {a}"), std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{a}: {e}"))));
    }
    v
}

/// All the views of a text, in each format: (view, format, output).
fn generate_all(text: &str) -> Vec<(String, Format, String)> {
    let lex = lex();
    let a = Analysis::text(text, &lex);
    assert!(a.errors.is_empty(), "{:?}", a.errors);
    let v = Views::new(&a);
    let mut out = Vec::new();
    for q in v.all() {
        for f in [Format::Text, Format::Svg, Format::Json] {
            out.push((v.id(&q), f, v.generate(&q, f)));
        }
    }
    out
}

#[test]
fn all_views_come_out_equal_in_two_generations() {
    for (name, t) in sources() {
        // each generation builds a fresh analysis (another hash state): the output must not change
        let (a, b) = (generate_all(&t), generate_all(&t));
        assert!(!a.is_empty(), "{name}");
        assert_eq!(a.len(), b.len(), "{name}");
        for ((va, fa, sa), (_, _, sb)) in a.iter().zip(&b) {
            assert!(sa == sb, "{name}: view {va} ({fa:?}) changed between two generations");
        }
    }
}

#[test]
fn svgs_well_formed_without_invalid_numbers() {
    for (name, t) in sources() {
        for (v, f, s) in generate_all(&t) {
            match f {
                Format::Svg => {
                    assert!(s.starts_with("<svg ") && s.trim_end().ends_with("</svg>"), "{name}/{v}");
                    assert!(!s.contains("NaN") && !s.contains("inf\"") && !s.contains("-inf"), "{name}/{v}: invalid number");
                    assert_eq!(s.matches("<text ").count(), s.matches("</text>").count(), "{name}/{v}");
                }
                Format::Json => assert!(s.starts_with('{') && s.trim_end().ends_with('}'), "{name}/{v}"),
                Format::Text => assert!(s.starts_with("view "), "{name}/{v}"),
            }
        }
    }
}

#[test]
fn structure_text_goldens() {
    let lex = lex();
    for c in ["wall_panel", "elevator"] {
        let a = Analysis::text(&read_text(&format!("corpora/{c}.intent")), &lex);
        let v = Views::new(&a);
        let q = v.resolve("structure").unwrap();
        check_or_write(&format!("golden/{c}.view"), &v.generate(&q, Format::Text));
    }
}

fn with_views(extra: &str) -> String {
    format!("{}\n{extra}\n", read_text("corpora/wall_panel.intent"))
}

#[test]
fn declared_view_selects_groups_and_marks() {
    let t = with_views(
        "The view \"energy\" shows the key-value store \"Store\".\n\
         The view \"energy\" shows every \"thread\".\n\
         The view \"energy\" has the form \"matrix\".\n\
         The view \"deployment\" shows every \"machine\" and \"process\".\n\
         The view \"deployment\" groups by \"runs on\".\n\
         The view \"deployment\" marks \"gaps\".",
    );
    let lex = lex();
    let a = Analysis::text(&t, &lex);
    assert!(a.errors.is_empty(), "{:?}", a.errors);
    let v = Views::new(&a);
    assert_eq!(v.decl.len(), 2);
    // energy: only the relations between Store and the threads (induced subgraph)
    let q = v.resolve("energy").unwrap();
    assert!(matches!(q, ViewKind::Declared(_)));
    let o = v.generate(&q, Format::Text);
    assert!(o.contains("Store · key-value store: accept device consumes; clock fills; monitor consumes; mqtt fills; weather fills"), "{o}");
    assert!(!o.contains("alert queue ·"), "{o}");
    // a declared deployment beats the built-in one of the same name; groups by 'runs on'
    let q = v.resolve("deployment").unwrap();
    assert!(matches!(q, ViewKind::Declared(_)));
    let o = v.generate(&q, Format::Text);
    assert!(o.contains("server host · machine:\n  chrome-headless-shell · process"), "{o}");
    assert!(o.contains("paneld · process: sends panel-server"), "{o}");
    // chosen marks: without 'invariants' and 'notes', the view does not show them; 'gaps' it does
    assert!(o.contains("gaps (the description does not say)"), "{o}");
    assert!(!o.contains("invariants touching"), "{o}");
    // the three outputs exist for the declared one
    for f in [Format::Svg, Format::Json] {
        assert!(v.generate(&q, f).len() > 200);
    }
    // catalog: the declared ones appear
    let c = v.catalog_text();
    assert!(c.contains("energy — shows Store; every thread; form matrix"), "{c}");
}

#[test]
fn shapes_and_groupings_do_not_break() {
    // every combination of form with and without grouping, in the three formats, in any corpus
    for (name, t) in sources() {
        for form in ["boxes", "matrix", "list"] {
            for group_by in [None, Some("contains")] {
                let mut extra = format!("The view \"x\" shows every \"module\", \"process\" and \"thread\".\nThe view \"x\" has the form \"{form}\".\n");
                if let Some(g) = group_by {
                    extra.push_str(&format!("The view \"x\" groups by \"{g}\".\n"));
                }
                let lex = lex();
                let a = Analysis::text(&format!("{t}\n{extra}"), &lex);
                assert!(a.errors.is_empty(), "{name}: {:?}", a.errors);
                let v = Views::new(&a);
                let q = v.resolve("x").unwrap();
                for f in [Format::Text, Format::Svg, Format::Json] {
                    assert!(!v.generate(&q, f).is_empty(), "{name} {form} {group_by:?}");
                }
            }
        }
    }
}

#[test]
fn suggestion_follows_the_rules() {
    let lex = lex();
    let ids = |t: &str| -> Vec<(String, String)> {
        let a = Analysis::text(t, &lex);
        Views::new(&a).suggested().into_iter().map(|s| (s.id, s.reason)).collect()
    };
    // a single sentence: only the structure
    assert_eq!(ids("The program \"p\" contains the module \"m\"."), vec![("structure".into(), "always".into())]);
    // wall_panel: 3 machines, 7 threads, invariants, gaps, absence and 3 state machines
    let k = ids(&read_text("corpora/wall_panel.intent"));
    let names: Vec<&str> = k.iter().map(|(i, _)| i.as_str()).collect();
    assert_eq!(names[..4], ["structure", "deployment", "concurrency", "constraints"]);
    assert_eq!(k[1].1, "3 machines");
    assert_eq!(k[2].1, "7 threads");
    assert_eq!(k[3].1, "13 invariants, 3 gaps, 1 declared absence");
    assert!(names.contains(&"machine:connection") && names.len() == 7, "{names:?}");
    // two machines or more suggest deployment; a single one does not
    let m = |n: usize| -> String { (0..n).map(|i| format!("The machine \"m{i}\" contains the process \"p{i}\".\n")).collect() };
    assert!(!ids(&m(1)).iter().any(|(i, _)| i == "deployment"));
    assert!(ids(&m(2)).iter().any(|(i, _)| i == "deployment"));
    // a single thread does not suggest concurrency
    assert!(!ids("The program \"p\" contains the thread \"t\".").iter().any(|(i, _)| i == "concurrency"));
}

#[test]
fn unknown_view_name_and_machine() {
    let lex = lex();
    let a = Analysis::text(&read_text("corpora/wall_panel.intent"), &lex);
    let v = Views::new(&a);
    assert!(v.resolve("Concurrency").is_ok());
    assert!(v.resolve("machine:connection").is_ok());
    assert!(v.resolve("connection").is_ok());
    assert!(v.resolve("machine:CONNECTION").is_ok());
    assert!(v.resolve("nothing").unwrap_err().contains("unknown view"));
    assert!(v.resolve("machine:nothing").unwrap_err().contains("connection"));
}

#[test]
fn absence_and_gap_stay_distinct_and_notes_marked() {
    let lex = lex();
    let a = Analysis::text(&read_text("corpora/elevator.intent"), &lex);
    let v = Views::new(&a);
    let t = v.generate(&v.resolve("structure").unwrap(), Format::Text);
    assert!(t.contains("∅ field “seed”: no limit, on purpose"), "{t}");
    assert!(t.contains("? percentile “wait p95”: missing definition"), "{t}");
    // every built-in view states the gaps (or that there are none)
    for q in v.all() {
        let o = v.generate(&q, Format::Text);
        assert!(o.contains("gaps (the description does not say)") || o.contains("constraints"), "{o}");
    }
    // notes: informal
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/minimake/system.intent");
    let a = Analysis::text(&std::fs::read_to_string(p).unwrap(), &lex);
    let v = Views::new(&a);
    let t = v.generate(&v.resolve("structure").unwrap(), Format::Text);
    assert!(t.contains("notes (free text, outside the IR)"), "{t}");
    let s = v.generate(&v.resolve("structure").unwrap(), Format::Svg);
    assert!(s.contains("informal: free text, outside the IR"));
}

#[test]
fn assemble_is_not_needed_for_plain_text() {
    // the views work with the text assembled from several files (as the command does)
    let f = assemble(&[("a.intent".into(), "The program \"p\" contains the module \"m\".".into()), ("b.intent".into(), "The module \"m\" uses the module \"n\".".into())], false).unwrap();
    let a = Analysis::new(&f.lines, &f.lex);
    let v = Views::new(&a);
    assert!(v.generate(&ViewKind::Builtin(super::builtin::BuiltinView::Structure), Format::Text).contains("m · module"));
}

/// The shapes of an entity carry `data-entity` (and `data-state` in a state machine) so a page
/// can find them; nothing else in the drawing changes.
#[test]
fn svg_shapes_carry_the_entity_they_draw() {
    let text = "The program \"demo\" contains the module \"a & b\" and the module \"c\".\n\
        The module \"a & b\" uses the module \"c\".\n\
        The state machine \"door\" has the states \"open\" and \"closed\".\n\
        The state machine \"door\" starts in the state \"closed\".\n\
        The module \"c\" moves the state machine \"door\" from the state \"closed\" to the state \"open\".\n";
    let lex = lex();
    let a = Analysis::text(text, &lex);
    assert!(a.errors.is_empty(), "{:?}", a.errors);
    let v = Views::new(&a);
    let svg_of = |id: &str| {
        let q = v.all().into_iter().find(|q| v.id(q) == id).unwrap_or_else(|| panic!("view {id}"));
        v.generate(&q, Format::Svg)
    };
    let structure = svg_of("structure");
    assert!(structure.contains("<rect data-entity=\"a &amp; b\" "), "module box tagged and escaped");
    assert!(structure.contains("<text data-entity=\"c\" "), "module text tagged");
    let machine = svg_of("machine:door");
    assert!(machine.contains("<rect data-entity=\"door\" data-state=\"open\" "));
    assert!(machine.contains("<text data-entity=\"door\" data-state=\"closed\" "));
    // only attributes were added: without them the shapes are the plain primitives
    let plain = structure.replace(" data-entity=\"a &amp; b\"", "").replace(" data-entity=\"c\"", "");
    assert!(!plain.contains("data-entity=\"a"), "{plain}");
}
