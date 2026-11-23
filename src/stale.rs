//! Support for `stale`: git, instant of the last `.intent` change and output formatting.

use std::collections::BTreeSet;
use std::path::Path;
use std::process::Command;

fn git(dir: &Path, args: &[&str]) -> Option<Vec<u8>> {
    let o = Command::new("git").arg("-C").arg(dir).args(["-c", "core.quotepath=false"]).args(args).output().ok()?;
    o.status.success().then_some(o.stdout)
}

fn git_txt(dir: &Path, args: &[&str]) -> Option<String> {
    git(dir, args).map(|b| String::from_utf8_lossy(&b).to_string())
}

/// Last change of the file: date of the last commit that touched it; with no commit, the file's mtime.
pub fn instant_of_map(map: &Path) -> Option<u64> {
    let dir = map.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let name = map.file_name()?.to_string_lossy().to_string();
    if let Some(t) = git_txt(dir, &["log", "-1", "--format=%ct", "--", &name]) {
        if let Ok(n) = t.trim().parse::<u64>() {
            return Some(n);
        }
    }
    let m = std::fs::metadata(map).ok()?.modified().ok()?;
    Some(m.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs())
}

/// Paths from `status --porcelain -z` (relative to the repository root); renames give the destination.
fn parse_status(b: &[u8]) -> Vec<String> {
    let s = String::from_utf8_lossy(b);
    let mut it = s.split('\0').filter(|x| !x.is_empty());
    let mut out = Vec::new();
    while let Some(e) = it.next() {
        if e.len() < 4 {
            continue;
        }
        out.push(e[3..].to_string());
        if e.starts_with('R') || e.starts_with('C') || e[1..].starts_with('R') || e[1..].starts_with('C') {
            it.next(); // source path
        }
    }
    out
}

/// Files changed since `ts` (exclusive), relative to the project root.
pub fn changed(root: &Path, ts: u64) -> Option<BTreeSet<String>> {
    let prefix = git_txt(root, &["rev-parse", "--show-prefix"])?.trim().to_string();
    let since = format!("--since={}", ts + 1);
    let log = git_txt(root, &["log", &since, "--name-only", "--format="])?;
    let st = git(root, &["status", "--porcelain", "-z", "-uall"])?;
    let mut set = BTreeSet::new();
    let all = log.lines().map(String::from).chain(parse_status(&st));
    for f in all {
        if f.is_empty() {
            continue;
        }
        if let Some(r) = f.strip_prefix(&prefix) {
            set.insert(r.to_string());
        }
    }
    Some(set)
}

pub const MSG_NO_GIT: &str = "stale requires git: the project is not in a repository (or git is not installed)\n";

pub const MSG_NOT_ON_DISK: &str = "stale is not available here: it reads git history and file dates, and this project is not on disk\n";

pub fn has_git(root: &Path) -> bool {
    git_txt(root, &["rev-parse", "--show-toplevel"]).is_some()
}

/// Output of `stale`: components with changed files (most files first, ties by the
/// name), loose files and rules to review.
pub fn format_output(mut groups: Vec<(String, Vec<String>)>, loose: Vec<String>, rules: Vec<String>) -> String {
    groups.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then_with(|| a.0.cmp(&b.0)));
    let mut o = String::new();
    for (c, fs) in groups {
        o.push_str(&format!("{c} {} e.g. {}\n", fs.len(), fs[0]));
    }
    for f in loose {
        o.push_str(&format!("no component {f}\n"));
    }
    if !rules.is_empty() {
        o.push_str("sentences to review\n");
        for r in rules {
            o.push_str(&r);
            o.push('\n');
        }
    }
    o
}

pub fn truncate_line(t: &str) -> String {
    if t.chars().count() <= 70 {
        return t.to_string();
    }
    let c: String = t.chars().take(70).collect();
    format!("{}…", c.trim_end())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_porcelain_with_rename() {
        let b = b" M src/a.rs\0?? new.py\0R  dest.rs\0orig.rs\0A  b/c d.txt\0";
        assert_eq!(parse_status(b), vec!["src/a.rs", "new.py", "dest.rs", "b/c d.txt"]);
    }
}
