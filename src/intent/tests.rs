//! Tests of the `.intent` format: goldens (rewritten with `UPDATE_GOLDEN=1 cargo test`),
//! shuffle and uniqueness properties, and lexicon checks.

use super::analysis::{Analysis, Fact};
use super::canon::{canonical_en, text_en, text_pt};
use super::lexicon::{Entry, Lexicon};
use super::syntax::{sentence, tokenize};
use super::{assemble, syntax};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

pub(crate) const CORPORA: [&str; 5] = ["elevator", "field_bridge", "mesh_patterns", "live_captions", "wall_panel"];
const CASES: [&str; 13] = ["vocabulary", "syntax", "semantics", "numbers", "conditions", "targets", "time", "absence", "comparisons", "quantifier", "layers", "notes", "views"];

pub(crate) fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/intent")
}

pub(crate) fn read_text(rel: &str) -> String {
    std::fs::read_to_string(dir().join(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

fn lex() -> Lexicon {
    Lexicon::embedded().expect("valid embedded lexicon")
}

fn ir(a: &Analysis) -> String {
    a.ir().compact()
}

/// Sentence lines (without comments, notes or blanks).
fn sentences(text: &str) -> Vec<String> {
    syntax::split_lines(text)
        .into_iter()
        .filter(|l| {
            let s = syntax::trim_blank(l);
            !s.is_empty() && !s.starts_with('#') && !s.starts_with(super::analysis::PREFIX_NOTE)
        })
        .map(String::from)
        .collect()
}

// ------------------------------------------------------------------ goldens

fn check_golden(name: &str, rel: &str, lex: &Lexicon) {
    let a = Analysis::text(&read_text(rel), lex);
    let mut check = String::new();
    for l in a.lines_of_error() {
        check.push_str(&format!("{l}\n"));
    }
    check.push_str(&a.summary());
    check.push('\n');
    for (ext, got) in [("json", a.ir().indented() + "\n"), ("pt", text_pt(&a)), ("en", text_en(&a)), ("check", check)] {
        check_or_write(&format!("golden/{name}.{ext}"), &got);
    }
}

/// Compares with the golden; with `UPDATE_GOLDEN=1`, writes the binary's output in its place.
pub(crate) fn check_or_write(rel: &str, got: &str) {
    if std::env::var_os("UPDATE_GOLDEN").is_some_and(|v| v == "1") {
        std::fs::write(dir().join(rel), got).unwrap_or_else(|e| panic!("{rel}: {e}"));
        return;
    }
    let expected = read_text(rel);
    assert!(got == expected, "{rel} differs from the golden (UPDATE_GOLDEN=1 rewrites it)\n--- expected\n{expected}\n--- got\n{got}");
}

#[test]
fn corpus_goldens() {
    let lex = lex();
    for c in CORPORA {
        check_golden(c, &format!("corpora/{c}.intent"), &lex);
    }
}

#[test]
fn error_and_number_case_goldens() {
    let lex = lex();
    for c in CASES {
        check_golden(c, &format!("cases/{c}.intent"), &lex);
    }
}

#[test]
fn lexicon_list_matches_the_golden() {
    check_or_write("golden/lexicon.list", &lex().list_entries());
}

// ------------------------------------------------------------------ properties

#[test]
fn uniqueness_every_sentence_has_one_analysis() {
    let lex = lex();
    for c in CORPORA {
        let t = read_text(&format!("corpora/{c}.intent"));
        let fs = sentences(&t);
        let ok = fs.iter().filter(|l| tokenize(syntax::trim_blank(l), &lex).and_then(|t| sentence(&t)).is_ok()).count();
        assert_eq!(ok, fs.len(), "{c}: {ok}/{} sentences with one analysis", fs.len());
        assert!(Analysis::text(&t, &lex).errors.is_empty(), "{c}");
    }
}

/// Every preposition of the sentence is accepted by the verb of its clause (the lexicon frame).
fn preps_accepted(q: &syntax::Sentence, lex: &Lexicon) -> bool {
    let ok = |o: &syntax::Clause| match o {
        syntax::Clause::Rel { verb, roles, .. } => roles.iter().all(|(p, _)| lex.verb(verb).is_some_and(|v| v.prep_pt(p).is_some())),
        _ => true,
    };
    match q {
        syntax::Sentence::Simple(o) | syntax::Sentence::All { o, .. } => ok(o),
        syntax::Sentence::Cond { when, then, .. } => ok(when) && ok(then),
    }
}

/// Strong uniqueness: of all the possible tokenizations of the sentence (any segmentation into
/// lexicon expressions, not only the longest-match one), only one has an analysis with
/// prepositions accepted by the verb.
#[test]
fn uniqueness_every_alternative_tokenization_is_rejected() {
    let lex = lex();
    let mut texts: Vec<(String, String)> = CORPORA.iter().map(|c| (c.to_string(), read_text(&format!("corpora/{c}.intent")))).collect();
    texts.extend(CASES.iter().map(|c| (c.to_string(), read_text(&format!("cases/{c}.intent")))));
    let mut total = 0;
    for (name, t) in texts {
        for l in sentences(&t) {
            let Ok((text, _)) = syntax::split_anchors(syntax::trim_blank(&l)) else { continue };
            // only the sentences the parser accepts (the syntax error cases are left out)
            if !tokenize(text, &lex).and_then(|t| sentence(&t)).is_ok_and(|q| preps_accepted(&q, &lex)) {
                continue;
            }
            let good = syntax::tokenizations(text, &lex).into_iter().filter(|ts| sentence(ts).is_ok_and(|q| preps_accepted(&q, &lex))).count();
            assert_eq!(good, 1, "{name}: {good} analyses for: {l}");
            total += 1;
        }
    }
    assert!(total > 350, "{total}");
}

/// splitmix64: own deterministic PRNG (no dependencies).
struct Prng(u64);

impl Prng {
    fn next_value(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn shuffle<T>(&mut self, v: &mut [T]) {
        for i in (1..v.len()).rev() {
            let j = (self.next_value() % (i as u64 + 1)) as usize;
            v.swap(i, j);
        }
    }
}

#[test]
fn order_50_shuffles_give_the_same_ir() {
    let lex = lex();
    let mut rng = Prng(0);
    for c in CORPORA {
        let lines = sentences(&read_text(&format!("corpora/{c}.intent")));
        let base = ir(&Analysis::text(&lines.join("\n"), &lex));
        for k in 0..50 {
            let mut shuffled = lines.clone();
            rng.shuffle(&mut shuffled);
            assert_eq!(ir(&Analysis::text(&shuffled.join("\n"), &lex)), base, "{c}: shuffle {k}");
        }
    }
}

#[test]
fn idempotence_reparsed_canon_en_gives_the_same_ir() {
    let lex = lex();
    for c in CORPORA {
        let a = Analysis::text(&sentences(&read_text(&format!("corpora/{c}.intent"))).join("\n"), &lex);
        let canon = canonical_en(&a).join("\n");
        let b = Analysis::text(&canon, &lex);
        assert_eq!(ir(&b), ir(&a), "{c}");
    }
}

/// Lines of a case that pass together (the error cases mix good and bad sentences): one
/// at a time, the line that adds no error to what is already there is kept.
fn sentences_good(text: &str, lex: &Lexicon) -> Vec<String> {
    let mut good: Vec<String> = Vec::new();
    for l in sentences(text) {
        good.push(l);
        if !Analysis::text(&good.join("\n"), lex).errors.is_empty() {
            good.pop();
        }
    }
    good
}

#[test]
fn idempotence_holds_in_all_cases() {
    let lex = lex();
    for c in CASES {
        let lines = sentences_good(&read_text(&format!("cases/{c}.intent")), &lex);
        let a = Analysis::text(&lines.join("\n"), &lex);
        assert!(a.errors.is_empty(), "{c}: {:?}", a.errors);
        let canon = canonical_en(&a).join("\n");
        let b = Analysis::text(&canon, &lex);
        assert!(b.errors.is_empty(), "{c}: canon with error {:?}\n{canon}", b.errors);
        assert_eq!(ir(&b), ir(&a), "{c}\n{canon}");
    }
}

/// Text of the samples without the anchors (the canonical form does not write them) and without the `=` lines.
fn sample_without_anchors(name: &str) -> String {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("examples/{name}/system.intent"));
    let t = std::fs::read_to_string(&p).unwrap();
    syntax::split_lines(&t).into_iter().map(|l| syntax::split_anchors(syntax::trim_blank(l)).map(|(x, _)| x.to_string()).unwrap()).collect::<Vec<_>>().join("\n")
}

#[test]
fn idempotence_and_uniqueness_on_samples() {
    let lex = lex();
    for name in ["shortener", "minimake", "elevator"] {
        let t = sample_without_anchors(name);
        let a = Analysis::text(&t, &lex);
        assert!(a.errors.is_empty(), "{name}: {:?}", a.errors);
        let canon = canonical_en(&a).join("\n");
        assert_eq!(ir(&Analysis::text(&canon, &lex)), ir(&a), "{name}\n{canon}");
        for l in sentences(&t) {
            let good = syntax::tokenizations(&l, &lex).into_iter().filter(|ts| sentence(ts).is_ok_and(|q| preps_accepted(&q, &lex))).count();
            assert_eq!(good, 1, "{name}: {good} analyses for: {l}");
        }
    }
}

#[test]
fn condition_with_states_creates_the_states() {
    let lex = lex();
    let a = Analysis::text("If the state machine \"sm\" has the states \"x\" and \"y\", the module \"m\" stops.\n", &lex);
    assert!(a.errors.is_empty(), "{:?}", a.errors);
    let when = a.invariants[0].when.as_ref().unwrap();
    for f in when {
        let o = f.obj().unwrap();
        assert_eq!(a.entities.get(o).map(|e| e.kind.as_str()), Some("p.state"), "id {o} without an entity");
    }
    assert_eq!(when.iter().map(|f| a.name(f.obj().unwrap())).collect::<Vec<_>>(), ["x", "y"]);
}

#[test]
fn monotonicity_by_content_id() {
    let lex = lex();
    for c in CORPORA {
        let lines = sentences(&read_text(&format!("corpora/{c}.intent")));
        let full = Analysis::text(&lines.join("\n"), &lex);
        let ids: BTreeMap<(String, String), String> =
            full.entities.values().map(|e| ((e.name.clone(), lex.root(&e.kind)), e.id.clone())).collect();
        let facts: BTreeSet<String> = full.facts.iter().map(|f| f.json().compact()).collect();
        let mut failures = 0;
        for k in 0..lines.len() {
            let mut sub = lines.clone();
            sub.remove(k);
            let s = Analysis::text(&sub.join("\n"), &lex);
            let ok = s.entities.values().all(|e| ids.get(&(e.name.clone(), lex.root(&e.kind))) == Some(&e.id))
                && s.facts.iter().all(|f| facts.contains(&f.json().compact()));
            failures += !ok as usize;
        }
        assert_eq!(failures, 0, "{c}: {failures}/{} removals changed other sentences", lines.len());
    }
}

#[test]
fn paraphrases_converge() {
    let lex = lex();
    let t = read_text("paraphrases.txt");
    let pairs: Vec<&str> = t.lines().filter(|l| !l.trim().is_empty() && !l.starts_with('#')).collect();
    assert_eq!(pairs.len(), 10);
    for p in pairs {
        let (x, y) = p.split_once("|||").expect("pair with |||");
        let (x, y) = (x.trim().replace("\\n", "\n"), y.trim().replace("\\n", "\n"));
        let (fa, fb) = (Analysis::text(&x, &lex), Analysis::text(&y, &lex));
        assert!(fa.errors.is_empty() && fb.errors.is_empty(), "{p}: {:?} {:?}", fa.errors, fb.errors);
        assert!(fa.facts == fb.facts && fa.invariants == fb.invariants, "diverge: {p}");
    }
}

// ------------------------------------------------------------------ error cases

fn errors(text: &str) -> Vec<String> {
    Analysis::text(text, &lex()).errors
}

/// The errors and hints as they come out in check: `error ...` and `  hint: ...` on separate lines.
fn with_hints(text: &str) -> Vec<String> {
    Analysis::text(text, &lex()).lines_of_error()
}

#[test]
fn error_word_outside_the_lexicon() {
    assert_eq!(errors("The module \"a\" frobnicates the module \"b\"."), ["line 1: word outside the lexicon: 'frobnicates' (after 'The module \"a\"'; expected a verb)"]);
}

#[test]
fn error_appositive_absent() {
    assert_eq!(errors("The module \"a\" uses the \"b\"."), ["line 1: expected a type (like module) after 'The module \"a\" uses the', found the name \"b\""]);
    assert_eq!(errors("The module uses the module \"b\"."), ["line 1: expected a quoted name after 'The module', found the verb 'uses'"]);
}

#[test]
fn error_list_without_and() {
    assert_eq!(
        errors("The module \"a\" uses the module \"b\", the module \"c\"."),
        ["line 1: expected the final period after 'The module \"a\" uses the module \"b\"', found a comma"]
    );
    assert!(errors("The module \"a\" uses the module \"b\", the module \"c\" and the module \"d\".").is_empty());
}

#[test]
fn error_wrong_kind_for_the_role() {
    assert_eq!(
        errors("The module \"a\" validates the function \"b\"."),
        ["line 1: 'validates' does not accept an object of type 'function' (\"b\"); accepts object: configuration, field, module, file"]
    );
    assert_eq!(
        errors("The module \"a\" starts in the state \"x\"."),
        ["line 1: 'starts in' does not accept a subject of type 'module' (\"a\"); accepts subject: state machine"]
    );
}

#[test]
fn error_kinds_outside_a_chain() {
    // two sibling subtypes with the same name: same root, different chains
    let text = "=type x.a | en: alpha thing | pt: coisa a | g: f | parent: e.metric\n\
                 =type x.b | en: beta thing | pt: coisa b | g: f | parent: e.metric\n\
                 The alpha thing \"n\" measures the value \"v\".\nThe beta thing \"n\" measures the value \"v\".\n";
    let f = assemble(&[("p.intent".into(), text.into())], false).expect("valid extension");
    let a = Analysis::new(&f.lines, &f.lex);
    assert_eq!(a.errors, ["'n': types ['x.a', 'x.b'] are not in the same chain"]);
}

// ------------------------------------------------------------------ messages that help converge

/// The hint (second line) of the text's only error.
fn hint(text: &str) -> String {
    let l = with_hints(text);
    assert_eq!(l.len(), 2, "{l:?}");
    assert!(l[0].starts_with("error "), "{l:?}");
    l[1].strip_prefix("  hint: ").expect("hint line").to_string()
}

#[test]
fn word_hint_suggests_by_synonym_and_by_distance() {
    // synonym listed in the lexicon (syn:)
    let d = hint("The module \"a\" opens the file \"b\".");
    assert!(d.contains("'reads' (verb; the lexicon lists 'opens' as a synonym)"), "{d}");
    // edit distance, in the class the position asks for (adjective after 'is')
    let d = hint("The module \"a\" is immutabel.");
    assert!(d.contains("close approved forms: 'immutable' (adjective)"), "{d}");
    // the first word of an expression (groups -> groups by)
    let d = hint("The module \"a\" groups the value \"x\".");
    assert!(d.contains("'groups by' (verb)"), "{d}");
    // at most 3 suggestions
    let d = hint("The module \"a\" has the value \"x\".");
    assert!(d.matches("(verb)").count() <= 3, "{d}");
}

#[test]
fn hint_does_not_suggest_a_merely_similar_spelling() {
    // click/clock, allows/follows, fails/falls back to: different words, not a typo
    for (v, bad) in [("clicks", "clock"), ("click", "clock"), ("allows", "follows"), ("fails", "falls back to"), ("fail", "falls back to")] {
        let d = hint(&format!("The module \"a\" {v} the module \"b\"."));
        assert!(!d.contains(bad), "{v}: {d}");
        assert!(d.contains("intentree lexicon propose"), "{v}: {d}");
    }
    let d = hint("The module \"a\" click the module \"b\".");
    assert!(d.contains("no similar approved form"), "{d}");
    // a typo still suggests
    let d = hint("The module \"a\" conects the module \"b\".");
    assert!(d.contains("'connects to' (verb)"), "{d}");
    let d = hint("The module \"a\" raeds the file \"b\".");
    assert!(d.contains("'reads' (verb)"), "{d}");
}

#[test]
fn word_hint_brings_the_ready_propose_command() {
    let d = hint("The module \"a\" frobnicates the module \"b\".");
    assert!(d.contains("intentree lexicon propose verb \"frobnicates\" --meaning "), "{d}");
    let d = hint("The module \"a\" uses the gizmo \"b\".");
    assert!(d.contains("intentree lexicon propose type \"gizmo\" --meaning "), "{d}");
    // core word (preposition, 'and'): nothing to propose
    let d = hint("The module \"a\" reads the file \"b\" at the file \"c\".");
    assert!(!d.contains("lexicon propose") && d.contains("prepositions"), "{d}");
}

#[test]
fn passive_voice_and_unquoted_name_hint() {
    assert!(hint("The module \"a\" is read by the module \"b\".").contains("there is no passive voice"));
    assert!(hint("The module a uses the module \"b\".").contains("names go in quotes"));
}

#[test]
fn syntax_hint_depends_on_what_was_missing() {
    // two verbs with 'and'
    let d = hint("The module \"a\" writes the value \"x\" and reads the value \"y\".");
    assert!(d.contains("one sentence per verb"), "{d}");
    // condition without a verb
    let d = hint("If the value \"x\", the module \"a\" stops.");
    assert!(d.contains("complete clause"), "{d}");
    // object without an article after a preposition
    let d = hint("The module \"a\" returns the value \"1\" on error.");
    assert!(d.contains("after 'on' write the error"), "{d}");
    // list without 'and'
    assert!(hint("The module \"a\" uses the module \"b\", the module \"c\".").contains("last item takes 'and'"));
    // missing period
    assert!(hint("The module \"a\" uses the module \"b\"").contains("the final period of the sentence is missing"));
}

#[test]
fn wrong_kind_hint_brings_verbs_that_accept_the_kind() {
    let l = with_hints("The module \"a\" validates the function \"b\".");
    assert_eq!(l[0], "error line 1: 'validates' does not accept an object of type 'function' (\"b\"); accepts object: configuration, field, module, file");
    assert!(l[1].contains("general verbs that accept 'function' as object: 'uses', 'reads', 'calls'"), "{}", l[1]);
    // verb linked by a synonym (syn:) appears as next
    let t = "=verb a.zap | en: zaps | pt: zapeia | syn: verifies\nThe module \"a\" validates the function \"b\".\n";
    let f = assemble(&[("p.intent".into(), t.into())], false).unwrap();
    let a = Analysis::new(&f.lines, &f.lex);
    assert!(a.lines_of_error()[1].contains("close verbs that accept 'function' as object: 'zaps'"), "{:?}", a.lines_of_error());
}

#[test]
fn preposition_and_role_hint() {
    let l = with_hints("The module \"a\" replaces the value \"x\" to the value \"y\".");
    assert_eq!(l[0], "error line 1: 'replaces' does not accept the preposition 'to' (accepted: with)");
    assert!(l[1].contains("verbs that accept 'to': "), "{}", l[1]);
    let l = with_hints("The module \"a\" reads the value \"x\" to the value \"y\".");
    assert_eq!(l[0], "error line 1: 'reads' does not accept the preposition 'to' (accepted: from, with)");
    let l = with_hints("The module \"a\" moves the state machine \"m\" to the state \"s\".");
    assert!(l[0].contains("'moves' requires the preposition 'from' in the sentence"), "{l:?}");
    assert!(l[1].contains("full form: The <type> \"<name>\" moves the <type> \"<name>\" from the <type> \"<name>\" to the <type> \"<name>\"."), "{}", l[1]);
}

#[test]
fn synonym_that_is_already_an_approved_form_is_a_lexicon_error() {
    let t = "=verb a.zap | en: zaps | pt: zapeia | syn: reads\n";
    let errors = assemble(&[("p.intent".into(), t.into())], false).err().expect("repeated synonym");
    assert!(errors.iter().any(|e| e.contains("a.zap: synonym 'reads' is already an approved form")), "{errors:?}");
}

// ------------------------------------------------------------------ lexicon and project

#[test]
fn http_and_cli_words_read_and_the_base_of_records_only_holds_after_negation() {
    let t = "The http endpoint \"api:8000\" contains the route \"GET /x\".
The route \"GET /x\" responds with the status code \"404\".
The route \"GET /x\" redirects to the route \"GET /y\".
The route \"GET /x\" does not respond with the status code \"500\".
The module \"m\" parses the file \"f\" into the record \"r\".
The module \"m\" records the event \"e\" to the table \"t\".
The module \"m\" does not record the event \"e2\".
The module \"m\" groups by the field \"day\".
The test suite \"ts\" tests the module \"m\".
The module \"m\" waits for the process \"p\".
The module \"m\" retries up to \"20 times\".
The option \"-j\" defaults to the value \"4\".
The environment variable \"X\" defaults to the value \"1\".
The module \"m\" validates the file \"f\".
The module \"m\" replaces the file \"a\" with the file \"b\".
The stream \"stdout\" is empty.
The digest \"sha256\" is persistent.
The header \"Referer\" is persistent.
The request \"r1\" is persistent.
";
    assert_eq!(errors(t), Vec::<String>::new());
    // the base of `records` is only a verb after the negation; outside it `record` is still the type
    assert!(errors("The module \"m\" record the event \"e\".").iter().any(|e| e.contains("record")));
    // the option presupposes the default; `has no default` declares the absence
    let l = lex();
    let a = Analysis::text("The option \"-j\" is persistent.", &l);
    assert_eq!(a.gaps.len(), 1);
    let a = Analysis::text("The option \"-j\" has no default.", &l);
    assert!(a.gaps.is_empty() && a.errors.is_empty());
    // `groups by` outside a view accepts an object; in a view it requires the relation in quotes
    assert!(errors("The view \"v\" shows the module \"m\".\nThe view \"v\" groups by the field \"day\".").iter().any(|e| e.contains("quoted relation")));
}

#[test]
fn embedded_lexicon_passes_the_static_checks() {
    let l = lex();
    assert!(l.check().is_empty());
    assert_eq!(l.version, "0.3");
    // every verb and type has a Portuguese form; every requires points to something that exists
    assert!(l.kinds.iter().all(|t| !t.pt.is_empty()) && l.verbs.iter().all(|v| !v.pt.is_empty()));
}

#[test]
fn overlapping_expressions_in_the_lexicon() {
    // the embedded one has none: 'starts in' x 'starts' + 'in' does not count (a.launches does not accept 'in'),
    // 'state machine' x 'state' + 'machine' is not grammatical, 'is in' x 'is' + 'in' neither
    assert_eq!(lex().overlaps(), Vec::<String>::new());
    let es = [
        Entry { label: "p.intent:1".into(), text: "=verb a.zap | en: zaps | pt: zapeia | preps: to=para" },
        Entry { label: "p.intent:2".into(), text: "=verb a.zapto | en: zaps to | pt: zapeia até" },
        Entry { label: "p.intent:3".into(), text: "=type x.cx | en: gadget box | pt: caixa | g: f" },
        Entry { label: "p.intent:4".into(), text: "=type x.g | en: gadget | pt: aparelho | g: m" },
        Entry { label: "p.intent:5".into(), text: "=type x.b | en: box | pt: caixote | g: m" },
    ];
    let l = Lexicon::load(&es).unwrap();
    assert_eq!(l.overlaps(), ["overlap: 'zaps to' ('VERB', 'a.zapto') also reads 'zaps' ('VERB', 'a.zap') + 'to' ('PREP', 'to')"]);
}

fn extension(lines: &[&str]) -> Vec<String> {
    let es: Vec<Entry> = lines.iter().enumerate().map(|(i, t)| Entry { label: format!("p.intent:{}", i + 1), text: t }).collect();
    match Lexicon::load(&es) {
        Ok(_) => vec![],
        Err(e) => e,
    }
}

#[test]
fn static_checks_catch_broken_extensions() {
    assert!(extension(&["=type x.ok | en: gadget | pt: aparelho | g: m | parent: e.metric"]).is_empty());
    let cases: [(&[&str], &str); 9] = [
        (&["=type x.a | en: gadget | pt: aparelho | g: m | parent: x.nothing"], "unknown parent: x.nothing"),
        (&["=type x.a | en: ga | pt: a | g: m | parent: x.b", "=type x.b | en: gb | pt: b | g: m | parent: x.a"], "subtype cycle"),
        (&["=type x.a | en: gadget | pt: aparelho | g: m | requires: owner=a.nothing"], "unknown verb: a.nothing"),
        (&["=type x.a | en: gadget | pt: aparelho | g: m | requires: owner=a.uses x.nothing"], "unknown type: x.nothing"),
        (&["=type x.a | en: gadget | g: x"], "gender 'x' (expected m or f)"),
        (&["=verb a.x | en: zaps | pt: zapeia | preps: to=para, x=y"], "preposition outside the core: x"),
        (&["=verb a.x | en: module | pt: zapeia"], "'module' has two classes: ('NOUN', 'e.module') and ('VERB', 'a.x')"),
        (&["=type e.module | en: thing | pt: coisa | g: f"], "duplicate id: e.module"),
        (&["=core article | en: a"], "only exists in the embedded lexicon"),
    ];
    for (lines, excerpt) in cases {
        let e = extension(lines);
        assert!(e.iter().any(|m| m.contains(excerpt)), "{lines:?}: expected '{excerpt}' in {e:?}");
    }
    assert!(extension(&["=type x.a | en: gadget | pt: aparelho | g: m | color: blue"]).iter().any(|m| m.starts_with("p.intent:1: unknown field")));
}

#[test]
fn project_with_several_files_and_extension() {
    let elev = read_text("corpora/elevator.intent");
    let lines = sentences(&elev);
    let (a, b) = lines.split_at(lines.len() / 2);
    let lex = lex();
    let um = ir(&Analysis::text(&elev, &lex));
    for order in [[0, 1], [1, 0]] {
        let parts = [a.join("\n"), b.join("\n")];
        let texts: Vec<(String, String)> = order.iter().map(|&i| (format!("p{i}.intent"), parts[i].clone())).collect();
        let f = assemble(&texts, false).unwrap();
        assert_eq!(ir(&Analysis::new(&f.lines, &f.lex)), um);
    }
    // a project extension applies to the sentences of another file; an error cites file:line
    let f = assemble(
        &[
            ("a.intent".into(), "The module \"m\" zaps the module \"n\".\nThe module \"m\" frobs.\n".into()),
            ("b.intent".into(), "# project vocabulary\n=verb a.zap | en: zaps, zap | pt: zapeia\n".into()),
        ],
        false,
    )
    .unwrap();
    let an = Analysis::new(&f.lines, &f.lex);
    assert_eq!(an.errors, ["a.intent:2: word outside the lexicon: 'frobs' (after 'The module \"m\"'; expected a verb)"]);
    assert!(matches!(&an.facts[..], [Fact::Rel { rel, .. }] if rel == "a.zap"));
}

#[test]
fn notes_stay_with_the_file_or_with_the_sentence_above_in_the_same_file() {
    let f = assemble(
        &[
            ("a.intent".into(), "Note: top of a\nThe module \"m\" uses the module \"n\".\nNote: of m\n".into()),
            // the note at the top of b does not stay with the last sentence of a
            ("b.intent".into(), "# comment\nNote: top of b\nThe module \"n\" stops.\nNote: of n\n".into()),
        ],
        false,
    )
    .unwrap();
    let an = Analysis::new(&f.lines, &f.lex);
    assert!(an.errors.is_empty(), "{:?}", an.errors);
    // in file order (files in project order), not alphabetical
    let j: Vec<String> = an.notes.iter().map(|n| n.json().compact()).collect();
    assert_eq!(
        j,
        [
            "{\"file\": \"a.intent\", \"text\": \"top of a\"}",
            "{\"sentence\": \"The module \\\"m\\\" uses the module \\\"n\\\".\", \"text\": \"of m\"}",
            "{\"file\": \"b.intent\", \"text\": \"top of b\"}",
            "{\"sentence\": \"The module \\\"n\\\" terminates.\", \"text\": \"of n\"}",
        ]
    );
    // the reparsed canonical form keeps the file of the top note and the order
    let canon = canonical_en(&an).join("\n");
    assert!(canon.contains("Note[a.intent]: top of a") && canon.contains("Note[b.intent]: top of b"), "{canon}");
    let re = Analysis::text(&canon, &f.lex);
    assert!(re.errors.is_empty(), "{:?}", re.errors);
    let jr: Vec<String> = re.notes.iter().map(|n| n.json().compact()).collect();
    assert_eq!(jr.iter().filter(|x| x.contains("\"file\"")).cloned().collect::<Vec<_>>(), [j[0].clone(), j[2].clone()]);
    assert_eq!(re.ir().compact(), an.ir().compact());
    // the formal IR does not depend on order: the notes come out sorted by content
    let ir = an.ir().compact();
    assert!(ir.find("top of a").unwrap() < ir.find("of m").unwrap(), "{ir}");
    // a note changes neither facts nor invariants
    let without_notes = Analysis::text("The module \"m\" uses the module \"n\".\nThe module \"n\" stops.\n", &f.lex);
    assert_eq!((&an.facts, &an.invariants), (&without_notes.facts, &without_notes.invariants));
}

// ------------------------------------------------------------------ anchors and globs

#[test]
fn anchor_after_the_dot_does_not_change_id_or_canon() {
    let lex = lex();
    let base = read_text("corpora/elevator.intent");
    let anchored = base
        .replace(
            "The module \"simulator\" changes the record \"Elevator\".",
            "The module \"simulator\" changes the record \"Elevator\". ^elevator/simulator.py#Simulator ^elevator/elevator.py",
        )
        .replace(
            "If the record \"Elevator\" is full, the module \"simulator\" skips the record \"Elevator\".",
            "If the record \"Elevator\" is full, the module \"simulator\" skips the record \"Elevator\". ^elevator/events.py#EventQueue",
        );
    assert_ne!(base, anchored);
    let (a, b) = (Analysis::text(&base, &lex), Analysis::text(&anchored, &lex));
    assert!(b.errors.is_empty(), "{:?}", b.errors);
    assert_eq!(canonical_en(&a), canonical_en(&b));
    assert_eq!(text_pt(&a), text_pt(&b));
    assert_eq!(a.entities.keys().collect::<Vec<_>>(), b.entities.keys().collect::<Vec<_>>());
    // the field only appears in the facts and invariants that carried an anchor
    let ir_a = a.ir().compact();
    let ir_b = b.ir().compact();
    assert!(!ir_a.contains("anchors"));
    assert_eq!(ir_b.matches("\"anchors\"").count(), 2);
    assert!(ir_b.contains("\"anchors\": [\"elevator/elevator.py\", \"elevator/simulator.py#Simulator\"]"));
    assert!(ir_b.contains("\"anchors\": [\"elevator/events.py#EventQueue\"], \"if\""), "a conditional invariant carries the field before if");
    // without the field, the IR is the same
    let without_anchors = ir_b.replace("\"anchors\": [\"elevator/elevator.py\", \"elevator/simulator.py#Simulator\"], ", "");
    let without_anchors = without_anchors.replace("\"anchors\": [\"elevator/events.py#EventQueue\"], ", "");
    assert_eq!(without_anchors, ir_a);
}

#[test]
fn anchors_of_a_repeated_sentence_merge() {
    let lex = lex();
    let t = "The module \"a\" uses the module \"b\". ^x/1.py\nThe module \"a\" uses the module \"b\" and the module \"c\". ^x/2.py#f\n";
    let an = Analysis::text(t, &lex);
    assert!(an.errors.is_empty());
    let by_obj: Vec<(String, Vec<String>)> = an.facts.iter().map(|f| (an.name(f.obj().unwrap()).to_string(), an.anchors_of_fact(f))).collect();
    let b = by_obj.iter().find(|(n, _)| n == "b").unwrap();
    let c = by_obj.iter().find(|(n, _)| n == "c").unwrap();
    assert_eq!(b.1, ["x/1.py", "x/2.py#f"]);
    assert_eq!(c.1, ["x/2.py#f"]);
    assert_eq!(an.origins.len(), 2);
    assert_eq!(an.origins[1].anchors, ["x/2.py#f"]);
}

#[test]
fn anchor_malformed_or_inside_quotes() {
    assert_eq!(
        errors("The module \"a\" uses the module \"b\". ^"),
        ["line 1: malformed anchor: '^' (use ^file or ^file#symbol, separated by spaces, after the period)"]
    );
    assert_eq!(errors("The module \"a\" uses the module \"b\". ^#f").len(), 1);
    assert_eq!(errors("The module \"a\" uses the module \"b\". ^x.py loose").len(), 1);
    assert_eq!(errors("The module \"a\" uses the module \"b\" ^x.py.").len(), 1, "an anchor before the period does not count");
    // `^` inside quotes is part of the name
    let l = lex();
    let an = Analysis::text("The module \"a^b\" uses the module \"c\". ^x.py\n", &l);
    assert!(an.errors.is_empty(), "{:?}", an.errors);
    assert!(an.entities.values().any(|e| e.name == "a^b"));
}

#[test]
fn implemented_in_and_excludes_are_literal_facts() {
    let lex = lex();
    let t = "The module \"m\" is implemented in \"src/{a,b}.py\" and \"lib/**\".\nThe program \"p\" excludes \"tests/**\".\n";
    let an = Analysis::text(t, &lex);
    assert!(an.errors.is_empty(), "{:?}", an.errors);
    let literals: Vec<(&str, &str)> = an.facts.iter().map(|f| (f.rel(), f.literal().unwrap())).collect();
    assert_eq!(literals, [("a.implemented_in", "lib/**"), ("a.implemented_in", "src/{a,b}.py"), ("a.excludes", "tests/**")]);
    assert_eq!(
        canonical_en(&an),
        [
            "The module \"m\" is implemented in \"lib/**\".",
            "The module \"m\" is implemented in \"src/{a,b}.py\".",
            "The program \"p\" excludes \"tests/**\"."
        ]
    );
    assert!(text_pt(&an).contains("O módulo “m” reside em “lib/**”.\n"));
    // the canonical English form is reparsable
    assert_eq!(ir(&Analysis::text(&canonical_en(&an).join("\n"), &lex)), ir(&an));
}

// ------------------------------------------------------------------ queue capacity

#[test]
fn negated_verb_outside_is_comes_out_in_correct_english_and_reparses() {
    let lex = lex();
    let t = "The clock \"now\" does not have the unit \"s\".\n\
             The value \"x\" does not have the same unit as the value \"y\".\n\
             The module \"m\" does not use the module \"n\".\n";
    let a = Analysis::text(t, &lex);
    assert!(a.errors.is_empty(), "{:?}", a.errors);
    let en = canonical_en(&a);
    assert!(en.contains(&"The clock \"now\" does not have the unit \"s\".".to_string()), "{en:?}");
    assert!(en.contains(&"The value \"x\" does not have the same unit as the value \"y\".".to_string()), "{en:?}");
    assert!(en.iter().all(|l| !l.contains("does not has")), "{en:?}");
    let b = Analysis::text(&en.join("\n"), &lex);
    assert!(b.errors.is_empty(), "{:?}", b.errors);
    assert_eq!(ir(&a), ir(&b));
}

#[test]
fn summary_counts_as_rules() {
    let lex = lex();
    let a = Analysis::text(&read_text("corpora/elevator.intent"), &lex);
    assert!(a.summary().contains(&format!("{} rules", a.rules.len())), "{}", a.summary());
}

#[test]
fn is_bounded_by_fills_the_queue_capacity() {
    let lex = lex();
    let with_capacity = Analysis::text("The queue \"q\" is bounded by 8.\nThe queue \"r\" is bounded by the value \"n\".\n", &lex);
    assert!(with_capacity.errors.is_empty(), "{:?}", with_capacity.errors);
    assert!(with_capacity.gaps.is_empty(), "{:?}", with_capacity.gaps);
    // no stated capacity, still a gap
    let without_capacity = Analysis::text("The queue \"q\" stops.\n", &lex);
    assert_eq!(without_capacity.gaps.len(), 1);
}
