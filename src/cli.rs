//! Command line: reads `-C root`, runs the command on that directory and prints its output.

use crate::ctx::Ctx;
use crate::vfs::DiskFs;
use crate::{intent, run_command};
use std::io::Write;

/// Runs the command line (arguments after the program name) and returns the exit code.
pub fn run(args: Vec<String>) -> i32 {
    let mut start: Option<String> = None;
    let mut rest = Vec::new();
    let mut it = args.into_iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "-C" => match it.next() {
                Some(d) => start = Some(d),
                None => {
                    let fs = DiskFs::new(".");
                    let cx = Ctx::new(&fs);
                    let code = intent::commands::usage(&cx, "-C requires a directory");
                    return emit(cx.finish(code));
                }
            },
            _ => rest.push(a),
        }
    }
    let fs = DiskFs::new(start.unwrap_or_else(|| ".".to_string()));
    emit(run_command(&fs, &rest))
}

/// Prints stdout, then stderr, and passes the exit code on.
fn emit(o: crate::Output) -> i32 {
    let _ = std::io::stdout().write_all(o.stdout.as_bytes());
    let _ = std::io::stdout().flush();
    let _ = std::io::stderr().write_all(o.stderr.as_bytes());
    o.code
}
