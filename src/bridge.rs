//! The JSON side of embedding intentree (WebAssembly, or any host that talks text): a project
//! as a JSON object of path to text, arguments as a JSON array of strings, the result as
//! `{"code":n,"stdout":"...","stderr":"..."}`. Pure functions, so they run the same natively.

use crate::intent::json::{parse, text, J};
use crate::{run_command, Fs, MemFs, Output};
use std::collections::BTreeMap;

/// `{"path": "text", ...}` into the files of a project.
pub fn files_from_json(src: &str) -> Result<BTreeMap<String, String>, String> {
    let J::Obj(pairs) = parse(src)? else { return Err("the files must be a JSON object of path to text".to_string()) };
    let mut files = BTreeMap::new();
    for (k, v) in pairs {
        let Some(t) = v.as_str() else { return Err(format!("the file {k} must be text")) };
        files.insert(k, t.to_string());
    }
    Ok(files)
}

/// `["check", "--file", "a.intent"]` into the arguments of a command.
pub fn args_from_json(src: &str) -> Result<Vec<String>, String> {
    let J::Arr(items) = parse(src)? else { return Err("the arguments must be a JSON array of strings".to_string()) };
    items.iter().map(|x| x.as_str().map(String::from).ok_or_else(|| "the arguments must be a JSON array of strings".to_string())).collect()
}

pub fn output_json(o: &Output) -> String {
    format!("{{\"code\":{},\"stdout\":{},\"stderr\":{}}}", o.code, text(&o.stdout), text(&o.stderr))
}

/// Runs the command given as a JSON array; a malformed array comes back as code 2 with the reason on stderr.
pub fn run_json(fs: &dyn Fs, args: &str) -> String {
    match args_from_json(args) {
        Ok(a) => output_json(&run_command(fs, &a)),
        Err(e) => output_json(&Output { code: 2, stdout: String::new(), stderr: format!("{e}\n") }),
    }
}

/// A project in memory from the JSON of `files_from_json`.
pub fn mem_fs_from_json(src: &str) -> Result<MemFs, String> {
    files_from_json(src).map(MemFs::new)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn files_and_arguments_round_trip() {
        let m = files_from_json("{\"a/b.rs\": \"fn x() {}\\n\", \"c\": \"\\u00e9\\ud83d\\ude00\"}").unwrap();
        assert_eq!(m["a/b.rs"], "fn x() {}\n");
        assert_eq!(m["c"], "é😀");
        assert!(files_from_json("[1]").is_err() && files_from_json("{\"a\": 1}").is_err() && files_from_json("{\"a\": \"x\"} x").is_err());
        assert_eq!(args_from_json("[\"check\", \"-v\"]").unwrap(), vec!["check", "-v"]);
        assert!(args_from_json("{}").is_err() && args_from_json("[1]").is_err());
    }

    /// Every output goes through the context: nothing may be left on the process's stdout.
    #[test]
    fn every_command_prints_into_the_output() {
        let intent = "The program \"p\" contains the module \"alpha\" and the module \"beta\".\nThe module \"alpha\" is implemented in \"src/alpha.rs\".\nThe module \"beta\" is implemented in \"src/beta.rs\".\nThe module \"alpha\" uses the module \"beta\".\n";
        let mut files = BTreeMap::new();
        files.insert("p.intent".to_string(), intent.to_string());
        files.insert("src/alpha.rs".to_string(), "use crate::beta;\npub fn go() {}\n".to_string());
        files.insert("src/beta.rs".to_string(), "pub fn x() {}\n".to_string());
        let fs = MemFs::new(files);
        let run = |a: &[&str]| run_command(&fs, &a.iter().map(|s| s.to_string()).collect::<Vec<_>>());
        for args in [
            &["check"][..],
            &["show", "src/alpha.rs"],
            &["where", "alpha"],
            &["where", "alpha", "--format", "json"],
            &["view"],
            &["view", "structure", "--format", "svg"],
            &["views"],
            &["facts"],
            &["coverage"],
            &["ir"],
            &["canon"],
            &["lexicon", "list"],
            &["lexicon", "check"],
            &["lexicon", "pending"],
        ] {
            let o = run(args);
            assert_eq!(o.code, 0, "{args:?}: {o:?}");
            assert!(!o.stdout.is_empty(), "{args:?} printed nothing");
        }
        let o = run(&["lexicon", "propose", "type", "gizmo", "--meaning", "a thing"]);
        assert_eq!(o.code, 2);
        assert!(o.stderr.contains("not available in memory"), "{o:?}");
        assert_eq!(run(&[]).code, 2);
    }

    #[test]
    fn runs_a_command_on_a_project_in_memory() {
        let fs = mem_fs_from_json("{\"a.intent\": \"The program \\\"p\\\" contains the module \\\"m\\\".\\n\"}").unwrap();
        let r = run_json(&fs, "[\"check\"]");
        assert!(r.starts_with("{\"code\":0,\"stdout\":\"") && r.contains("0 errors"), "{r}");
        let r = run_json(&fs, "[\"stale\"]");
        assert!(r.starts_with("{\"code\":2,") && r.contains("not on disk"), "{r}");
        let r = run_json(&fs, "nope");
        assert!(r.starts_with("{\"code\":2,"), "{r}");
        let r = run_json(&MemFs::new(BTreeMap::new()), "[\"check\"]");
        assert!(r.starts_with("{\"code\":2,") && r.contains("no .intent file"), "{r}");
    }
}
