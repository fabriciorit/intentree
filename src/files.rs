//! Project root and file list.

use std::fs;
use std::path::Path;
use std::process::Command;

/// Paths relative to the root, with `/`, sorted, existing files only, without the map (`skip`).
pub fn list(root: &Path, skip: Option<&str>) -> Vec<String> {
    let mut v = git_list(root).unwrap_or_else(|| {
        let mut v = Vec::new();
        walk(root, "", &mut v);
        v
    });
    v.retain(|f| Some(f.as_str()) != skip && root.join(f).is_file());
    v.sort();
    v.dedup();
    v
}

fn git_list(root: &Path) -> Option<Vec<String>> {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["ls-files", "-z", "-co", "--exclude-standard"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout);
    Some(s.split('\0').filter(|x| !x.is_empty()).map(String::from).collect())
}

fn walk(dir: &Path, prefix: &str, out: &mut Vec<String>) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        let rel = format!("{prefix}{name}");
        let p = e.path();
        if p.is_dir() {
            if !(name.starts_with('.') || ["target", "node_modules", "__pycache__"].contains(&name.as_str())) {
                walk(&p, &format!("{rel}/"), out);
            }
        } else {
            out.push(rel);
        }
    }
}
