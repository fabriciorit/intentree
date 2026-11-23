//! Anchors with a symbol (`^path#symbol`): presence of the word and definition per language.

use crate::facts::{mask, Lang};
use crate::glob::{has_wild, Glob};
use crate::vfs::Fs;
use std::path::Path;

/// Splits an anchor (without the `^`) into (path, symbol).
pub fn part_anchor(a: &str) -> (&str, Option<&str>) {
    match a.split_once('#') {
        Some((c, s)) => (c, Some(s)),
        None => (a, None),
    }
}

fn ident(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// Segments of a symbol (`a::b`, `A.b`); `None` if malformed.
pub fn segments(s: &str) -> Option<Vec<&str>> {
    let norm: Vec<&str> = s.split("::").flat_map(|x| x.split('.')).collect();
    if norm.is_empty() || norm.iter().any(|x| x.is_empty() || !x.chars().all(ident)) {
        return None;
    }
    Some(norm)
}

/// The word `name` appears whole (with no letters/digits/`_` stuck to it) in `t`.
pub fn has_word(t: &str, name: &str) -> bool {
    t.match_indices(name).any(|(i, _)| {
        let before = t[..i].chars().next_back().is_none_or(|c| !ident(c));
        let after = t[i + name.len()..].chars().next().is_none_or(|c| !ident(c));
        before && after
    })
}

/// `s` starts with the word `p` (followed by a non-identifier); returns the rest.
fn word<'a>(s: &'a str, p: &str) -> Option<&'a str> {
    let r = s.strip_prefix(p)?;
    if r.chars().next().is_some_and(ident) {
        return None;
    }
    Some(r)
}

/// `s` (already without leading spaces) starts exactly with the identifier `name`.
fn starts_with_name(s: &str, name: &str) -> bool {
    word(s, name).is_some()
}

/// `kw` followed by a space and `name`.
fn kw_name(s: &str, kw: &str, name: &str) -> bool {
    match word(s, kw) {
        Some(r) if r.starts_with(char::is_whitespace) => starts_with_name(r.trim_start(), name),
        _ => false,
    }
}

fn rust_line(l: &str, name: &str) -> bool {
    let mut s = l.trim_start();
    if let Some(r) = word(s, "pub") {
        s = r.trim_start();
        if s.starts_with('(') {
            if let Some(k) = s.find(')') {
                s = s[k + 1..].trim_start();
            }
        }
    }
    loop {
        if ["fn", "struct", "enum", "trait", "type", "const", "static", "mod", "union"].iter().any(|k| kw_name(s, k, name)) {
            return true;
        }
        if let Some(r) = word(s, "static") {
            if let Some(r) = word(r.trim_start(), "mut") {
                if starts_with_name(r.trim_start(), name) {
                    return true;
                }
            }
        }
        if let Some(r) = word(s, "macro_rules") {
            if let Some(r) = r.trim_start().strip_prefix('!') {
                if starts_with_name(r.trim_start(), name) {
                    return true;
                }
            }
        }
        let mut advanced = false;
        for q in ["async", "unsafe", "const", "default", "extern"] {
            if let Some(r) = word(s, q) {
                s = r.trim_start();
                if q == "extern" && s.starts_with('"') {
                    if let Some(k) = s[1..].find('"') {
                        s = s[k + 2..].trim_start();
                    }
                }
                advanced = true;
                break;
            }
        }
        if !advanced {
            return false;
        }
    }
}

fn ts_decl(l: &str, name: &str) -> bool {
    let mut s = l.trim_start();
    for q in ["export", "default", "declare", "async"] {
        while let Some(r) = word(s, q) {
            s = r.trim_start();
        }
    }
    ["function", "class", "const", "let", "var", "interface", "type", "enum"].iter().any(|k| kw_name(s, k, name))
}

/// Method `name(` at the start of the line, with `{` on the same line or the next one.
fn ts_method(l: &str, next: &str, name: &str) -> bool {
    let mut s = l.trim_start();
    loop {
        let mut advanced = false;
        for q in ["async", "static", "get", "set", "public", "private", "protected", "readonly", "override"] {
            if let Some(r) = word(s, q) {
                // `get(`/`set(` would be the method's own name
                if !r.trim_start().starts_with('(') {
                    s = r.trim_start();
                    advanced = true;
                    break;
                }
            }
        }
        if !advanced {
            break;
        }
    }
    let Some(r) = word(s, name) else { return false };
    let Some(r) = r.trim_start().strip_prefix('(') else { return false };
    r.contains('{') || (!r.contains(';') && next.trim_start().starts_with('{'))
}

fn py_line(l: &str, name: &str) -> bool {
    let s = l.trim_start();
    let s2 = word(s, "async").map(str::trim_start).unwrap_or(s);
    if kw_name(s2, "def", name) || kw_name(s, "class", name) {
        return true;
    }
    if let Some(r) = word(l, name) {
        let r = r.trim_start();
        return r.starts_with('=') && !r.starts_with("==");
    }
    false
}

fn sh_line(l: &str, name: &str) -> bool {
    let s = l.trim_start();
    if kw_name(s, "function", name) {
        return true;
    }
    match word(s, name) {
        Some(r) => r.trim_start().strip_prefix('(').is_some_and(|x| x.trim_start().starts_with(')')),
        None => false,
    }
}

/// Some line of `src` (file `file`) defines `name`. With no known language: presence of the word.
pub fn define(file: &str, src: &str, name: &str) -> bool {
    let ext = file.rsplit('.').next().unwrap_or("");
    match ext {
        "rs" => mask(src, Lang::Rs).lines().any(|l| rust_line(l, name)),
        "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs" | "svelte" => {
            let m = mask(src, Lang::Ts);
            let ls: Vec<&str> = m.lines().collect();
            ls.iter().enumerate().any(|(i, l)| ts_decl(l, name) || ts_method(l, ls.get(i + 1).copied().unwrap_or(""), name))
        }
        "py" => mask(src, Lang::Py).lines().any(|l| py_line(l, name)),
        "sh" | "bash" => mask(src, Lang::Py).lines().any(|l| sh_line(l, name)),
        _ => has_word(src, name),
    }
}

/// Text used to look for the presence of the word (comments masked in the known languages).
pub fn text_search(file: &str, src: &str) -> String {
    match file.rsplit('.').next().unwrap_or("") {
        "rs" => mask(src, Lang::Rs),
        "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs" | "svelte" => mask(src, Lang::Ts),
        "py" | "sh" | "bash" => mask(src, Lang::Py),
        _ => src.to_string(),
    }
}

#[derive(Debug, PartialEq)]
pub enum Verdict {
    Ok,
    /// Appears, but no definition matches.
    OnlyQuoted,
    /// The word (the indicated segment) does not exist in any file.
    Absent(String),
}

/// Evaluates the symbol (already in segments) over the files read (name, content).
pub fn evaluate(segs: &[&str], files: &[(String, String)]) -> Verdict {
    let (last, qualifiers) = segs.split_last().expect("non-empty symbol");
    let texts: Vec<(&String, String)> = files.iter().map(|(n, s)| (n, text_search(n, s))).collect();
    let mut quoted = false;
    for ((n, src), (_, t)) in files.iter().zip(&texts) {
        if segs.iter().all(|s| has_word(t, s)) {
            quoted = true;
            if define(n, src, last) {
                return Verdict::Ok;
            }
        }
    }
    if quoted {
        return Verdict::OnlyQuoted;
    }
    let missing = qualifiers
        .iter()
        .chain(std::iter::once(last))
        .find(|s| !texts.iter().any(|(_, t)| has_word(t, s)))
        .unwrap_or(last);
    Verdict::Absent(missing.to_string())
}

pub fn hint_glob(g: &str, fs: &dyn Fs, root: &Path) -> String {
    if !has_wild(g) {
        let p = root.join(g.trim_end_matches('/'));
        if fs.is_dir(&p) {
            return format!(" (it is a directory; use {}/**)", g.trim_end_matches('/'));
        }
        if fs.exists(&p) {
            return " (the path exists on disk, but is ignored by git)".to_string();
        }
    }
    String::new()
}

/// Checks an anchor (`path` or `path#symbol`, without the `^`, with glob) against the project's
/// files. Returns the indices of the matched files and the messages (error?, text).
pub fn check_anchor(anchor: &str, files: &[String], fs: &dyn Fs, root: &Path) -> (Vec<usize>, Vec<(bool, String)>) {
    let mut msgs = Vec::new();
    let (a, symbol) = part_anchor(anchor);
    let g = Glob::new(a);
    let matched: Vec<usize> = (0..files.len()).filter(|&i| g.matches(&files[i])).collect();
    if matched.is_empty() {
        msgs.push((true, format!("anchor matches no file: {a}{}", hint_glob(a, fs, root))));
        return (matched, msgs);
    }
    if let Some(sym) = symbol {
        match segments(sym) {
            None => msgs.push((true, format!("anchor: invalid symbol in {anchor}: use identifiers separated by :: or ."))),
            Some(segs) => {
                let contents: Vec<(String, String)> = matched
                    .iter()
                    .filter_map(|&i| {
                        let b = fs.read(&root.join(&files[i]))?;
                        Some((files[i].clone(), String::from_utf8_lossy(&b).to_string()))
                    })
                    .collect();
                if !contents.is_empty() {
                    match evaluate(&segs, &contents) {
                        Verdict::Ok => {}
                        Verdict::OnlyQuoted => msgs.push((false, format!("anchor: {sym} appears in {a} but not as a definition"))),
                        Verdict::Absent(n) => msgs.push((true, format!("anchor: symbol {n} does not exist in {a}"))),
                    }
                }
            }
        }
    }
    (matched, msgs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn part_and_segments() {
        assert_eq!(part_anchor("a.rs#f"), ("a.rs", Some("f")));
        assert_eq!(part_anchor("a.rs#T::m"), ("a.rs", Some("T::m")));
        assert_eq!(part_anchor("a.py#Class.method"), ("a.py", Some("Class.method")));
        assert_eq!(part_anchor("src/**"), ("src/**", None));
        assert_eq!(segments("T::m"), Some(vec!["T", "m"]));
        assert_eq!(segments("Class.method"), Some(vec!["Class", "method"]));
        assert_eq!(segments("a b"), None);
        assert_eq!(segments(""), None);
        assert_eq!(segments("a::"), None);
    }

    #[test]
    fn definitions_by_language() {
        let rs = "// fn ghost\npub(crate) async fn real() {}\nimpl T {\n    pub const fn m() {}\n}\nmacro_rules! mac { () => {} }\nstatic mut G: u8 = 0;\nlet x = almost;\n";
        assert!(define("a.rs", rs, "real") && define("a.rs", rs, "m") && define("a.rs", rs, "mac") && define("a.rs", rs, "G"));
        assert!(!define("a.rs", rs, "ghost") && !define("a.rs", rs, "almost"));
        let ts = "export default function alpha() {}\nconst beta = () => 1;\nclass K {\n  static async gamma(x) {\n  }\n  delta(a)\n  {\n  }\n  calls(x);\n}\n";
        for n in ["alpha", "beta", "K", "gamma", "delta"] {
            assert!(define("a.ts", ts, n), "{n}");
        }
        assert!(!define("a.ts", ts, "calls"));
        let py = "def f(): pass\nasync def g(): pass\nclass C:\n    def met(self): pass\nVALUE = 3\nx == 2\n# def dead\n";
        for n in ["f", "g", "C", "met", "VALUE"] {
            assert!(define("a.py", py, n), "{n}");
        }
        assert!(!define("a.py", py, "dead") && !define("a.py", py, "x"));
        let sh = "hello() {\n}\nfunction bye {\n}\nonlyQuoted\n";
        assert!(define("a.sh", sh, "hello") && define("a.sh", sh, "bye") && !define("a.sh", sh, "onlyQuoted"));
        assert!(define("a.txt", "any thing here", "thing"));
    }

    #[test]
    fn verdict() {
        let a = vec![("a.rs".to_string(), "struct T;\nimpl T {\n    fn m() {}\n}\nfn usage() { quoted(); }\n".to_string())];
        assert_eq!(evaluate(&["T", "m"], &a), Verdict::Ok);
        assert_eq!(evaluate(&["quoted"], &a), Verdict::OnlyQuoted);
        assert_eq!(evaluate(&["nothing"], &a), Verdict::Absent("nothing".into()));
        assert_eq!(evaluate(&["Other", "m"], &a), Verdict::Absent("Other".into()));
    }
}
