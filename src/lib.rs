//! intentree as a library: every command runs against an `Fs` (a directory or a map in
//! memory) and returns what it printed. The binary (`src/main.rs`) and the WebAssembly
//! exports (`src/wasm.rs`) are thin layers over `run_command`.

pub mod bridge;
pub mod cli;
pub mod ctx;
mod facts;
mod files;
mod glob;
mod intent;
mod measure;
mod stale;
mod symbol;
pub mod vfs;
#[cfg(target_arch = "wasm32")]
pub mod wasm;

pub use ctx::Output;
pub use vfs::{DiskFs, Fs, MemFs};

use ctx::Ctx;

/// Runs one command (`check`, `show`, `where`, ...) with the arguments the command line takes
/// after `-C root`, against the project in `fs`. Never prints and never exits.
pub fn run_command(fs: &dyn Fs, args: &[String]) -> Output {
    let cx = Ctx::new(fs);
    let code = match args.split_first() {
        None => intent::commands::usage(&cx, "missing command"),
        Some((cmd, rest)) => match cmd.as_str() {
            "ir" | "canon" | "lexicon" | "views" | "check" | "show" | "where" | "stale" | "facts" | "coverage" | "view" => intent::commands::run(cmd, rest, &cx),
            x => intent::commands::usage(&cx, &format!("unknown command: {x}")),
        },
    };
    cx.finish(code)
}
