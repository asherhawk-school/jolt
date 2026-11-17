//! Raw WebAssembly bindings (no wasm-bindgen): the page passes UTF-8 buffers in
//! and receives output through imported `host_*` functions.

use crate::{compile_and_run, Host};

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

/// Compile (and with `flags & 1`, run; with `flags & 2`, disassemble) a Java source.
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
    let source = String::from_utf8_lossy(std::slice::from_raw_parts(src, src_len)).into_owned();
    let stdin = String::from_utf8_lossy(std::slice::from_raw_parts(input, input_len)).into_owned();
    compile_and_run(
        Box::new(Web),
        &source,
        Some(&stdin),
        &[],
        flags & 1 != 0,
        flags & 2 != 0,
    )
}
