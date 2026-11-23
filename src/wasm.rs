//! WebAssembly exports with the plain C ABI (no wasm-bindgen, no imports): the host copies
//! bytes in with `alloc`, calls `set_files` and `run`, reads the length-prefixed answer and
//! gives every buffer back with `dealloc`. `web/intentree.js` wraps this.
//!
//! Buffers coming out are `[u32 little-endian length][UTF-8 bytes]`, `4 + length` bytes in
//! all. A panic aborts the instance (that target has no unwinding): the hook keeps its message
//! so the host can read it with `last_panic` after the trap, then discard the instance.

use crate::bridge;
use crate::MemFs;
use std::cell::RefCell;
use std::rc::Rc;

thread_local! {
    static FILES: RefCell<Option<Rc<MemFs>>> = const { RefCell::new(None) };
    static PANIC: RefCell<String> = const { RefCell::new(String::new()) };
}

/// A buffer of `len` bytes for the host to fill.
#[no_mangle]
pub extern "C" fn alloc(len: usize) -> *mut u8 {
    let mut v = Vec::<u8>::with_capacity(len.max(1));
    let p = v.as_mut_ptr();
    std::mem::forget(v);
    p
}

/// Gives back a buffer from `alloc` (with the same `len`) or the answer of `run`, `set_files`
/// and `last_panic` (with `4 + length`).
///
/// # Safety
/// `ptr` and `len` must be exactly what `alloc` or one of the answers handed out.
#[no_mangle]
pub unsafe extern "C" fn dealloc(ptr: *mut u8, len: usize) {
    drop(Vec::from_raw_parts(ptr, 0, len.max(1)));
}

fn reply(text: &str) -> *mut u8 {
    let mut v = Vec::with_capacity(4 + text.len());
    v.extend_from_slice(&(text.len() as u32).to_le_bytes());
    v.extend_from_slice(text.as_bytes());
    v.shrink_to_fit();
    let mut v = std::mem::ManuallyDrop::new(v.into_boxed_slice());
    v.as_mut_ptr()
}

unsafe fn read_text<'a>(ptr: *const u8, len: usize) -> &'a str {
    std::str::from_utf8(std::slice::from_raw_parts(ptr, len)).unwrap_or("")
}

fn hook() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        std::panic::set_hook(Box::new(|info| {
            let at = info.location().map(|l| format!(" at {}:{}", l.file(), l.line())).unwrap_or_default();
            let msg = info.payload().downcast_ref::<&str>().map(|s| s.to_string()).or_else(|| info.payload().downcast_ref::<String>().cloned()).unwrap_or_default();
            PANIC.with(|p| *p.borrow_mut() = format!("panic: {msg}{at}"));
        }));
    });
}

/// Replaces the project: a JSON object of path to text. The answer is empty on success, or the reason.
///
/// # Safety
/// `ptr` and `len` must describe bytes written into a buffer from `alloc`.
#[no_mangle]
pub unsafe extern "C" fn set_files(ptr: *const u8, len: usize) -> *mut u8 {
    hook();
    match bridge::mem_fs_from_json(read_text(ptr, len)) {
        Ok(fs) => {
            FILES.with(|f| *f.borrow_mut() = Some(Rc::new(fs)));
            reply("")
        }
        Err(e) => reply(&e),
    }
}

/// Runs a command given as a JSON array of strings (`["check"]`) on the project from
/// `set_files` (empty if none). The answer is `{"code":n,"stdout":"...","stderr":"..."}`.
///
/// # Safety
/// `ptr` and `len` must describe bytes written into a buffer from `alloc`.
#[no_mangle]
pub unsafe extern "C" fn run(ptr: *const u8, len: usize) -> *mut u8 {
    hook();
    let fs = FILES.with(|f| f.borrow().clone()).unwrap_or_else(|| Rc::new(MemFs::new(Default::default())));
    reply(&bridge::run_json(&*fs, read_text(ptr, len)))
}

/// The message of the last panic, or empty. Readable after the instance trapped.
#[no_mangle]
pub extern "C" fn last_panic() -> *mut u8 {
    reply(&PANIC.with(|p| p.borrow().clone()))
}
