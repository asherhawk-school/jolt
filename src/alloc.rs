//! A small, fast allocator for a short-lived compiler process.
//!
//! Compilers make millions of tiny allocations (tree nodes, short strings) and
//! free most of them soon after. Requests up to 512 bytes are served from
//! per-thread size-class free lists carved out of 256 KB chunks: allocation and
//! deallocation are a couple of pointer moves, with no locking. Larger requests
//! go to the system allocator. Chunks are never returned to the OS, which is
//! fine for a process that lives for milliseconds.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::ptr;

const GRAIN: usize = 16;
const MAX_SMALL: usize = 512;
const CLASSES: usize = MAX_SMALL / GRAIN;
const CHUNK: usize = 256 * 1024;

struct Local {
    free: [Cell<*mut u8>; CLASSES],
    bump: Cell<*mut u8>,
    end: Cell<*mut u8>,
}

thread_local! {
    static LOCAL: Local = const {
        Local {
            free: [const { Cell::new(ptr::null_mut()) }; CLASSES],
            bump: Cell::new(ptr::null_mut()),
            end: Cell::new(ptr::null_mut()),
        }
    };
}

pub struct JoltAlloc;

#[inline]
fn is_small(l: &Layout) -> bool {
    l.size() <= MAX_SMALL && l.align() <= GRAIN
}

#[inline]
fn class_of(size: usize) -> usize {
    (size.max(1) + GRAIN - 1) / GRAIN - 1
}

unsafe impl GlobalAlloc for JoltAlloc {
    #[inline]
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        if !is_small(&l) {
            return System.alloc(l);
        }
        let c = class_of(l.size());
        let size = (c + 1) * GRAIN;
        let r = LOCAL.try_with(|t| {
            let head = t.free[c].get();
            if !head.is_null() {
                t.free[c].set(*(head as *mut *mut u8));
                return head;
            }
            let mut p = t.bump.get();
            if p.is_null() || (t.end.get() as usize) - (p as usize) < size {
                p = System.alloc(Layout::from_size_align_unchecked(CHUNK, GRAIN));
                if p.is_null() {
                    return p;
                }
                t.end.set(p.add(CHUNK));
            }
            t.bump.set(p.add(size));
            p
        });
        match r {
            Ok(p) => p,
            // Thread-local storage unavailable (thread teardown): use a block of
            // the full class size so it can later be recycled safely.
            Err(_) => System.alloc(Layout::from_size_align_unchecked(size, GRAIN)),
        }
    }

    #[inline]
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        if !is_small(&l) {
            return System.dealloc(p, l);
        }
        let c = class_of(l.size());
        let _ = LOCAL.try_with(|t| {
            *(p as *mut *mut u8) = t.free[c].get();
            t.free[c].set(p);
        });
    }

    #[inline]
    unsafe fn realloc(&self, p: *mut u8, l: Layout, new_size: usize) -> *mut u8 {
        let new_l = Layout::from_size_align_unchecked(new_size, l.align());
        match (is_small(&l), is_small(&new_l)) {
            (false, false) => System.realloc(p, l, new_size),
            (true, true) if class_of(l.size()) == class_of(new_size) => p,
            _ => {
                let q = self.alloc(new_l);
                if !q.is_null() {
                    ptr::copy_nonoverlapping(p, q, l.size().min(new_size));
                    self.dealloc(p, l);
                }
                q
            }
        }
    }
}
