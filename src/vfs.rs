//! File access behind a trait: the commands read the project through `Fs`, so they run on a
//! directory (`DiskFs`) or on a map of path to text held in memory (`MemFs`, for the browser).

use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

/// What the commands need from the project's files. Paths are the ones the code builds by
/// joining `root()` with a project-relative path, or an absolute path given by the user.
pub trait Fs {
    /// The project root as it was given (`.` by default); it is only used to build paths and messages.
    fn root(&self) -> &Path;
    /// Canonical form of the path (symlinks resolved on disk); the path itself when it cannot be resolved.
    fn canonicalize(&self, p: &Path) -> PathBuf;
    /// Contents as text; the error is the reason, as a message.
    fn read_to_string(&self, p: &Path) -> Result<String, String>;
    /// Contents as bytes; `None` if it cannot be read.
    fn read(&self, p: &Path) -> Option<Vec<u8>>;
    fn is_file(&self, p: &Path) -> bool;
    fn is_dir(&self, p: &Path) -> bool;
    fn exists(&self, p: &Path) -> bool {
        self.is_file(p) || self.is_dir(p)
    }
    /// Names of the regular files directly inside the directory (unsorted).
    fn list_dir_files(&self, dir: &Path) -> Vec<String>;
    /// The project's files: relative to the root, with `/`, sorted, existing files only
    /// (on disk: what git tracks or leaves untracked-but-not-ignored, or a directory scan).
    fn project_files(&self) -> Vec<String>;
    /// Writes the file, creating the directories above it. `MemFs` refuses.
    fn write(&self, p: &Path, text: &str) -> Result<(), String>;
    /// The directory on disk behind this project, for what needs git (`stale`); `None` in memory.
    fn disk_root(&self) -> Option<&Path>;
}

/// The project on disk: behavior of the command line.
pub struct DiskFs {
    root: PathBuf,
}

impl DiskFs {
    pub fn new(root: impl Into<PathBuf>) -> DiskFs {
        DiskFs { root: root.into() }
    }
}

impl Fs for DiskFs {
    fn root(&self) -> &Path {
        &self.root
    }

    fn canonicalize(&self, p: &Path) -> PathBuf {
        std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf())
    }

    fn read_to_string(&self, p: &Path) -> Result<String, String> {
        std::fs::read_to_string(p).map_err(|e| e.to_string())
    }

    fn read(&self, p: &Path) -> Option<Vec<u8>> {
        std::fs::read(p).ok()
    }

    fn is_file(&self, p: &Path) -> bool {
        p.is_file()
    }

    fn is_dir(&self, p: &Path) -> bool {
        p.is_dir()
    }

    fn exists(&self, p: &Path) -> bool {
        p.exists()
    }

    fn list_dir_files(&self, dir: &Path) -> Vec<String> {
        let Ok(rd) = std::fs::read_dir(dir) else { return Vec::new() };
        rd.flatten().filter(|e| e.path().is_file()).map(|e| e.file_name().to_string_lossy().to_string()).collect()
    }

    fn project_files(&self) -> Vec<String> {
        crate::files::list(&self.root, None)
    }

    fn write(&self, p: &Path, text: &str) -> Result<(), String> {
        if let Some(d) = p.parent() {
            std::fs::create_dir_all(d).map_err(|e| format!("{}: {e}", d.display()))?;
        }
        std::fs::write(p, text).map_err(|e| format!("{}: {e}", p.display()))
    }

    fn disk_root(&self) -> Option<&Path> {
        Some(&self.root)
    }
}

/// Directories the directory scan skips; they are also left out of `MemFs` listings.
pub const SKIPPED_DIRS: [&str; 4] = [".git", "target", "node_modules", "__pycache__"];

/// A project held in memory: relative paths (`/`-separated) to text, under an imaginary root.
pub struct MemFs {
    files: BTreeMap<String, String>,
    root: PathBuf,
}

/// The path as keys of the map use it: relative to the imaginary root, `/`-separated, without
/// `.` and with `..` resolved.
fn key_of(p: &Path) -> String {
    let mut v: Vec<String> = Vec::new();
    for c in p.components() {
        match c {
            Component::ParentDir => {
                v.pop();
            }
            Component::Normal(s) => v.push(s.to_string_lossy().to_string()),
            _ => {}
        }
    }
    v.join("/")
}

impl MemFs {
    pub fn new(files: BTreeMap<String, String>) -> MemFs {
        let files = files.into_iter().map(|(k, v)| (key_of(Path::new(&k)), v)).collect();
        MemFs { files, root: PathBuf::from(".") }
    }

    fn below(&self, dir: &str) -> impl Iterator<Item = &str> {
        let pre = if dir.is_empty() { String::new() } else { format!("{dir}/") };
        self.files.keys().filter_map(move |k| k.strip_prefix(pre.as_str())).collect::<Vec<_>>().into_iter()
    }
}

impl Fs for MemFs {
    fn root(&self) -> &Path {
        &self.root
    }

    fn canonicalize(&self, p: &Path) -> PathBuf {
        Path::new("/").join(key_of(p))
    }

    fn read_to_string(&self, p: &Path) -> Result<String, String> {
        self.files.get(&key_of(p)).cloned().ok_or_else(|| "No such file or directory (os error 2)".to_string())
    }

    fn read(&self, p: &Path) -> Option<Vec<u8>> {
        self.files.get(&key_of(p)).map(|t| t.as_bytes().to_vec())
    }

    fn is_file(&self, p: &Path) -> bool {
        self.files.contains_key(&key_of(p))
    }

    fn is_dir(&self, p: &Path) -> bool {
        let k = key_of(p);
        !self.files.contains_key(&k) && self.below(&k).next().is_some()
    }

    fn list_dir_files(&self, dir: &Path) -> Vec<String> {
        self.below(&key_of(dir)).filter(|r| !r.contains('/')).map(String::from).collect()
    }

    fn project_files(&self) -> Vec<String> {
        self.files
            .keys()
            .filter(|k| !k.split('/').rev().skip(1).any(|d| SKIPPED_DIRS.contains(&d)))
            .cloned()
            .collect()
    }

    fn write(&self, _p: &Path, _text: &str) -> Result<(), String> {
        Err("writing is not available in memory".to_string())
    }

    fn disk_root(&self) -> Option<&Path> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mem(files: &[(&str, &str)]) -> MemFs {
        MemFs::new(files.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect())
    }

    #[test]
    fn paths_are_relative_to_the_imaginary_root() {
        let m = mem(&[("a.intent", "x"), ("src/b.rs", "y"), ("src/deep/c.rs", "z"), ("./d.txt", "w")]);
        assert_eq!(m.read_to_string(&m.root().join("src/b.rs")), Ok("y".to_string()));
        assert_eq!(m.read_to_string(Path::new("/src/../a.intent")), Ok("x".to_string()));
        assert!(m.is_file(Path::new("d.txt")));
        assert!(m.is_dir(Path::new("./src")) && !m.is_dir(Path::new("src/b.rs")) && !m.is_dir(Path::new("nope")));
        assert!(m.exists(Path::new("src")) && !m.exists(Path::new("nope")));
        assert!(m.read_to_string(Path::new("nope")).is_err());
        let mut v = m.list_dir_files(Path::new("src"));
        v.sort();
        assert_eq!(v, vec!["b.rs"]);
        assert_eq!(m.canonicalize(Path::new("./src/../a.intent")), Path::new("/a.intent"));
    }

    #[test]
    fn listing_is_sorted_and_skips_build_directories() {
        let m = mem(&[("z.rs", ""), ("a.rs", ""), ("target/x", ""), ("src/__pycache__/m.pyc", ""), ("node_modules/p/i.js", ""), (".github/ci.yml", "")]);
        assert_eq!(m.project_files(), vec![".github/ci.yml", "a.rs", "z.rs"]);
    }

    #[test]
    fn memory_is_read_only_and_has_no_disk() {
        let m = mem(&[]);
        assert!(m.write(Path::new("a"), "b").is_err());
        assert!(m.disk_root().is_none());
    }
}
