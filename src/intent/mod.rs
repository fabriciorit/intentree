//! The `.intent` format: controlled English with a lexicon, a deterministic parser and the IR.

pub mod hints;
pub mod analysis;
pub mod layers;
pub mod canon;
pub mod code;
pub mod commands;
pub mod json;
pub mod lexicon;
pub mod locate;
pub mod project;
pub mod propose;
pub mod sha1;
pub mod syntax;
pub mod time;
pub mod views;

#[cfg(test)]
mod tests;

use analysis::Line;
use lexicon::{Entry, Lexicon};
use crate::vfs::Fs;
use std::path::PathBuf;

/// The project's `.intent` files: those at the root and in `intent/`, sorted by relative
/// path. Returns (relative path with `/`, absolute path).
pub fn files_of_project(fs: &dyn Fs) -> Vec<(String, PathBuf)> {
    let root = fs.root();
    let mut v = Vec::new();
    for (sub, prefix) in [("", ""), ("intent", "intent/")] {
        for name in fs.list_dir_files(&root.join(sub)) {
            let p = root.join(sub).join(&name);
            if p.extension().is_some_and(|x| x == "intent") {
                v.push((format!("{prefix}{name}"), p));
            }
        }
    }
    v.sort();
    v
}

/// Sentences and lexicon of a set of `.intent` files.
pub struct Source {
    pub lex: Lexicon,
    pub lines: Vec<Line>,
    /// names of the files read (relative to the root, or the one given in `--file`)
    pub files: Vec<String>,
    /// text of all the files, one after another
    pub text: String,
}

/// Joins the texts: `=` lines extend the embedded lexicon; the others are sentences.
/// With `single_file`, sentences carry the label "line N"; otherwise "file:N".
pub fn assemble(texts: &[(String, String)], single_file: bool) -> Result<Source, Vec<String>> {
    let mut entries = Vec::new();
    let mut lines = Vec::new();
    for (name, text) in texts {
        for (i, l) in syntax::split_lines(text).into_iter().enumerate() {
            let n = i + 1;
            if syntax::trim_blank(l).starts_with('=') {
                entries.push(Entry { label: format!("{name}:{n}"), text: l });
                continue;
            }
            let label = if single_file { format!("line {n}") } else { format!("{name}:{n}") };
            lines.push(Line { label, text: l.to_string(), file: name.clone() });
        }
    }
    let lex = Lexicon::load(&entries)?;
    let files = texts.iter().map(|(n, _)| n.clone()).collect();
    let text = texts.iter().map(|(_, t)| t.as_str()).collect::<Vec<_>>().join("\n");
    Ok(Source { lex, lines, files, text })
}
