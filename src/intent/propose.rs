//! Lexicon proposals: `lexicon propose` writes the new entry to a pending file in the project,
//! as comment lines; `lexicon pending` lists them. A proposal only becomes a lexicon entry when
//! the human removes the `# ` from the `=...` line: the tool never approves.

use super::lexicon::{Lexicon, Tok};
use crate::vfs::Fs;
use std::path::Path;

/// Where the proposals live, relative to the project root (with `--file X`, to the directory of
/// X). The project reads `intent/*.intent`, and this file only holds comments, so nothing changes
/// until the human approves.
pub const FILE: &str = "intent/lexicon-pending.intent";

const HEADER: &str = "# Pending lexicon proposals (intentree lexicon propose). Everything here is a comment:\n\
# nothing counts until you approve it. To approve, remove the \"# \" from the proposal's \"=...\" line and\n\
# check the id, gender and fields; to refuse, delete the block. The tool never approves.\n";

const CLASSES: [&str; 3] = ["type", "verb", "adjective"];

pub struct Proposal {
    pub class: &'static str,
    pub form: String,
    pub id: String,
    pub meaning: String,
    pub evidence: Option<String>,
    /// the finished entry (without the `# `)
    pub entry: String,
}

/// A proposal read from the pending file.
#[derive(Debug, PartialEq)]
pub struct Pending {
    pub class: String,
    pub form: String,
    pub id: String,
    pub meaning: String,
    pub evidence: Option<String>,
    /// the `=...` line no longer has the `# ` (the human approved)
    pub approved: bool,
}

/// Lowercase, accents removed, only `a-z0-9`, everything else becomes `_`.
fn slug(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars().flat_map(|c| c.to_lowercase()) {
        let c = match c {
            'á' | 'à' | 'â' | 'ã' | 'ä' => 'a',
            'é' | 'ê' | 'è' => 'e',
            'í' | 'ì' => 'i',
            'ó' | 'ô' | 'õ' | 'ò' => 'o',
            'ú' | 'ù' | 'ü' => 'u',
            'ç' => 'c',
            c => c,
        };
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('_') && !out.is_empty() {
            out.push('_');
        }
    }
    out.trim_end_matches('_').to_string()
}

/// Base form (without the third-person `s`) of the first word: `redirects` -> `redirect`.
fn form_base(f: &str) -> Option<String> {
    let (w, rest) = f.split_once(' ').map(|(a, b)| (a, format!(" {b}"))).unwrap_or((f, String::new()));
    let base = if let Some(r) = w.strip_suffix("ies") {
        format!("{r}y")
    } else if ["ches", "shes", "sses", "xes", "zes", "oes"].iter().any(|s| w.ends_with(s)) {
        w[..w.len() - 2].to_string()
    } else if w.ends_with('s') && !w.ends_with("ss") && w.len() > 2 {
        w[..w.len() - 1].to_string()
    } else {
        return None;
    };
    Some(format!("{base}{rest}"))
}

fn form_is_valid(f: &str) -> bool {
    !f.is_empty()
        && f.split(' ').all(|w| {
            let mut cs = w.chars();
            cs.next().is_some_and(|c| c.is_ascii_alphabetic()) && cs.all(|c| c.is_ascii_alphabetic() || matches!(c, '/' | '_' | '-'))
        })
}

/// New id: the class prefix and the first English form, without repeating the lexicon ids or
/// those of the proposals already written.
fn id_new(lex: &Lexicon, class: &str, form: &str, used: &[String]) -> String {
    let prefix = match class {
        "type" => "e.",
        "verb" => "a.",
        _ => "adj.",
    };
    let base = format!("{prefix}{}", slug(form));
    let exists = |id: &str| {
        used.iter().any(|u| u == id)
            || match class {
                "type" => lex.kind(id).is_some(),
                "verb" => lex.verb(id).is_some(),
                _ => lex.adjective(id).is_some(),
            }
    };
    if !exists(&base) {
        return base;
    }
    (2..).map(|n| format!("{base}_{n}")).find(|c| !exists(c)).unwrap()
}

/// Builds the proposal, or says why it does not hold (form already approved or proposed, bad
/// fields). `pt` is the optional Portuguese label: without it the entry has no `pt:` and the
/// Portuguese renderer falls back to the English form.
#[allow(clippy::too_many_arguments)]
pub fn assemble(
    lex: &Lexicon,
    pending: &[Pending],
    class: &str,
    form: &str,
    pt: Option<&str>,
    meaning: &str,
    evidence: Option<&str>,
    gender: Option<&str>,
    id: Option<&str>,
) -> Result<Proposal, String> {
    let class: &'static str = CLASSES.iter().copied().find(|c| *c == class).ok_or_else(|| format!("unknown class '{class}' (use type, verb or adjective)"))?;
    let single_space = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ");
    let (form, meaning) = (single_space(form).to_lowercase(), single_space(meaning));
    let pt = pt.map(single_space);
    let forms: Vec<String> = form.split(',').map(|f| f.trim().to_string()).filter(|f| !f.is_empty()).collect();
    if forms.is_empty() || forms.iter().any(|f| !form_is_valid(f)) {
        return Err(format!("invalid English form: '{form}' (words with only letters, '-', '/' and '_'; several forms separated by commas)"));
    }
    if let Some(pt) = &pt {
        if pt.is_empty() || pt.contains(" | ") || pt.contains('\n') {
            return Err("--pt requires a Portuguese label (without ' | ')".into());
        }
    }
    if meaning.is_empty() {
        return Err("--meaning requires what the word means".into());
    }
    for f in &forms {
        if let Some(t) = lex.sentences.get(f) {
            let locate = match t {
                Tok::Noun(i) => format!("type {i}"),
                Tok::Verb(i) | Tok::VerbNeg(i) => format!("verb {i}"),
                Tok::Adj(i) => format!("adjective {i}"),
                other => other.class().to_string(),
            };
            return Err(format!("'{f}' is already an approved lexicon form ({locate}): use it"));
        }
        if pending.iter().any(|p| p.class == class && p.form.split(',').any(|x| x.trim() == f)) {
            return Err(format!("'{f}' is already proposed in {FILE}: awaiting your approval"));
        }
    }
    let used: Vec<String> = pending.iter().map(|p| p.id.clone()).collect();
    let id = match id {
        Some(i) => i.to_string(),
        None => id_new(lex, class, &forms[0], &used),
    };
    let entry = match class {
        "type" => {
            if let Some(g) = gender {
                if g != "m" && g != "f" {
                    return Err(format!("--g '{g}' (expected m or f)"));
                }
            }
            let mut e = format!("=type {id} | en: {}", forms.join(", "));
            if let Some(pt) = &pt {
                e.push_str(&format!(" | pt: {pt}"));
            }
            match (gender, &pt) {
                (Some(g), _) => e.push_str(&format!(" | g: {g}")),
                (None, Some(_)) => e.push_str(" | g: m"),
                (None, None) => {}
            }
            e.push_str(" | layer: element");
            e
        }
        "verb" => {
            let mut en = forms.clone();
            if en.len() == 1 {
                en.extend(form_base(&en[0]));
            }
            let mut e = format!("=verb {id} | en: {}", en.join(", "));
            if let Some(pt) = &pt {
                e.push_str(&format!(" | pt: {pt}"));
            }
            e
        }
        _ => {
            let mut e = format!("=adjective {id} | en: {}", forms.join(", "));
            if let Some(pt) = &pt {
                // two Portuguese forms (masculine, feminine): a single one serves both
                let pts = if pt.contains(", ") { pt.clone() } else { format!("{pt}, {pt}") };
                e.push_str(&format!(" | pt: {pts}"));
            }
            e
        }
    };
    Ok(Proposal { class, form: forms.join(", "), id, meaning, evidence: evidence.map(single_space), entry })
}

impl Proposal {
    /// The block that goes into the pending file.
    pub fn block(&self) -> String {
        let mut b = format!("# pending: {} \"{}\" (suggested id {})\n# meaning: {}\n", self.class, self.form, self.id, self.meaning);
        if let Some(e) = &self.evidence {
            b.push_str(&format!("# evidence: {e}\n"));
        }
        b.push_str(&format!("# {}\n", self.entry));
        b
    }
}

/// Proposals from the text of the pending file.
pub fn read_pending(text: &str) -> Vec<Pending> {
    let mut v: Vec<Pending> = Vec::new();
    for l in text.lines() {
        let t = l.trim();
        if let Some(r) = t.strip_prefix("# pending: ") {
            // class "form" (suggested id X)
            let Some((class, r)) = r.split_once(' ') else { continue };
            let Some(r) = r.strip_prefix('"') else { continue };
            let Some((form, r)) = r.split_once("\" (suggested id ") else { continue };
            v.push(Pending {
                class: class.to_string(),
                form: form.to_string(),
                id: r.trim_end_matches(')').to_string(),
                meaning: String::new(),
                evidence: None,
                approved: false,
            });
        } else if let Some(p) = v.last_mut() {
            if let Some(s) = t.strip_prefix("# meaning: ") {
                p.meaning = s.to_string();
            } else if let Some(e) = t.strip_prefix("# evidence: ") {
                p.evidence = Some(e.to_string());
            } else if t.starts_with(&format!("={} {}", p.class, p.id)) {
                p.approved = true;
            }
        }
    }
    v
}

fn read_file(fs: &dyn Fs, base: &Path) -> Vec<Pending> {
    fs.read_to_string(&base.join(FILE)).map(|t| read_pending(&t)).unwrap_or_default()
}

/// The pending file as it appears in messages: the root-relative path, or with the `--file`
/// directory in front.
fn label(root: &Path, base: &Path) -> String {
    if base == root {
        FILE.to_string()
    } else {
        base.join(FILE).display().to_string()
    }
}

/// `lexicon propose`: writes the proposal to the pending file of `base` (the root, or the
/// directory of `--file`). Never touches the project lexicon.
pub fn propose(fs: &dyn Fs, base: &Path, lex: &Lexicon, args: &[String]) -> Result<String, String> {
    let (mut pt, mut meaning, mut evidence, mut gender, mut id) = (None, None, None, None, None);
    let mut pos: Vec<&String> = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut value = |n: &str| it.next().cloned().ok_or_else(|| format!("{n} requires a value"));
        match a.as_str() {
            "--pt" => pt = Some(value("--pt")?),
            "--meaning" => meaning = Some(value("--meaning")?),
            "--evidence" => evidence = Some(value("--evidence")?),
            "--g" => gender = Some(value("--g")?),
            "--id" => id = Some(value("--id")?),
            o if o.starts_with("--") => return Err(format!("unknown option: {o}")),
            _ => pos.push(a),
        }
    }
    let [class, form] = pos.as_slice() else {
        return Err("usage: lexicon propose type|verb|adjective <en form> --meaning \"<text>\" [--pt \"<label>\"] [--evidence \"<file/sentence>\"] [--g m|f] [--id <id>]".into());
    };
    let meaning = meaning.ok_or("--meaning requires what the word means")?;
    let existing = read_file(fs, base);
    let p = assemble(lex, &existing, class, form, pt.as_deref(), &meaning, evidence.as_deref(), gender.as_deref(), id.as_deref())?;
    let path = base.join(FILE);
    let mut text = fs.read_to_string(&path).unwrap_or_default();
    if text.is_empty() {
        text.push_str(HEADER);
    }
    if !text.ends_with('\n') {
        text.push('\n');
    }
    text.push('\n');
    text.push_str(&p.block());
    fs.write(&path, &text)?;
    Ok(format!(
        "proposal written to {}: {} {} ({})\nit stays pending until you remove the \"# \" from the \"=...\" line; the tool does not approve\n",
        label(fs.root(), base),
        p.class, p.id, p.form
    ))
}

/// `lexicon pending`: lists the proposals (those already approved by hand are shown apart).
pub fn pending(fs: &dyn Fs, base: &Path) -> String {
    let all = read_file(fs, base);
    let file = label(fs.root(), base);
    let (approved, pending): (Vec<&Pending>, Vec<&Pending>) = all.iter().partition(|p| p.approved);
    if pending.is_empty() {
        let extra = if approved.is_empty() { String::new() } else { format!(" ({} already approved by hand: you can delete the block)", approved.len()) };
        return format!("no pending proposals{extra}\n");
    }
    let mut o = format!("{} pending proposal(s) in {file}\n", pending.len());
    for p in &pending {
        let ev = p.evidence.as_ref().map(|e| format!(" [evidence: {e}]")).unwrap_or_default();
        o.push_str(&format!("  {} \"{}\" {}: {}{ev}\n", p.class, p.form, p.id, p.meaning));
    }
    if !approved.is_empty() {
        o.push_str(&format!("{} already approved by hand (\"=...\" line without the \"# \"): you can delete the block\n", approved.len()));
    }
    o.push_str("approving is up to you: remove the \"# \" from the proposal's \"=...\" line; the tool does not approve\n");
    o
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lex() -> Lexicon {
        Lexicon::embedded().unwrap()
    }

    #[test]
    fn ids_without_accents_and_without_repeating() {
        let l = lex();
        assert_eq!(id_new(&l, "type", "http route", &[]), "e.http_route");
        assert_eq!(id_new(&l, "verb", "bounces", &["a.bounces".into()]), "a.bounces_2");
        assert_eq!(id_new(&l, "verb", "uses", &[]), "a.uses_2");
        assert_eq!(id_new(&l, "adjective", "sortable", &[]), "adj.sortable");
    }

    #[test]
    fn base_form_of_the_third_person() {
        assert_eq!(form_base("redirects").as_deref(), Some("redirect"));
        assert_eq!(form_base("tries up to").as_deref(), Some("try up to"));
        assert_eq!(form_base("pushes").as_deref(), Some("push"));
        assert_eq!(form_base("responds with").as_deref(), Some("respond with"));
        assert_eq!(form_base("pass"), None);
    }

    #[test]
    fn entries_ready_to_uncomment_pass_the_lexicon_checks() {
        let l = lex();
        for (class, form, pt, g) in [
            ("type", "gadget", Some("engenhoca"), Some("f")),
            ("verb", "bounces", Some("rebate"), None),
            ("adjective", "sortable", Some("ordenável"), None),
            ("type", "widget", None, None),
            ("verb", "ricochets", None, None),
            ("adjective", "sortable", None, None),
        ] {
            let p = assemble(&l, &[], class, form, pt, "x", None, g, None).unwrap();
            let e = super::super::lexicon::Entry { label: "p".into(), text: &p.entry };
            Lexicon::load(&[e]).unwrap_or_else(|er| panic!("{}: {er:?}", p.entry));
        }
    }

    #[test]
    fn proposal_without_pt_omits_the_label() {
        let l = lex();
        let p = assemble(&l, &[], "type", "widget", None, "x", None, None, None).unwrap();
        assert_eq!(p.entry, "=type e.widget | en: widget | layer: element");
        let p = assemble(&l, &[], "verb", "bounces", None, "x", None, None, None).unwrap();
        assert_eq!(p.entry, "=verb a.bounces | en: bounces, bounce");
        let p = assemble(&l, &[], "type", "gadget", Some("engenhoca"), "x", None, None, None).unwrap();
        assert_eq!(p.entry, "=type e.gadget | en: gadget | pt: engenhoca | g: m | layer: element");
    }

    #[test]
    fn block_read_back_and_manual_approval() {
        let l = lex();
        let p = assemble(&l, &[], "verb", "bounces", Some("rebate"), "reply with a redirect", Some("shortener/server.py"), None, None).unwrap();
        let entries = read_pending(&p.block());
        assert_eq!(
            entries,
            vec![Pending {
                class: "verb".into(),
                form: "bounces".into(),
                id: "a.bounces".into(),
                meaning: "reply with a redirect".into(),
                evidence: Some("shortener/server.py".into()),
                approved: false
            }]
        );
        let approved = p.block().replace("# =verb", "=verb");
        assert!(read_pending(&approved)[0].approved);
    }

    #[test]
    fn refuses_an_approved_form_and_bad_fields() {
        let l = lex();
        let e = assemble(&l, &[], "verb", "reads", Some("lê"), "x", None, None, None).err().unwrap();
        assert!(e.contains("'reads' is already an approved lexicon form (verb a.reads)"), "{e}");
        assert!(assemble(&l, &[], "thing", "x", None, "x", None, None, None).is_err());
        assert!(assemble(&l, &[], "type", "ro ta1", None, "x", None, None, None).is_err());
        assert!(assemble(&l, &[], "type", "gizmo", Some(""), "x", None, None, None).is_err());
        assert!(assemble(&l, &[], "type", "gizmo", None, "", None, None, None).is_err());
        assert!(assemble(&l, &[], "type", "gizmo", Some("traquitana"), "x", None, Some("n"), None).is_err());
    }
}
