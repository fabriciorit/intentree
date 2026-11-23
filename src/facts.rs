//! Facts extracted from the code by text scanning (no parser): dependencies, embeds,
//! HTTP calls, invoked programs, disk writes, routes and environment variables.

use crate::glob::expand;
use crate::intent::json::text as jstr;
use crate::intent::project::Project;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fmt::Write;
use crate::vfs::Fs;
use std::path::Path;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Kind {
    Dep,
    Embed,
    Http,
    Invoke,
    Write,
    Route,
    Env,
    Device,
}

impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Kind::Dep => "dep",
            Kind::Embed => "embed",
            Kind::Http => "http",
            Kind::Invoke => "invoke",
            Kind::Write => "write",
            Kind::Route => "route",
            Kind::Env => "env",
            Kind::Device => "device",
        }
    }
}

/// Raw occurrence, before path resolution.
#[derive(Debug, Clone, PartialEq)]
pub enum Raw {
    /// Rust path: `crate::a::b`, `super::a` or `other_crate::x`.
    UseRust(String),
    Program(String),
    IncludeStr(String),
    Dir(String),
    Route(String),
    Write,
    Env(String),
    /// String literal that starts with `/dev/`, `/sys/` or `/proc/`.
    Device(String),
    /// Relative TS/JS/Svelte specifier.
    Import(String),
    Fetch(String),
    /// Python: module (with leading dots if relative) and imported names.
    PyMod(String, Vec<String>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Oc {
    pub raw: Raw,
    pub line: usize,
}

// ---------- text scanning ----------

fn ident(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

fn line_starts(s: &str) -> Vec<usize> {
    let mut v = vec![0];
    v.extend(s.match_indices('\n').map(|(i, _)| i + 1));
    v
}

fn line(starts: &[usize], pos: usize) -> usize {
    match starts.binary_search(&pos) {
        Ok(i) => i + 1,
        Err(i) => i,
    }
}

fn blank_out(o: &mut [u8], from: usize, upto: usize) {
    for x in o.iter_mut().take(upto).skip(from) {
        if *x != b'\n' {
            *x = b' ';
        }
    }
}

/// Language for the mask: changes what counts as a comment and what as a literal.
#[derive(Clone, Copy, PartialEq)]
pub enum Lang {
    Rs,
    Ts,
    /// Python and shell (`#` comments).
    Py,
}

/// Replaces comments with spaces (keeps line breaks, hence the line numbers).
/// Knows string literals: `"/**"` inside a string does not open a block.
pub fn mask(s: &str, l: Lang) -> String {
    scan(s, l).0
}

/// End (exclusive) of a literal closed by `closing`, starting at `k`, with `\` escapes.
fn until_closing(b: &[u8], mut k: usize, closing: u8, one_line: bool) -> usize {
    while k < b.len() {
        match b[k] {
            b'\\' => k += 2,
            c if c == closing => return k + 1,
            b'\n' if one_line => return k,
            _ => k += 1,
        }
    }
    b.len()
}

/// End (exclusive) of a triple-quoted literal starting at `k`.
fn until_triple(b: &[u8], mut k: usize, q: u8) -> usize {
    while k < b.len() {
        if b[k] == b'\\' {
            k += 2;
        } else if b[k] == q && b.get(k + 1) == Some(&q) && b.get(k + 2) == Some(&q) {
            return k + 3;
        } else {
            k += 1;
        }
    }
    b.len()
}

/// End (exclusive) of the literal that opens at `b[i]`, or None if `i` does not open a literal.
fn end_literal(b: &[u8], i: usize, l: Lang) -> Option<usize> {
    let q = b[i];
    let triple = b.get(i + 1) == Some(&q) && b.get(i + 2) == Some(&q);
    match (l, q) {
        (Lang::Py, b'"' | b'\'') => Some(if triple { until_triple(b, i + 3, q) } else { until_closing(b, i + 1, q, true) }),
        (Lang::Rs, b'"') => {
            // raw string: r"..." / r#"..."# / br"..."; the r prefix cannot be part of an identifier
            let mut j = i;
            while j > 0 && b[j - 1] == b'#' {
                j -= 1;
            }
            let hashes = i - j;
            let raw = j > 0 && b[j - 1] == b'r' && (j == 1 || !ident(b[j - 2]) || (b[j - 2] == b'b' && (j == 2 || !ident(b[j - 3]))));
            if !raw {
                return Some(until_closing(b, i + 1, b'"', false));
            }
            let mut k = i + 1;
            while k < b.len() {
                if b[k] == b'"' && b[k + 1..].iter().take_while(|&&c| c == b'#').count() >= hashes {
                    return Some(k + 1 + hashes);
                }
                k += 1;
            }
            Some(b.len())
        }
        (Lang::Rs, b'\'') => {
            // char literal ('x', '\n', '\u{..}', multibyte); a lifetime ('a) is not a literal
            if b.get(i + 1) == Some(&b'\\') {
                return Some(until_closing(b, i + 2, b'\'', true));
            }
            let w = match b.get(i + 1)? {
                c if *c < 0x80 => 1,
                c if *c >= 0xF0 => 4,
                c if *c >= 0xE0 => 3,
                _ => 2,
            };
            (b.get(i + 1 + w) == Some(&b'\'')).then_some(i + 2 + w)
        }
        (Lang::Ts, b'"' | b'\'') => Some(until_closing(b, i + 1, q, true)),
        (Lang::Ts, b'`') => Some(until_closing(b, i + 1, b'`', false)),
        _ => None,
    }
}

/// Text without comments and, per byte, whether it is inside a string literal (after the
/// opening quote, closing quote included; the opening quote counts as code).
pub fn scan(s: &str, l: Lang) -> (String, Vec<bool>) {
    let b = s.as_bytes();
    let n = b.len();
    let mut o = b.to_vec();
    let mut in_str = vec![false; n];
    let mut i = 0;
    while i < n {
        if let Some(end) = end_literal(b, i, l) {
            let end = end.min(n).max(i + 1);
            for e in in_str.iter_mut().take(end).skip(i + 1) {
                *e = true;
            }
            i = end;
            continue;
        }
        if l == Lang::Py {
            if b[i] == b'#' {
                let end = s[i..].find('\n').map(|k| i + k).unwrap_or(n);
                let start_line = s[..i].rfind('\n').map(|k| k + 1).unwrap_or(0);
                if s[start_line..i].trim().is_empty() {
                    blank_out(&mut o, i, end);
                }
                i = end;
                continue;
            }
        } else if b[i] == b'/' && i + 1 < n && (i == 0 || matches!(b[i - 1], b' ' | b'\t' | b'\n' | b'\r' | b';' | b'{' | b'}' | b'(' | b')' | b',')) {
            if b[i + 1] == b'/' {
                let end = s[i..].find('\n').map(|k| i + k).unwrap_or(n);
                blank_out(&mut o, i, end);
                i = end;
                continue;
            }
            if b[i + 1] == b'*' {
                let end = s[i + 2..].find("*/").map(|k| i + 2 + k + 2).unwrap_or(n);
                blank_out(&mut o, i, end);
                i = end;
                continue;
            }
        }
        i += 1;
    }
    (String::from_utf8(o).unwrap_or_default(), in_str)
}

/// Positions of `pat`; with `bounded`, only when the previous character is not an identifier character.
fn find(t: &str, in_str: &[bool], pat: &str, limit: bool) -> Vec<usize> {
    let b = t.as_bytes();
    t.match_indices(pat).map(|(i, _)| i).filter(|&i| !in_str[i] && (!limit || i == 0 || !ident(b[i - 1]))).collect()
}

fn skip_ws(t: &str, mut i: usize) -> usize {
    let b = t.as_bytes();
    while i < b.len() && b[i].is_ascii_whitespace() {
        i += 1;
    }
    i
}

/// Quoted string (`"`, `'` or backtick) starting at `i`; only a backtick string spans lines.
fn read_str(t: &str, i: usize) -> Option<String> {
    let b = t.as_bytes();
    let q = *b.get(i)?;
    if !matches!(q, b'"' | b'\'' | b'`') {
        return None;
    }
    let end = t[i + 1..].find(q as char)?;
    let s = &t[i + 1..i + 1 + end];
    if q != b'`' && s.contains('\n') {
        return None;
    }
    Some(s.to_string())
}

/// String right after `pos`, ignoring spaces.
fn arg_str(t: &str, pos: usize) -> Option<String> {
    read_str(t, skip_ws(t, pos))
}

fn read_ident(t: &str, i: usize) -> usize {
    let b = t.as_bytes();
    let mut k = i;
    while k < b.len() && ident(b[k]) {
        k += 1;
    }
    k
}

fn basename(p: &str) -> &str {
    p.rsplit('/').next().unwrap_or(p)
}

const PREFIXES_DEV: [&str; 3] = ["/dev/", "/sys/", "/proc/"];

/// String literals that start with `/dev/`, `/sys/` or `/proc/` (text already without comments).
fn devices(t: &str, in_str: &[bool], starts: &[usize]) -> Vec<Oc> {
    let mut out = Vec::new();
    for (i, &q) in t.as_bytes().iter().enumerate() {
        if in_str[i] || !matches!(q, b'"' | b'\'' | b'`') {
            continue;
        }
        let r = &t[i + 1..];
        // the bare prefix ("/dev/") is a search pattern, not a device
        if !PREFIXES_DEV.iter().any(|p| r.starts_with(p) && !r[p.len()..].starts_with(q as char)) {
            continue;
        }
        let Some(end) = r.find(q as char) else { continue };
        if q != b'`' && r[..end].contains('\n') {
            continue;
        }
        out.push(Oc { raw: Raw::Device(r[..end].to_string()), line: line(starts, i) });
    }
    out
}

// ---------- Rust ----------

/// Strips `as x` and spaces from a `use` body; None if it does not look like a path.
fn normalize_use(c: &str) -> Option<String> {
    let mut o = String::new();
    let mut skip = false;
    for t in c.split_whitespace() {
        if skip {
            skip = false;
            o.push_str(t.trim_start_matches(|ch: char| ch.is_ascii_alphanumeric() || ch == '_'));
            continue;
        }
        if t == "as" {
            skip = true;
            continue;
        }
        o.push_str(t);
    }
    let ok = !o.is_empty() && o.chars().all(|c| c.is_ascii_alphanumeric() || "_:{},*".contains(c));
    ok.then_some(o)
}

/// Blanks `#[cfg(test)]` blocks (the next item, up to the closing brace): use only in tests
/// is not a production dependency.
fn without_tests(s: &str) -> String {
    let b = s.as_bytes();
    let mut o = b.to_vec();
    let mut from = 0;
    while let Some(k) = s[from..].find("#[cfg(test)]") {
        let start = from + k;
        let Some(open_brace) = s[start..].find('{').map(|x| start + x) else { break };
        // a one-line attribute (e.g. `#[cfg(test)] use x;`) ends in `;` before the brace
        if let Some(pv) = s[start..open_brace].find(';') {
            blank_out(&mut o, start, start + pv + 1);
            from = start + pv + 1;
            continue;
        }
        let mut level = 0i32;
        let mut end = b.len();
        for (j, &c) in b.iter().enumerate().skip(open_brace) {
            if c == b'{' {
                level += 1;
            } else if c == b'}' {
                level -= 1;
                if level == 0 {
                    end = j + 1;
                    break;
                }
            }
        }
        blank_out(&mut o, start, end);
        from = end;
    }
    String::from_utf8(o).unwrap_or_default()
}

pub fn extract_rust(src: &str, crates: &[String]) -> Vec<Oc> {
    let (masked, in_str) = scan(src, Lang::Rs);
    let clean = without_tests(&masked);
    let starts = line_starts(&clean);
    let b = clean.as_bytes();
    let mut out = Vec::new();
    let mut without_use = b.to_vec();
    let mut skip_until = 0;
    for pos in find(&clean, &in_str, "use", true) {
        let after = pos + 3;
        if pos < skip_until || after >= b.len() || !b[after].is_ascii_whitespace() {
            continue;
        }
        let Some(end) = clean[after..].find(';') else { continue };
        let Some(norm) = normalize_use(&clean[after..after + end]) else { continue };
        for c in expand(&norm) {
            let c = c.trim_end_matches("::*").trim_end_matches("::self");
            let segs: Vec<&str> = c.split("::").filter(|s| !s.is_empty()).collect();
            if let (Some(p), true) = (segs.first(), segs.len() > 1) {
                if *p == "crate" || *p == "super" || crates.iter().any(|n| n == p) {
                    out.push(Oc { raw: Raw::UseRust(segs.join("::")), line: line(&starts, pos) });
                }
            }
        }
        skip_until = after + end + 1;
        blank_out(&mut without_use, pos, skip_until);
    }
    let t = String::from_utf8(without_use).unwrap_or_default();
    let b = t.as_bytes();
    // crate::x::y and other_crate::x
    let mut i = 0;
    while i < b.len() {
        if !in_str[i] && (b[i].is_ascii_alphabetic() || b[i] == b'_') && (i == 0 || (!ident(b[i - 1]) && b[i - 1] != b':')) {
            let j = read_ident(&t, i);
            let tok = &t[i..j];
            if t[j..].starts_with("::") && (tok == "crate" || crates.iter().any(|n| n == tok)) {
                let mut segs = vec![tok.to_string()];
                let mut cur = j;
                while t[cur..].starts_with("::") {
                    let k = read_ident(&t, cur + 2);
                    if k == cur + 2 {
                        break;
                    }
                    segs.push(t[cur + 2..k].to_string());
                    cur = k;
                }
                if segs.len() > 1 {
                    out.push(Oc { raw: Raw::UseRust(segs.join("::")), line: line(&starts, i) });
                }
                i = cur.max(j);
                continue;
            }
            i = j;
        } else {
            i += 1;
        }
    }
    let mut scan_str = |pat: &str, limit: bool, f: &dyn Fn(String) -> Option<Raw>| {
        for pos in find(&t, &in_str, pat, limit) {
            if let Some(c) = arg_str(&t, pos + pat.len()).and_then(f) {
                out.push(Oc { raw: c, line: line(&starts, pos) });
            }
        }
    };
    scan_str("Command::new(", true, &|s| Some(Raw::Program(basename(&s).to_string())));
    scan_str("include_str!(", true, &|s| Some(Raw::IncludeStr(s)));
    scan_str("include_bytes!(", true, &|s| Some(Raw::IncludeStr(s)));
    scan_str(".route(", false, &|s| s.starts_with('/').then_some(Raw::Route(s)));
    scan_str("env::var(", true, &|s| Some(Raw::Env(s)));
    scan_str("env::var_os(", true, &|s| Some(Raw::Env(s)));
    for pos in find(&t, &in_str, "var_os(", true) {
        if pos == 0 || b[pos - 1] != b':' {
            if let Some(s) = arg_str(&t, pos + 7) {
                out.push(Oc { raw: Raw::Env(s), line: line(&starts, pos) });
            }
        }
    }
    // #[folder = "p"] (rust_embed)
    for pos in find(&t, &in_str, "folder", true) {
        let a = skip_ws(&t, pos + 6);
        if b.get(a) == Some(&b'=') {
            if let Some(s) = arg_str(&t, a + 1) {
                out.push(Oc { raw: Raw::Dir(s), line: line(&starts, pos) });
            }
        }
    }
    for pat in ["fs::write(", "File::create(", "OpenOptions::new(", "fs::rename("] {
        for pos in find(&t, &in_str, pat, true) {
            out.push(Oc { raw: Raw::Write, line: line(&starts, pos) });
        }
    }
    for pos in find(&t, &in_str, "create_dir", true) {
        let r = &t[pos + 10..];
        if r.starts_with('(') || r.starts_with("_all(") {
            out.push(Oc { raw: Raw::Write, line: line(&starts, pos) });
        }
    }
    out.extend(devices(&t, &in_str, &starts));
    out.sort_by_key(|o| o.line);
    out
}

// ---------- TS / JS / Svelte ----------

fn cut_url(s: &str) -> String {
    let end = s.find(['?', '#']).unwrap_or(s.len());
    let s = &s[..end];
    s[..s.find("${").unwrap_or(s.len())].to_string()
}

pub fn extract_ts(src: &str) -> Vec<Oc> {
    let (t, in_str) = scan(src, Lang::Ts);
    let starts = line_starts(&t);
    let mut out = Vec::new();
    let mut add = |pos: usize, c: Raw| out.push(Oc { raw: c, line: line(&starts, pos) });
    let rel = |s: String| s.starts_with('.').then_some(s);
    for pos in find(&t, &in_str, "from", true) {
        if let Some(s) = arg_str(&t, pos + 4).and_then(rel) {
            add(pos, Raw::Import(s));
        }
    }
    for pos in find(&t, &in_str, "import", true) {
        if let Some(s) = arg_str(&t, pos + 6).and_then(rel) {
            add(pos, Raw::Import(s)); // import './x'
        }
    }
    for pat in ["import(", "require(", "new URL("] {
        for pos in find(&t, &in_str, pat, true) {
            if let Some(s) = arg_str(&t, pos + pat.len()).and_then(rel) {
                add(pos, Raw::Import(s));
            }
        }
    }
    for pos in find(&t, &in_str, "fetch(", true) {
        if let Some(s) = arg_str(&t, pos + 6).filter(|s| s.starts_with('/')) {
            add(pos, Raw::Fetch(cut_url(&s)));
        }
    }
    for pat in ["process.env.", "import.meta.env."] {
        for pos in find(&t, &in_str, pat, true) {
            let a = pos + pat.len();
            let f = read_ident(&t, a);
            if f > a {
                add(pos, Raw::Env(t[a..f].to_string()));
            }
        }
    }
    for o in devices(&t, &in_str, &starts) {
        out.push(o);
    }
    out.sort_by_key(|o| o.line);
    out
}

// ---------- Python ----------

pub fn extract_py(src: &str) -> Vec<Oc> {
    let (t, in_str) = scan(src, Lang::Py);
    let starts = line_starts(&t);
    let mut out = Vec::new();
    let mod_ok = |m: &str| !m.is_empty() && m.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.');
    for (i, l) in t.lines().enumerate() {
        let body = l.trim_start();
        // a line that starts inside a triple-quoted string (docstring) is not code
        if starts.get(i).is_some_and(|&start| in_str.get(start + (l.len() - body.len())).copied().unwrap_or(false)) {
            continue;
        }
        let l = body;
        if let Some(r) = l.strip_prefix("from ") {
            if let Some((m, n)) = r.split_once(" import ") {
                let m = m.trim();
                let names: Vec<String> = n
                    .trim()
                    .trim_start_matches('(')
                    .split(',')
                    .filter_map(|x| x.split_whitespace().next())
                    .map(|x| x.trim_end_matches(')').to_string())
                    .filter(|x| mod_ok(x))
                    .collect();
                if mod_ok(m) && m != "__future__" {
                    out.push(Oc { raw: Raw::PyMod(m.to_string(), names), line: i + 1 });
                }
            }
        } else if let Some(r) = l.strip_prefix("import ") {
            for item in r.split(',') {
                if let Some(m) = item.split_whitespace().next().filter(|m| mod_ok(m)) {
                    out.push(Oc { raw: Raw::PyMod(m.to_string(), vec![]), line: i + 1 });
                }
            }
        }
    }
    for pos in find(&t, &in_str, "subprocess.", true) {
        let a = pos + 11;
        let f = read_ident(&t, a);
        if !["run", "Popen", "call", "check_output", "check_call"].contains(&&t[a..f]) {
            continue;
        }
        let b = t.as_bytes();
        let mut k = skip_ws(&t, f);
        if b.get(k) != Some(&b'(') {
            continue;
        }
        k = skip_ws(&t, k + 1);
        if b.get(k) == Some(&b'[') {
            k = skip_ws(&t, k + 1);
        }
        if let Some(s) = read_str(&t, k) {
            out.push(Oc { raw: Raw::Program(basename(&s).to_string()), line: line(&starts, pos) });
        }
    }
    for pat in ["os.environ[", "os.environ.get(", "os.getenv("] {
        for pos in find(&t, &in_str, pat, true) {
            if let Some(s) = arg_str(&t, pos + pat.len()) {
                out.push(Oc { raw: Raw::Env(s), line: line(&starts, pos) });
            }
        }
    }
    out.extend(devices(&t, &in_str, &starts));
    out.sort_by_key(|o| o.line);
    out
}

// ---------- shell

const PROGRAMS_SH: [&str; 22] = [
    "ssh", "scp", "rsync", "systemctl", "journalctl", "openssl", "cargo", "npm", "npx", "node", "docker", "git", "curl", "wget", "make",
    "python3", "ffmpeg", "sqlite3", "chrony", "chronyc", "apt", "apt-get",
];

/// Cuts the `#` comment (at line start or after a space, outside quotes).
fn without_comment_sh(l: &str) -> &str {
    let b = l.as_bytes();
    let (mut sq, mut dq, mut esc) = (false, false, false);
    for (i, &c) in b.iter().enumerate() {
        if esc {
            esc = false;
        } else if c == b'\\' && !sq {
            esc = true;
        } else if sq {
            sq = c != b'\'';
        } else if c == b'\'' && !dq {
            sq = true;
        } else if c == b'"' {
            dq = !dq;
        } else if c == b'#' && !dq && (i == 0 || b[i - 1].is_ascii_whitespace()) {
            return &l[..i];
        }
    }
    l
}

/// Pieces of the line where a command starts: line start, after `|`, `&`, `;`, `(` (includes `$(`),
/// a backtick, and `$(` inside double quotes.
fn segments_sh(l: &str) -> Vec<&str> {
    let b = l.as_bytes();
    let (mut sq, mut dq, mut esc) = (false, false, false);
    let mut starts = vec![0];
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if esc {
            esc = false;
        } else if c == b'\\' && !sq {
            esc = true;
        } else if sq {
            sq = c != b'\'';
        } else if c == b'\'' && !dq {
            sq = true;
        } else if c == b'"' {
            dq = !dq;
        } else if dq {
            if c == b'$' && b.get(i + 1) == Some(&b'(') {
                starts.push(i + 2);
                i += 1;
            }
        } else if matches!(c, b'|' | b'&' | b';' | b'(' | b'`') {
            starts.push(i + 1);
        }
        i += 1;
    }
    let mut out = Vec::new();
    for (k, &start) in starts.iter().enumerate() {
        let end = starts.get(k + 1).map(|&x| x - 1).unwrap_or(l.len()).min(l.len());
        if start <= end {
            out.push(&l[start..end]);
        }
    }
    out
}

/// Known program in command position in the piece (skipping `sudo`, `exec`, control words
/// and `VAR=x` assignments).
fn command_sh(seg: &str) -> Option<String> {
    let mut skip_arg = false;
    let mut flags_sudo = false;
    for w in seg.split_whitespace() {
        if skip_arg {
            skip_arg = false;
            continue;
        }
        let w = w.trim_matches(|c| c == '"' || c == '\'');
        let attr = w.split_once('=').is_some_and(|(k, _)| !k.is_empty() && k.chars().all(ident_ch) && !k.starts_with(|c: char| c.is_ascii_digit()));
        if attr || ["if", "then", "else", "elif", "do", "while", "until", "!", "time", "exec"].contains(&w) {
            continue;
        }
        if w == "sudo" {
            flags_sudo = true;
            continue;
        }
        if flags_sudo && w.starts_with('-') {
            skip_arg = ["-u", "-g", "-h", "-p", "-C", "-D", "-R", "-T", "-U"].contains(&w);
            continue;
        }
        let p = basename(w);
        return PROGRAMS_SH.contains(&p).then(|| p.to_string());
    }
    None
}

fn ident_ch(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// Terminator of the heredoc opened on the line (`<<EOF`, `<<-'EOF'`), if any.
fn heredoc_sh(l: &str) -> Option<String> {
    let mut rest = l;
    while let Some(k) = rest.find("<<") {
        let after = &rest[k + 2..];
        if let Some(d) = after.strip_prefix('<') {
            rest = d;
            continue;
        }
        let d = after.trim_start_matches('-').trim_start();
        let d = d.trim_start_matches(|c| c == '"' || c == '\'');
        let w: String = d.chars().take_while(|&c| ident_ch(c)).collect();
        if w.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_') {
            return Some(w);
        }
        rest = after;
    }
    None
}

pub fn extract_sh(src: &str) -> Vec<Oc> {
    let mut out = Vec::new();
    let mut hd: Option<String> = None;
    for (i, l) in src.lines().enumerate() {
        if let Some(t) = &hd {
            if l.trim() == t {
                hd = None;
            }
            continue;
        }
        let l = without_comment_sh(l);
        for seg in segments_sh(l) {
            if let Some(p) = command_sh(seg) {
                out.push(Oc { raw: Raw::Program(p), line: i + 1 });
            }
        }
        hd = heredoc_sh(l);
    }
    out
}

/// The first line is an sh/bash/dash shebang (direct or through `env`).
fn shebang_sh(src: &str) -> bool {
    let Some(r) = src.lines().next().and_then(|l| l.strip_prefix("#!")) else { return false };
    let mut it = r.split_whitespace();
    let mut prog = it.next().map(basename).unwrap_or("");
    if prog == "env" {
        prog = it.find(|x| !x.starts_with('-')).unwrap_or("");
    }
    ["sh", "bash", "dash"].contains(&prog)
}

// ---------- resolution ----------

pub struct Proj {
    pub files: HashSet<String>,
    /// crate name (with `_`) -> directory of its Cargo.toml
    pub crates: HashMap<String, String>,
}

#[derive(Debug, PartialEq)]
pub enum Res {
    Dep(Vec<String>),
    Embed(Vec<String>),
    Invoke(String),
    Write,
    Route(String),
    Env(String),
    Http(String),
    Device(String),
}

fn dirname(p: &str) -> &str {
    p.rfind('/').map(|i| &p[..i]).unwrap_or("")
}

/// `dir` + normalized `rel`; None if it leaves the root.
pub fn join_path(dir: &str, rel: &str) -> Option<String> {
    let mut v: Vec<&str> = dir.split('/').filter(|s| !s.is_empty()).collect();
    for s in rel.split('/') {
        match s {
            "" | "." => {}
            ".." => {
                v.pop()?;
            }
            x => v.push(x),
        }
    }
    Some(v.join("/"))
}

/// Name of the `[package]` of a Cargo.toml.
pub fn name_of_package(toml: &str) -> Option<String> {
    let mut in_package = false;
    for l in toml.lines() {
        let l = l.trim();
        if l.starts_with('[') {
            in_package = l == "[package]";
        } else if in_package {
            if let Some((k, v)) = l.split_once('=') {
                if k.trim() == "name" {
                    return Some(v.trim().trim_matches(|c| c == '"' || c == '\'').to_string());
                }
            }
        }
    }
    None
}

impl Proj {
    pub fn new(files: &[String], reader: &dyn Fn(&str) -> Option<String>) -> Proj {
        let mut crates = HashMap::new();
        for f in files {
            if basename(f) == "Cargo.toml" {
                if let Some(n) = reader(f).as_deref().and_then(name_of_package) {
                    crates.insert(n.replace('-', "_"), dirname(f).to_string());
                }
            }
        }
        Proj { files: files.iter().cloned().collect(), crates }
    }

    pub fn names_of_crates(&self) -> Vec<String> {
        let mut v: Vec<String> = self.crates.keys().cloned().collect();
        v.sort();
        v
    }

    fn has(&self, p: &Option<String>) -> bool {
        p.as_ref().is_some_and(|p| self.files.contains(p))
    }

    fn first(&self, cands: Vec<Option<String>>) -> Option<String> {
        cands.into_iter().find(|c| self.has(c)).flatten()
    }

    fn crate_dir(&self, file: &str) -> String {
        let mut d = dirname(file).to_string();
        loop {
            if self.files.contains(&join_path(&d, "Cargo.toml").unwrap_or_default()) {
                return d;
            }
            if d.is_empty() {
                return d;
            }
            d = dirname(&d).to_string();
        }
    }

    fn mod_path(file: &str, cdir: &str) -> Vec<String> {
        let rel = if cdir.is_empty() { file } else { file.strip_prefix(cdir).map(|r| r.trim_start_matches('/')).unwrap_or(file) };
        let Some(r) = rel.strip_prefix("src/").and_then(|r| r.strip_suffix(".rs")) else { return vec![] };
        let mut v: Vec<String> = r.split('/').map(String::from).collect();
        if v.last().map(String::as_str) == Some("mod") {
            v.pop();
        } else if v.len() == 1 && (v[0] == "lib" || v[0] == "main") {
            v.clear();
        }
        v
    }

    /// Longest prefix of `segs` that is a module of the crate (`src/a/b.rs`, `src/a/b/mod.rs`, `src/a.rs`...).
    fn find_module(&self, cdir: &str, segs: &[String]) -> Option<String> {
        for k in (1..=segs.len()).rev() {
            let base = format!("src/{}", segs[..k].join("/"));
            let c = vec![join_path(cdir, &format!("{base}.rs")), join_path(cdir, &format!("{base}/mod.rs"))];
            if let Some(f) = self.first(c) {
                return Some(f);
            }
        }
        None
    }

    fn rust(&self, file: &str, raw: &Raw) -> Vec<String> {
        let cdir = self.crate_dir(file);
        match raw {
            Raw::UseRust(p) => {
                let segs: Vec<String> = p.split("::").map(String::from).collect();
                match segs[0].as_str() {
                    "crate" => self.find_module(&cdir, &segs[1..]).into_iter().collect(),
                    "super" => {
                        let n = segs.iter().take_while(|s| *s == "super").count();
                        let base = Self::mod_path(file, &cdir);
                        if n > base.len() {
                            return vec![];
                        }
                        let mut full: Vec<String> = base[..base.len() - n].to_vec();
                        full.extend(segs[n..].iter().cloned());
                        self.find_module(&cdir, &full).into_iter().collect()
                    }
                    name => {
                        let Some(d) = self.crates.get(name) else { return vec![] };
                        self.first(vec![join_path(d, "src/lib.rs")]).into_iter().collect()
                    }
                }
            }
            _ => vec![],
        }
    }

    fn ts(&self, file: &str, spec: &str) -> Vec<String> {
        let Some(base) = join_path(dirname(file), spec) else { return vec![] };
        let mut c = vec![Some(base.clone())];
        for e in [".ts", ".js", ".svelte", ".mjs"] {
            c.push(Some(format!("{base}{e}")));
        }
        c.push(Some(format!("{base}/index.ts")));
        c.push(Some(format!("{base}/index.js")));
        if let Some(s) = base.strip_suffix(".js") {
            c.push(Some(format!("{s}.ts")));
        }
        self.first(c).into_iter().collect()
    }

    fn py_prefix(&self, bases: &[String], segs: &[String], full: bool) -> Option<String> {
        let min = if full { segs.len() } else { 1 };
        for k in (min..=segs.len()).rev() {
            for b in bases {
                let p = segs[..k].join("/");
                let c = vec![join_path(b, &format!("{p}.py")), join_path(b, &format!("{p}/__init__.py"))];
                if let Some(f) = self.first(c) {
                    return Some(f);
                }
            }
        }
        None
    }

    fn py(&self, file: &str, module: &str, names: &[String]) -> Vec<String> {
        let points = module.chars().take_while(|&c| c == '.').count();
        let rest: Vec<String> = module[points..].split('.').filter(|s| !s.is_empty()).map(String::from).collect();
        let bases: Vec<String> = if points > 0 {
            let mut d = dirname(file).to_string();
            for _ in 1..points {
                d = dirname(&d).to_string();
            }
            vec![d]
        } else {
            let mut v = vec![dirname(file).to_string()];
            if !v.contains(&String::new()) {
                v.push(String::new());
            }
            v
        };
        let mut out = Vec::new();
        for n in names {
            let mut s = rest.clone();
            s.push(n.clone());
            if let Some(f) = self.py_prefix(&bases, &s, true) {
                if !out.contains(&f) {
                    out.push(f);
                }
            }
        }
        if out.is_empty() && !rest.is_empty() {
            out.extend(self.py_prefix(&bases, &rest, false));
        }
        out
    }

    pub fn resolve(&self, file: &str, raw: &Raw) -> Option<Res> {
        Some(match raw {
            Raw::UseRust(_) => Res::Dep(self.rust(file, raw)),
            Raw::Import(s) => Res::Dep(self.ts(file, s)),
            Raw::PyMod(m, n) => Res::Dep(self.py(file, m, n)),
            Raw::Program(p) => Res::Invoke(p.clone()),
            Raw::IncludeStr(p) => {
                Res::Embed(join_path(dirname(file), p).filter(|f| self.files.contains(f)).into_iter().collect())
            }
            Raw::Dir(p) => {
                let Some(pre) = join_path(&self.crate_dir(file), p) else { return Some(Res::Embed(vec![])) };
                let mut v: Vec<String> = self
                    .files
                    .iter()
                    .filter(|f| **f == pre || pre.is_empty() || f.starts_with(&format!("{pre}/")))
                    .cloned()
                    .collect();
                v.sort();
                Res::Embed(v)
            }
            Raw::Route(r) => Res::Route(r.clone()),
            Raw::Write => Res::Write,
            Raw::Env(e) => Res::Env(e.clone()),
            Raw::Fetch(p) => Res::Http(p.clone()),
            Raw::Device(p) => Res::Device(p.clone()),
        })
    }
}

// ---------- routes ----------

fn param(seg: &str) -> bool {
    seg.starts_with('{') || seg.starts_with(':') || seg.starts_with('*')
}

/// Number of segments of the route if it matches the path (equal, parameter, or route that prefixes the path).
pub fn match_route(route: &str, path: &str) -> Option<usize> {
    let r: Vec<&str> = route.split('/').filter(|s| !s.is_empty()).collect();
    let mut p: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    if path.ends_with('/') && !p.is_empty() {
        p.push("\0"); // template cut at `${`: a variable segment follows
    }
    if r.len() > p.len() || (r.is_empty() && !p.is_empty()) {
        return None;
    }
    r.iter().zip(&p).all(|(a, b)| a == b || param(a)).then_some(r.len())
}

// ---------- project facts ----------

#[derive(Debug, Clone)]
pub struct Fact {
    pub kind: Kind,
    pub file: String,
    pub line: usize,
    pub from: Option<usize>,
    pub to: Option<usize>,
    /// Path (http, route), program (invoke) or variable (env); empty for the others.
    pub target: String,
}

pub fn extract_file(file: &str, src: &str, crates: &[String]) -> Vec<Oc> {
    let base = basename(file);
    let ext = if base.contains('.') { base.rsplit('.').next().unwrap_or("") } else { "" };
    match ext {
        "sh" => extract_sh(src),
        "" if shebang_sh(src) => extract_sh(src),
        "rs" => extract_rust(src, crates),
        "ts" | "tsx" | "js" | "mjs" | "svelte" => extract_ts(src),
        "py" => extract_py(src),
        _ => vec![],
    }
}

pub fn collect_facts(m: &Project, files: &[String], reader: &dyn Fn(&str) -> Option<String>) -> Vec<Fact> {
    let proj = Proj::new(files, reader);
    let names = proj.names_of_crates();
    let mut out = Vec::new();
    for f in files {
        let Some(src) = reader(f) else { continue };
        let from = m.owner(f);
        for oc in extract_file(f, &src, &names) {
            let Some(res) = proj.resolve(f, &oc.raw) else { continue };
            let mut new = |kind, to: Option<usize>, target: &str| {
                out.push(Fact { kind, file: f.clone(), line: oc.line, from, to, target: target.to_string() })
            };
            match res {
                Res::Dep(targets) => {
                    let owners: BTreeSet<Option<usize>> = targets.iter().map(|a| m.owner(a)).collect();
                    for d in owners {
                        new(Kind::Dep, d, "");
                    }
                }
                Res::Embed(targets) => {
                    let owners: BTreeSet<Option<usize>> = targets.iter().map(|a| m.owner(a)).collect();
                    if owners.is_empty() {
                        new(Kind::Embed, None, "");
                    }
                    for d in owners {
                        new(Kind::Embed, d, "");
                    }
                }
                Res::Invoke(p) => new(Kind::Invoke, None, &p),
                Res::Write => new(Kind::Write, None, ""),
                Res::Route(r) => new(Kind::Route, None, &r),
                Res::Env(e) => new(Kind::Env, None, &e),
                Res::Http(h) => new(Kind::Http, None, &h),
                Res::Device(d) => new(Kind::Device, None, &d),
            }
        }
    }
    let routes: Vec<(Option<usize>, String)> = out.iter().filter(|f| f.kind == Kind::Route).map(|f| (f.from, f.target.clone())).collect();
    for f in out.iter_mut().filter(|f| f.kind == Kind::Http) {
        let mut best: Option<(usize, Option<usize>)> = None;
        for (c, r) in &routes {
            if let Some(n) = match_route(r, &f.target) {
                if best.is_none_or(|(k, _)| n > k) {
                    best = Some((n, *c));
                }
            }
        }
        f.to = best.and_then(|(_, c)| c);
    }
    out
}

/// Reads the project from disk.
pub fn collect(m: &Project, files: &[String], fs: &dyn Fs, root: &Path) -> Vec<Fact> {
    collect_facts(m, files, &|f| fs.read_to_string(&root.join(f)).ok())
}

// ---------- aggregation and output ----------

#[derive(Debug, Clone)]
pub struct Agg {
    pub kind: Kind,
    pub from: Option<usize>,
    pub to: Option<usize>,
    pub target: String,
    pub n: usize,
    pub ex: Vec<String>,
}

fn name(m: &Project, c: Option<usize>) -> String {
    c.map(|c| m.id(c).to_string()).unwrap_or_else(|| "?".into())
}

pub fn aggregate(m: &Project, facts: &[Fact]) -> Vec<Agg> {
    let mut map: BTreeMap<(Kind, String, String, String), Agg> = BTreeMap::new();
    for f in facts {
        if f.kind == Kind::Dep && f.from == f.to {
            continue;
        }
        let target = if matches!(f.kind, Kind::Dep | Kind::Embed | Kind::Write) { String::new() } else { f.target.clone() };
        let to = if matches!(f.kind, Kind::Dep | Kind::Embed | Kind::Http) { f.to } else { None };
        let key = (f.kind, name(m, f.from), name(m, to), target.clone());
        let a = map.entry(key).or_insert_with(|| Agg { kind: f.kind, from: f.from, to, target, n: 0, ex: vec![] });
        a.n += 1;
        let ex = format!("{}:{}", f.file, f.line);
        if a.ex.len() < 3 && !a.ex.contains(&ex) {
            a.ex.push(ex);
        }
    }
    map.into_values().collect()
}

pub fn text(m: &Project, aggs: &[Agg]) -> String {
    let mut o = String::new();
    for a in aggs {
        let (from, to) = (name(m, a.from), name(m, a.to));
        let ex = a.ex.first().cloned().unwrap_or_default();
        let _ = match a.kind {
            Kind::Dep => writeln!(o, "dep {from} -> {to} ({}) e.g. {ex}", a.n),
            Kind::Embed => writeln!(o, "embed {from} -> {to} ({}) e.g. {ex}", a.n),
            Kind::Http => writeln!(o, "http {from} -> {to} {} e.g. {ex}", a.target),
            Kind::Invoke => writeln!(o, "invoke {from} {} ({}) e.g. {ex}", a.target, a.n),
            Kind::Write => writeln!(o, "write {from} ({}) e.g. {ex}", a.n),
            Kind::Route => writeln!(o, "route {from} {} e.g. {ex}", a.target),
            Kind::Env => writeln!(o, "env {from} {} e.g. {ex}", a.target),
            Kind::Device => writeln!(o, "device {from} {} e.g. {ex}", a.target),
        };
    }
    o
}

pub fn json(m: &Project, aggs: &[Agg]) -> String {
    let mut o = String::from("[");
    for (i, a) in aggs.iter().enumerate() {
        if i > 0 {
            o.push(',');
        }
        let opt = |c: Option<usize>, has: bool| if has { jstr(&name(m, c)) } else { "null".to_string() };
        let has_for = matches!(a.kind, Kind::Dep | Kind::Embed | Kind::Http);
        let target = if a.target.is_empty() { "null".to_string() } else { jstr(&a.target) };
        let ex: Vec<String> = a.ex.iter().map(|e| jstr(e)).collect();
        let _ = write!(
            o,
            "{{\"kind\":{},\"from\":{},\"to\":{},\"target\":{},\"n\":{},\"examples\":[{}]}}",
            jstr(a.kind.name()),
            jstr(&name(m, a.from)),
            opt(a.to, has_for),
            target,
            a.n,
            ex.join(",")
        );
    }
    o.push_str("]\n");
    o
}

// ---------- check warnings ----------

fn word_set(s: &str) -> Vec<String> {
    s.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).map(|w| w.to_lowercase()).collect()
}

/// There is a relation between an ancestor-or-descendant of `x` and one of `y` (in either direction).
pub fn pair_covered(m: &Project, x: usize, y: usize) -> bool {
    let covers = |target: usize, c: usize| m.inside(c, target) || m.inside(target, c);
    m.edges().iter().any(|&(from, to)| (covers(x, from) && covers(y, to)) || (covers(y, from) && covers(x, to)))
}

/// Programs invoked per component of the map: name -> (components, first example, last component).
pub fn programs<'a>(m: &Project, aggs: &'a [Agg]) -> BTreeMap<&'a str, (BTreeSet<String>, String, usize)> {
    let mut progs: BTreeMap<&str, (BTreeSet<String>, String, usize)> = BTreeMap::new();
    for a in aggs.iter().filter(|a| a.kind == Kind::Invoke && a.from.is_some()) {
        let e = progs.entry(a.target.as_str()).or_insert_with(|| (BTreeSet::new(), a.ex.first().cloned().unwrap_or_default(), 0));
        e.0.insert(name(m, a.from));
        e.2 = a.from.unwrap_or(0);
    }
    progs
}

/// Some external component matches the program (id, or a word of the id or of the text).
pub fn program_covered(m: &Project, prog: &str) -> bool {
    let p = prog.to_lowercase();
    (0..m.n()).filter(|&c| m.external(c)).any(|c| m.id(c) == prog || word_set(m.id(c)).contains(&p))
}

/// Warnings of a fact without a relation, with the component each one refers to (so the caller
/// can find the line or the label).
pub fn warnings_c(m: &Project, aggs: &[Agg]) -> Vec<(usize, String)> {
    let mut d = Vec::new();
    // dep, embed, http without a relation between the ancestor chains
    let mut pairs: BTreeMap<(usize, usize), (BTreeSet<&'static str>, usize, String)> = BTreeMap::new();
    for a in aggs.iter().filter(|a| matches!(a.kind, Kind::Dep | Kind::Embed | Kind::Http)) {
        let (Some(x), Some(y)) = (a.from, a.to) else { continue };
        if x == y || m.inside(x, y) || m.inside(y, x) {
            continue;
        }
        let e = pairs.entry((x, y)).or_insert_with(|| (BTreeSet::new(), 0, a.ex.first().cloned().unwrap_or_default()));
        e.0.insert(a.kind.name());
        e.1 += a.n;
    }
    for ((x, y), (kinds, n, ex)) in pairs {
        if !pair_covered(m, x, y) {
            let kinds: Vec<&str> = kinds.into_iter().collect();
            d.push((x, format!("fact without relation: {} -> {} ({}, {n} occurrences, e.g. {ex})", m.id(x), m.id(y), kinds.join("+"))));
        }
    }
    // program invoked without an external
    for (prog, (comps, ex, c)) in programs(m, aggs) {
        if !program_covered(m, prog) {
            let cs: Vec<String> = comps.into_iter().collect();
            d.push((c, format!("program invoked without an external component: {prog} ({}, e.g. {ex})", cs.join(","))));
        }
    }
    // disk write without a relation to an external
    let mut writes: BTreeMap<usize, (usize, String)> = BTreeMap::new();
    for a in aggs.iter().filter(|a| a.kind == Kind::Write) {
        let Some(c) = a.from else { continue };
        let e = writes.entry(c).or_insert_with(|| (0, a.ex.first().cloned().unwrap_or_default()));
        e.0 += a.n;
    }
    let edges = m.edges();
    for (c, (n, ex)) in writes {
        let is_linked = |x: usize| m.inside(x, c) || m.inside(c, x);
        let rel = edges.iter().any(|&(from, to)| (is_linked(from) && m.datum(to)) || (is_linked(to) && m.datum(from)));
        if !rel {
            d.push((c, format!("writes to disk without a relation to an external: {} ({n} sites, e.g. {ex})", m.id(c))));
        }
    }
    d
}

#[cfg(test)]
mod tests {
    #[test]
    fn mask_knows_string_literals() {
        use super::{mask, scan, Lang};
        // "/**" in a string does not open a block; the real comment is blanked
        let rs = "let a = \"/**\"; // note\nlet b = 1; /* ZZZ */ let c = 2;\n";
        let m = mask(rs, Lang::Rs);
        assert!(m.contains("let a = \"/**\";") && m.contains("let b = 1;") && m.contains("let c = 2;"), "{m}");
        assert!(!m.contains("note") && !m.contains("ZZZ"), "{m}");
        // raw strings, with and without #, and a char literal with quotes
        let rs = "let a = r\"/*\"; let b = r#\"\"/*\"#; let q = '\"'; let l: &'a str = \"//x\";\nlet end = 1;\n";
        let m = mask(rs, Lang::Rs);
        assert_eq!(m, rs);
        let (_, in_str) = scan(rs, Lang::Rs);
        assert!(in_str[rs.find("/*").unwrap()] && in_str[rs.find("//x").unwrap()]);
        assert!(!in_str[rs.find("end").unwrap()]);
        // TS/JS: single quotes, double quotes and backticks
        let ts = "const a = '/*'; const b = \"//\"; const c = `/**\n`; // x\nend();\n";
        let m = mask(ts, Lang::Ts);
        assert!(m.contains("'/*'") && m.contains("\"//\"") && m.contains("/**\n") && m.contains("end();") && !m.contains("// x"), "{m}");
        // Python: triple quotes; # inside a string is not a comment, nor at the start of a docstring line
        let py = "\"\"\"doc\n# not a comment\nimport os\n\"\"\"\nx = '#'\n# comment\n";
        let m = mask(py, Lang::Py);
        assert!(m.contains("# not a comment") && m.contains("x = '#'") && !m.contains("# comment\n"), "{m}");
    }

    #[test]
    fn pattern_inside_string_is_not_a_fact() {
        let rs = "fn f() {\n    let p = \"fs::write(\";\n    let q = \"/dev/\";\n    let r = r#\"env::var(\"X\")\"#;\n    let s = \"use crate::a::b;\";\n}\n";
        assert!(extract_rust(rs, &[]).is_empty(), "{:?}", extract_rust(rs, &[]).iter().map(|o| o.line).collect::<Vec<_>>());
        let py = "\"\"\"\nimport os\nfrom a import b\n\"\"\"\nx = \"os.getenv('A')\"\n";
        assert!(extract_py(py).is_empty());
        let ts = "const a = \"fetch('/x')\"; const b = 'process.env.A';\n";
        assert!(extract_ts(ts).is_empty());
        // real code next to it still counts
        let rs = "fn f() { let p = \"fs::write(\"; std::fs::write(\"a\", \"b\"); }\n";
        assert_eq!(extract_rust(rs, &[]).len(), 1);
    }

    #[test]
    fn ignore_test_block() {
        let src = "use crate::a;\nfn f() {}\n#[cfg(test)]\nmod t {\n    use crate::b;\n    fn g() { crate::c::x(); }\n}\nuse crate::d;\n";
        let clean = super::without_tests(src);
        assert!(clean.contains("crate::a") && clean.contains("crate::d"), "{clean}");
        assert!(!clean.contains("crate::b") && !clean.contains("crate::c"), "{clean}");
        assert_eq!(clean.lines().count(), src.lines().count());
    }

    use super::*;
    use crate::intent::analysis::Analysis;
    use crate::intent::lexicon::Lexicon;

    /// Project from an `.intent` text (without errors) handed to the test.
    fn with_project<R>(text: &str, f: impl FnOnce(&Project) -> R) -> R {
        let lex = Lexicon::embedded().unwrap();
        let a = Analysis::text(text, &lex);
        assert!(a.errors.is_empty(), "{:?}", a.lines_of_error());
        f(&Project::new(&a))
    }

    fn raws(v: &[Oc]) -> Vec<(usize, Raw)> {
        v.iter().map(|o| (o.line, o.raw.clone())).collect()
    }

    fn proj(files: &[&str], tomls: &[(&str, &str)]) -> Proj {
        let v: Vec<String> = files.iter().map(|s| s.to_string()).collect();
        let t: HashMap<String, String> = tomls.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect();
        Proj::new(&v, &|f| t.get(f).cloned())
    }

    #[test]
    fn rust_extract_all() {
        let src = r#"
use crate::a::b;
use crate::{c, d::e as f};
pub use super::g::*;
use other_crate::x;
use std::fs;
mod h;
// use crate::commented;
fn f() {
    let p = crate::z::run();
    Command::new("/usr/bin/git").arg("x");
    let s = include_str!("../web/index.html");
    let _ = include_bytes!( "a.bin" );
    let r = Router::new().route("/api/x/{id}", get(h)).route(
        "/y", post(h));
    fs::write("a", "b").unwrap();
    let _ = std::fs::File::create("c");
    let _ = OpenOptions::new();
    fs::rename(a, b);
    std::fs::create_dir_all("d");
    let v = std::env::var("MY_VAR");
    let w = env::var_os("OTHER");
}
#[derive(RustEmbed)]
#[folder = "web/dist"]
struct W;
"#;
        let v = raws(&extract_rust(src, &["other_crate".to_string()]));
        let has = |l: usize, c: Raw| assert!(v.contains(&(l, c.clone())), "missing {l} {c:?} in {v:?}");
        has(2, Raw::UseRust("crate::a::b".into()));
        has(3, Raw::UseRust("crate::c".into()));
        has(3, Raw::UseRust("crate::d::e".into()));
        has(4, Raw::UseRust("super::g".into()));
        has(5, Raw::UseRust("other_crate::x".into()));
        assert!(!v.iter().any(|(l, _)| *l == 7), "mod h; is not a use: {v:?}");
        has(10, Raw::UseRust("crate::z::run".into()));
        has(11, Raw::Program("git".into()));
        has(12, Raw::IncludeStr("../web/index.html".into()));
        has(13, Raw::IncludeStr("a.bin".into()));
        has(14, Raw::Route("/api/x/{id}".into()));
        has(14, Raw::Route("/y".into()));
        has(16, Raw::Write);
        has(17, Raw::Write);
        has(18, Raw::Write);
        has(19, Raw::Write);
        has(20, Raw::Write);
        has(21, Raw::Env("MY_VAR".into()));
        has(22, Raw::Env("OTHER".into()));
        has(25, Raw::Dir("web/dist".into()));
        assert!(!v.iter().any(|(_, c)| matches!(c, Raw::UseRust(p) if p.contains("comment") || p.starts_with("std"))), "{v:?}");
        assert_eq!(v.iter().filter(|(_, c)| matches!(c, Raw::Write)).count(), 5);
    }

    #[test]
    fn rust_use_multiline_and_crate_in_expression() {
        let v = raws(&extract_rust("use crate::{\n    a::x,\n    b,\n};\nfn f() { other::g(); crate::m::h(); x::crate_y(); }\n", &["other".into()]));
        assert!(v.contains(&(1, Raw::UseRust("crate::a::x".into()))) && v.contains(&(1, Raw::UseRust("crate::b".into()))), "{v:?}");
        assert!(v.contains(&(5, Raw::UseRust("other::g".into()))) && v.contains(&(5, Raw::UseRust("crate::m::h".into()))), "{v:?}");
        assert_eq!(v.len(), 4, "{v:?}");
    }

    #[test]
    fn ts_extract_all() {
        let src = "import a from './a';\nimport { b } from \"../b\";\nimport pkg from 'package';\nimport './side';\n\
const c = await import('./c');\nconst d = require('./d');\nconst u = new URL('./w.js', import.meta.url);\n\
fetch('/api/x?y=1');\nfetch(`/api/items/${id}/log`, {});\nfetch(\"https://outside/x\");\nconst e = process.env.TOKEN + import.meta.env.VITE_URL;\n// import z from './commented'\nexport * from './re';\n";
        let v = raws(&extract_ts(src));
        for (l, c) in [
            (1, Raw::Import("./a".into())),
            (2, Raw::Import("../b".into())),
            (4, Raw::Import("./side".into())),
            (5, Raw::Import("./c".into())),
            (6, Raw::Import("./d".into())),
            (7, Raw::Import("./w.js".into())),
            (8, Raw::Fetch("/api/x".into())),
            (9, Raw::Fetch("/api/items/".into())),
            (11, Raw::Env("TOKEN".into())),
            (11, Raw::Env("VITE_URL".into())),
            (13, Raw::Import("./re".into())),
        ] {
            assert!(v.contains(&(l, c.clone())), "missing {l} {c:?} in {v:?}");
        }
        assert!(!v.iter().any(|(_, c)| matches!(c, Raw::Import(p) if p == "package" || p.contains("comment"))), "{v:?}");
        assert!(!v.iter().any(|(_, c)| matches!(c, Raw::Fetch(p) if p.contains("fora"))), "{v:?}");
    }

    #[test]
    fn py_extract_all() {
        let src = "import os, json\nfrom .x import a, b as c\nfrom pkg.y import z\nimport pkg.w\n  # import commented\n\
def f():\n    import internal\n    subprocess.run([\"/bin/ffmpeg\", \"-i\"])\n    subprocess.Popen('git status', shell=True)\n    subprocess.check_output(cmd)\n\
    k = os.environ[\"A\"] + os.environ.get('B') + os.getenv(\"C\", \"d\")\nfrom __future__ import annotations\n";
        let v = raws(&extract_py(src));
        for (l, c) in [
            (2, Raw::PyMod(".x".into(), vec!["a".into(), "b".into()])),
            (3, Raw::PyMod("pkg.y".into(), vec!["z".into()])),
            (4, Raw::PyMod("pkg.w".into(), vec![])),
            (7, Raw::PyMod("internal".into(), vec![])),
            (8, Raw::Program("ffmpeg".into())),
            (9, Raw::Program("git status".into())),
            (11, Raw::Env("A".into())),
            (11, Raw::Env("B".into())),
            (11, Raw::Env("C".into())),
        ] {
            assert!(v.contains(&(l, c.clone())), "missing {l} {c:?} in {v:?}");
        }
        assert!(!v.iter().any(|(_, c)| matches!(c, Raw::PyMod(m, _) if m.contains("comment") || m == "__future__")), "{v:?}");
        assert_eq!(v.iter().filter(|(_, c)| matches!(c, Raw::Program(_))).count(), 2, "{v:?}");
    }

    #[test]
    fn resolve_modules_rust() {
        let p = proj(
            &["Cargo.toml", "src/lib.rs", "src/a.rs", "src/a/b.rs", "src/c/mod.rs", "src/c/d.rs", "x/Cargo.toml", "x/src/lib.rs", "x/src/main.rs", "x/src/q.rs", "tests/t.rs"],
            &[("Cargo.toml", "[package]\nname = \"my-crate\"\n"), ("x/Cargo.toml", "[package]\nname = \"other\"\n[dependencies]\nname = \"no\"\n")],
        );
        assert_eq!(p.crates["my_crate"], "");
        assert_eq!(p.crates["other"], "x");
        let r = |file: &str, c: Raw| p.resolve(file, &c).unwrap();
        let u = |s: &str| Raw::UseRust(s.into());
        assert_eq!(r("src/lib.rs", u("crate::a::b::Item")), Res::Dep(vec!["src/a/b.rs".into()]));
        assert_eq!(r("src/lib.rs", u("crate::a::Item")), Res::Dep(vec!["src/a.rs".into()]));
        assert_eq!(r("src/a.rs", u("crate::c::d")), Res::Dep(vec!["src/c/d.rs".into()]));
        assert_eq!(r("src/lib.rs", u("crate::c::Kind")), Res::Dep(vec!["src/c/mod.rs".into()]));
        assert_eq!(r("src/a/b.rs", u("super::b")), Res::Dep(vec!["src/a/b.rs".into()]));
        assert_eq!(r("src/c/d.rs", u("super::Kind")), Res::Dep(vec!["src/c/mod.rs".into()]));
        assert_eq!(r("src/a/b.rs", u("super::super::c::d")), Res::Dep(vec!["src/c/d.rs".into()]));
        assert_eq!(r("src/lib.rs", u("crate::nothing::x")), Res::Dep(vec![]));
        assert_eq!(r("src/lib.rs", u("other::f")), Res::Dep(vec!["x/src/lib.rs".into()]));
        assert_eq!(r("tests/t.rs", u("my_crate::f")), Res::Dep(vec!["src/lib.rs".into()]));
        assert_eq!(r("x/src/main.rs", u("crate::q::f")), Res::Dep(vec!["x/src/q.rs".into()]));
    }

    #[test]
    fn resolve_embed_ts_and_python() {
        let p = proj(
            &["Cargo.toml", "src/m.rs", "web/index.html", "web/dist/a.js", "web/dist/b/c.css", "ui/a.ts", "ui/b.svelte", "ui/lib/index.ts", "ui/d.js", "py/app.py", "py/pkg/__init__.py", "py/pkg/x.py", "py/y.py"],
            &[],
        );
        let r = |file: &str, c: Raw| p.resolve(file, &c).unwrap();
        assert_eq!(r("src/m.rs", Raw::IncludeStr("../web/index.html".into())), Res::Embed(vec!["web/index.html".into()]));
        assert_eq!(r("src/m.rs", Raw::Dir("web/dist/".into())), Res::Embed(vec!["web/dist/a.js".into(), "web/dist/b/c.css".into()]));
        assert_eq!(r("src/m.rs", Raw::IncludeStr("no.txt".into())), Res::Embed(vec![]));
        let i = |s: &str| Raw::Import(s.into());
        assert_eq!(r("ui/a.ts", i("./b")), Res::Dep(vec!["ui/b.svelte".into()]));
        assert_eq!(r("ui/a.ts", i("./lib")), Res::Dep(vec!["ui/lib/index.ts".into()]));
        assert_eq!(r("ui/lib/index.ts", i("../d")), Res::Dep(vec!["ui/d.js".into()]));
        assert_eq!(r("ui/a.ts", i("./d.js")), Res::Dep(vec!["ui/d.js".into()]));
        assert_eq!(r("ui/a.ts", i("./b.js")), Res::Dep(vec![]));
        assert_eq!(r("ui/a.ts", i("./lib/index.js")), Res::Dep(vec!["ui/lib/index.ts".into()]));
        let y = |m: &str, n: &[&str]| Raw::PyMod(m.into(), n.iter().map(|s| s.to_string()).collect());
        assert_eq!(r("py/app.py", y(".y", &["f"])), Res::Dep(vec!["py/y.py".into()]));
        assert_eq!(r("py/app.py", y(".pkg", &["x"])), Res::Dep(vec!["py/pkg/x.py".into()]));
        assert_eq!(r("py/app.py", y(".pkg", &["func"])), Res::Dep(vec!["py/pkg/__init__.py".into()]));
        assert_eq!(r("py/app.py", y("pkg.x", &["f"])), Res::Dep(vec!["py/pkg/x.py".into()]));
        assert_eq!(r("py/app.py", y("pkg.x", &[])), Res::Dep(vec!["py/pkg/x.py".into()]));
        assert_eq!(r("py/pkg/x.py", y("..y", &[])), Res::Dep(vec!["py/y.py".into()]));
        assert_eq!(r("py/pkg/x.py", y("os", &[])), Res::Dep(vec![]));
        assert_eq!(r("py/pkg/x.py", y(".", &["y"])), Res::Dep(vec![]));
    }

    #[test]
    fn matching_of_routes() {
        assert_eq!(match_route("/api/x", "/api/x"), Some(2));
        assert_eq!(match_route("/api/x/{id}", "/api/x/${n}"), Some(3));
        assert_eq!(match_route("/api/:id/log", "/api/7/log"), Some(3));
        assert_eq!(match_route("/api", "/api/items/"), Some(1)); // prefix
        assert_eq!(match_route("/api/*rest", "/api/a/b"), Some(2));
        assert_eq!(match_route("/api/items/{id}", "/api/items/"), Some(3)); // fetch(`/api/items/${id}`)
        assert_eq!(match_route("/api/items/new", "/api/items/"), None);
        assert_eq!(match_route("/api/x", "/api"), None);
        assert_eq!(match_route("/api/x", "/api/y"), None);
        assert_eq!(match_route("/", "/api"), None);
        assert_eq!(match_route("/", "/"), Some(0));
    }

    const MAP: &str = "The module \"web\" is implemented in \"ui/**\".\n\
The module \"api\" is implemented in \"src/**\".\n\
The module \"api\" contains the module \"api.routes\" and the module \"api.log\".\n\
The module \"api.routes\" is implemented in \"src/routes.rs\".\n\
The module \"api.log\" is implemented in \"src/log.rs\".\n\
The module \"web\" uses the module \"api.routes\".\n";

    #[test]
    fn aggregated_facts_http_and_warnings() {
        let file_pairs: Vec<(&str, &str)> = vec![
            ("Cargo.toml", "[package]\nname = \"x\"\n"),
            ("ui/a.ts", "fetch('/api/log/${id}');\nfetch('/api/data');\nfetch('/outside');\n"),
            ("src/routes.rs", "use crate::log::x;\nfn r(){ .route(\"/api/data\", g) .route(\"/api/log/{id}\", g); }\n"),
            ("src/log.rs", "fn f(){ std::fs::write(\"a\",\"b\"); Command::new(\"git\"); env::var(\"V\"); }\n"),
        ];
        let files: Vec<String> = file_pairs.iter().map(|a| a.0.to_string()).collect();
        let l = |f: &str| file_pairs.iter().find(|a| a.0 == f).map(|a| a.1.to_string());
        with_project(MAP, |m| {
            let facts = collect_facts(m, &files, &l);
            let aggs = aggregate(m, &facts);
            let t = text(m, &aggs);
            assert!(t.contains("http web -> api.routes /api/data e.g. ui/a.ts:2\n"), "{t}");
            assert!(t.contains("http web -> api.routes /api/log/ e.g. ui/a.ts:1\n"), "{t}"); // longest route matches among routes
            assert!(t.contains("http web -> ? /outside e.g. ui/a.ts:3\n"), "{t}");
            assert!(t.contains("route api.routes /api/data e.g. src/routes.rs:2\n"), "{t}");
            assert!(t.contains("dep api.routes -> api.log (1) e.g. src/routes.rs:1\n"), "{t}");
            assert!(t.contains("write api.log (1) e.g. src/log.rs:1\n"), "{t}");
            assert!(t.contains("invoke api.log git (1) e.g. src/log.rs:1\n"), "{t}");
            assert!(t.contains("env api.log V e.g. src/log.rs:1\n"), "{t}");
            let j = json(m, &aggs);
            assert!(j.starts_with("[{\"kind\":") && j.contains("\"kind\":\"dep\",\"from\":\"api.routes\",\"to\":\"api.log\",\"target\":null,\"n\":1,\"examples\":[\"src/routes.rs:1\"]"), "{j}");
            let warns: Vec<String> = warnings_c(m, &aggs).into_iter().map(|d| d.1).collect();
            let a = warns.join("\n");
            assert!(a.contains("fact without relation: api.routes -> api.log (dep, 1 occurrences, e.g. src/routes.rs:1)"), "{a}");
            assert!(!a.contains("web -> api.routes"), "{a}");
            assert!(a.contains("program invoked without an external component: git (api.log, e.g. src/log.rs:1)"), "{a}");
            assert!(a.contains("writes to disk without a relation to an external: api.log (1 sites, e.g. src/log.rs:1)"), "{a}");
        });
    }

    #[test]
    fn warnings_vanish_with_external_and_ancestor_relations() {
        let map = format!(
            "{MAP}The module \"api\" launches the program \"git\".\n\
The module \"api.log\" writes the file \"disk\".\n\
The module \"api\" uses the module \"api.log\".\n"
        );
        let file_pairs = [
            ("Cargo.toml", "[package]\nname = \"x\"\n"),
            ("src/routes.rs", "use crate::log::x;\n"),
            ("src/log.rs", "fn f(){ std::fs::write(\"a\",\"b\"); Command::new(\"git\"); }\n"),
        ];
        let files: Vec<String> = file_pairs.iter().map(|a| a.0.to_string()).collect();
        let l = |f: &str| file_pairs.iter().find(|a| a.0 == f).map(|a| a.1.to_string());
        with_project(&map, |m| {
            let aggs = aggregate(m, &collect_facts(m, &files, &l));
            assert!(warnings_c(m, &aggs).is_empty(), "{:?}", warnings_c(m, &aggs));
        });
    }

    #[test]
    fn declared_mod_is_not_a_dependency() {
        assert!(extract_rust("mod x;\npub mod y;\npub(crate) mod z;\n", &[]).is_empty());
        with_project("The module \"a\" is implemented in \"src/a.rs\".\nThe module \"b\" is implemented in \"src/b.rs\".\n", |m| {
            let file_pairs = [("Cargo.toml", "[package]\nname = \"x\"\n"), ("src/a.rs", "mod b;\n"), ("src/b.rs", "fn f() {}\n")];
            let files: Vec<String> = file_pairs.iter().map(|a| a.0.to_string()).collect();
            let l = |f: &str| file_pairs.iter().find(|a| a.0 == f).map(|a| a.1.to_string());
            assert!(collect_facts(m, &files, &l).is_empty());
            // the real use still counts
            let file_pairs = [("Cargo.toml", "[package]\nname = \"x\"\n"), ("src/a.rs", "mod b;\nuse crate::b::f;\n"), ("src/b.rs", "fn f() {}\n")];
            let l = |f: &str| file_pairs.iter().find(|a| a.0 == f).map(|a| a.1.to_string());
            assert_eq!(collect_facts(m, &files, &l).len(), 1);
        });
    }

    #[test]
    fn devices_in_literals() {
        let rs = "fn f() {\n    let a = \"/dev/dri/renderD128\";\n    // \"/dev/commented\"\n    let b = std::fs::read_to_string(\"/sys/class/thermal/thermal_zone0/temp\");\n    let c = \"/home/x\";\n    let d = \"/proc/{}/stat\";\n}\n";
        let targets = |v: Vec<Oc>| -> Vec<(usize, String)> {
            v.into_iter().filter_map(|o| if let Raw::Device(p) = o.raw { Some((o.line, p)) } else { None }).collect()
        };
        assert_eq!(
            targets(extract_rust(rs, &[])),
            vec![(2, "/dev/dri/renderD128".to_string()), (4, "/sys/class/thermal/thermal_zone0/temp".to_string()), (6, "/proc/{}/stat".to_string())]
        );
        assert_eq!(targets(extract_ts("const a = '/dev/video0'; const b = `/proc/cpuinfo`; // '/dev/x'\nconst c = \"/dev\";\n")), vec![(1, "/dev/video0".to_string()), (1, "/proc/cpuinfo".to_string())]);
        assert_eq!(targets(extract_py("a = open('/sys/class/gpio/export')\n# b = '/dev/none'\nc = '/etc/x'\n")), vec![(1, "/sys/class/gpio/export".to_string())]);
        // aggregate
        with_project("The module \"a\" is implemented in \"src/**\".\n", |m| {
            let l = |_: &str| Some(rs.to_string());
            let t = text(m, &aggregate(m, &collect_facts(m, &["src/a.rs".to_string()], &l)));
            assert!(t.contains("device a /dev/dri/renderD128 e.g. src/a.rs:2\n"), "{t}");
        });
    }

    fn progs_sh(src: &str) -> Vec<(usize, String)> {
        extract_sh(src).into_iter().filter_map(|o| if let Raw::Program(p) = o.raw { Some((o.line, p)) } else { None }).collect()
    }

    #[test]
    fn shell_only_in_command_position() {
        let src = "#!/bin/bash\n# ssh commented\nssh host uptime # curl in a comment\necho x | rsync -a a b\ncd d && cargo build\n\
FOO=1 sudo -u root systemctl restart x\nv=$(git rev-parse HEAD)\necho \"ssh is not a command\"\necho ssh scp\n\
if curl -s u; then wget y; fi\ncat > f <<'EOF'\nssh inside the heredoc\nEOF\nexec openssl x\n/usr/bin/docker ps\n\
foo || make\necho \"$(node -v)\"\nssh) x ;;\nssh_agent\nsudo python3 a.py\n";
        let expected: Vec<(usize, &str)> = vec![
            (3, "ssh"),
            (4, "rsync"),
            (5, "cargo"),
            (6, "systemctl"),
            (7, "git"),
            (10, "curl"),
            (10, "wget"),
            (14, "openssl"),
            (15, "docker"),
            (16, "make"),
            (17, "node"),
            (20, "python3"),
        ];
        let expected: Vec<(usize, String)> = expected.into_iter().map(|(l, p)| (l, p.to_string())).collect();
        assert_eq!(progs_sh(src), expected);
        assert_eq!(progs_sh("sudo -n apt-get install x\nwhile ! ffmpeg -i a; do sqlite3 b; done\nnpm run x && npx y\nchronyc tracking; chrony x\n").iter().map(|x| x.1.as_str()).collect::<Vec<_>>(),
            vec!["apt-get", "ffmpeg", "sqlite3", "npm", "npx", "chronyc", "chrony"]);
    }

    #[test]
    fn shell_by_extension_or_shebang() {
        let r = |file: &str, src: &str| extract_file(file, src, &[]).len();
        assert_eq!(r("i/a.sh", "ssh x\n"), 1);
        assert_eq!(r("i/run", "#!/usr/bin/env bash\nssh x\n"), 1);
        assert_eq!(r("i/run", "#!/bin/sh\nssh x\n"), 1);
        assert_eq!(r("i/run", "#!/usr/bin/python3\nssh x\n"), 0);
        assert_eq!(r("i/run", "ssh x\n"), 0);
        assert_eq!(r("i.d/run", "#!/bin/sh\nssh x\n"), 1);
    }
}
