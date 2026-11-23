//! What a command runs against: the project's files (`Fs`) and the text it prints. Nothing
//! writes to the process's stdout or stderr; the caller decides what to do with `Output`.

use crate::vfs::Fs;
use std::cell::RefCell;
use std::fmt;

/// Result of a command: exit code and everything it printed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Output {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

pub struct Ctx<'a> {
    pub fs: &'a dyn Fs,
    out: RefCell<String>,
    err: RefCell<String>,
}

impl<'a> Ctx<'a> {
    pub fn new(fs: &'a dyn Fs) -> Ctx<'a> {
        Ctx { fs, out: RefCell::new(String::new()), err: RefCell::new(String::new()) }
    }

    pub fn put_out(&self, a: fmt::Arguments) {
        fmt::write(&mut *self.out.borrow_mut(), a).expect("writing to a String");
    }

    pub fn put_err(&self, a: fmt::Arguments) {
        fmt::write(&mut *self.err.borrow_mut(), a).expect("writing to a String");
    }

    pub fn finish(self, code: i32) -> Output {
        Output { code, stdout: self.out.into_inner(), stderr: self.err.into_inner() }
    }
}

/// `print!` into the context's stdout.
#[macro_export]
macro_rules! out {
    ($cx:expr, $($a:tt)*) => { $cx.put_out(format_args!($($a)*)) };
}

/// `println!` into the context's stdout.
#[macro_export]
macro_rules! outln {
    ($cx:expr) => { $cx.put_out(format_args!("\n")) };
    ($cx:expr, $($a:tt)*) => {{ $cx.put_out(format_args!($($a)*)); $cx.put_out(format_args!("\n")) }};
}

/// `eprint!` into the context's stderr.
#[macro_export]
macro_rules! err {
    ($cx:expr, $($a:tt)*) => { $cx.put_err(format_args!($($a)*)) };
}

/// `eprintln!` into the context's stderr.
#[macro_export]
macro_rules! errln {
    ($cx:expr, $($a:tt)*) => {{ $cx.put_err(format_args!($($a)*)); $cx.put_err(format_args!("\n")) }};
}
