//! Raw WebAssembly bindings (no wasm-bindgen): the page passes UTF-8 buffers in
//! and receives output through imported `host_*` functions.

use std::cell::RefCell;

use crate::{start, Host, Input, Session, Started};

/// Returned instead of an exit code while a program waits for input.
const WAITING: i32 = i32::MIN;
/// Returned by `jolt_input`/`jolt_eof` when no program is waiting.
const NO_SESSION: i32 = i32::MIN + 1;

thread_local! {
    static SESSION: RefCell<Option<Session>> = const { RefCell::new(None) };
}

fn settle(s: Started) -> i32 {
    match s {
        Started::Done(code) => code,
        Started::Waiting(session) => {
            SESSION.with(|cell| *cell.borrow_mut() = Some(session));
            WAITING
        }
    }
}

fn resume(input: Option<&str>) -> i32 {
    match SESSION.with(|cell| cell.borrow_mut().take()) {
        Some(session) => settle(session.resume(input)),
        None => NO_SESSION,
    }
}

#[link(wasm_import_module = "env")]
extern "C" {
    fn host_write(fd: u32, ptr: *const u8, len: usize);
    fn host_now() -> f64;
    fn host_epoch() -> f64;
    fn host_random() -> f64;
    fn host_sleep(ms: f64);
}

struct Web;

impl Host for Web {
    fn write(&mut self, fd: u32, s: &str) {
        unsafe { host_write(fd, s.as_ptr(), s.len()) }
    }
    fn now_ms(&mut self) -> f64 {
        unsafe { host_now() }
    }
    fn epoch_ms(&mut self) -> f64 {
        unsafe { host_epoch() }
    }
    fn random(&mut self) -> f64 {
        unsafe { host_random() }
    }
    fn sleep_ms(&mut self, ms: f64) {
        unsafe { host_sleep(ms) }
    }
}

#[no_mangle]
pub extern "C" fn jolt_alloc(len: usize) -> *mut u8 {
    let mut v = Vec::<u8>::with_capacity(len.max(1));
    let p = v.as_mut_ptr();
    std::mem::forget(v);
    p
}

/// # Safety
/// `ptr`/`cap` must come from `jolt_alloc`.
#[no_mangle]
pub unsafe extern "C" fn jolt_free(ptr: *mut u8, cap: usize) {
    drop(Vec::from_raw_parts(ptr, 0, cap.max(1)));
}

/// Compile a Java source. Flags: 1 = run it, 2 = disassemble, 4 = interactive
/// input (pause for input instead of seeing end-of-input; `input` is ignored).
/// Returns the exit code, or `WAITING` if the program paused for input.
///
/// # Safety
/// The pointers must reference valid UTF-8 buffers of the given lengths.
#[no_mangle]
pub unsafe extern "C" fn jolt_run(
    src: *const u8,
    src_len: usize,
    input: *const u8,
    input_len: usize,
    flags: u32,
) -> i32 {
    SESSION.with(|cell| *cell.borrow_mut() = None);
    let source = String::from_utf8_lossy(std::slice::from_raw_parts(src, src_len)).into_owned();
    let stdin = String::from_utf8_lossy(std::slice::from_raw_parts(input, input_len)).into_owned();
    let input = if flags & 4 != 0 {
        Input::Interactive
    } else {
        Input::Given(&stdin)
    };
    settle(start(
        Box::new(Web),
        &source,
        input,
        &[],
        flags & 1 != 0,
        flags & 2 != 0,
    ))
}

/// Give a paused program more input and continue it.
///
/// # Safety
/// The pointer must reference a valid UTF-8 buffer of the given length.
#[no_mangle]
pub unsafe extern "C" fn jolt_input(ptr: *const u8, len: usize) -> i32 {
    let text = String::from_utf8_lossy(std::slice::from_raw_parts(ptr, len)).into_owned();
    resume(Some(&text))
}

/// Tell a paused program its input has ended (Ctrl+D) and continue it.
#[no_mangle]
pub extern "C" fn jolt_eof() -> i32 {
    resume(None)
}
