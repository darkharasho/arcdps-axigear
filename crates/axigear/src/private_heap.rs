//! Global allocator backed by a private Win32 heap.
//!
//! The default `System` allocator uses the shared process heap, whose
//! lock the game's render thread also takes; under Wine an allocation
//! burst on our threads contends it and hitches a frame. A heap from
//! `HeapCreate` has its own lock, so only our own threads share it.
//!
//! This replaces mimalloc, which hooks DLL_THREAD_DETACH for every
//! thread in the process and crashed in `_mi_thread_done` when a game
//! thread exited. This allocator installs no thread hooks.
//!
//! The heap is created on first use and never destroyed, so a late free
//! (a thread still winding down at unload) always finds a live heap.

use std::alloc::{GlobalAlloc, Layout};
use std::ffi::c_void;
use std::ptr;
use std::sync::atomic::{AtomicPtr, Ordering};

use windows::Win32::Foundation::HANDLE;
use windows::Win32::System::Memory::{
    HeapAlloc, HeapCreate, HeapFree, HeapReAlloc, HEAP_FLAGS, HEAP_ZERO_MEMORY,
};

/// `HeapAlloc` guarantees this alignment on x86_64
/// (MEMORY_ALLOCATION_ALIGNMENT).
const MIN_ALIGN: usize = 16;

static HEAP: AtomicPtr<c_void> = AtomicPtr::new(ptr::null_mut());

pub struct PrivateHeap;

fn heap() -> Option<HANDLE> {
    let h = HEAP.load(Ordering::Acquire);
    if !h.is_null() {
        return Some(HANDLE(h));
    }
    // Growable, serialized. A racing thread may create a second heap;
    // the loser's stays empty and is leaked, which is harmless.
    let new = unsafe { HeapCreate(HEAP_FLAGS(0), 0, 0) }.ok()?;
    match HEAP.compare_exchange(ptr::null_mut(), new.0, Ordering::AcqRel, Ordering::Acquire) {
        Ok(_) => Some(new),
        Err(existing) => Some(HANDLE(existing)),
    }
}

unsafe fn alloc_with(layout: Layout, flags: HEAP_FLAGS) -> *mut u8 {
    let Some(h) = heap() else { return ptr::null_mut() };
    if layout.align() <= MIN_ALIGN {
        return unsafe { HeapAlloc(h, flags, layout.size()) } as *mut u8;
    }
    // Over-allocate, align inside the block, and keep the raw pointer
    // in the word just before the aligned one (same scheme as std).
    let Some(total) = layout.size().checked_add(layout.align()) else {
        return ptr::null_mut();
    };
    let raw = unsafe { HeapAlloc(h, flags, total) } as *mut u8;
    if raw.is_null() {
        return raw;
    }
    let offset = layout.align() - (raw as usize & (layout.align() - 1));
    unsafe {
        let aligned = raw.add(offset);
        *(aligned as *mut *mut u8).sub(1) = raw;
        aligned
    }
}

unsafe impl GlobalAlloc for PrivateHeap {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        unsafe { alloc_with(layout, HEAP_FLAGS(0)) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        unsafe { alloc_with(layout, HEAP_ZERO_MEMORY) }
    }

    unsafe fn dealloc(&self, p: *mut u8, layout: Layout) {
        let raw = if layout.align() <= MIN_ALIGN {
            p
        } else {
            unsafe { *(p as *mut *mut u8).sub(1) }
        };
        // Only reachable after a successful alloc, so the heap exists.
        let h = HANDLE(HEAP.load(Ordering::Acquire));
        let _ = unsafe { HeapFree(h, HEAP_FLAGS(0), Some(raw as *const c_void)) };
    }

    unsafe fn realloc(&self, p: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        if layout.align() <= MIN_ALIGN {
            let h = HANDLE(HEAP.load(Ordering::Acquire));
            return unsafe { HeapReAlloc(h, HEAP_FLAGS(0), Some(p as *const c_void), new_size) }
                as *mut u8;
        }
        // Over-aligned: allocate, copy, free.
        let new_layout = unsafe { Layout::from_size_align_unchecked(new_size, layout.align()) };
        let new = unsafe { self.alloc(new_layout) };
        if !new.is_null() {
            unsafe {
                ptr::copy_nonoverlapping(p, new, layout.size().min(new_size));
                self.dealloc(p, layout);
            }
        }
        new
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alignments_sizes_and_realloc_round_trip() {
        let a = PrivateHeap;
        for align in [1usize, 8, 16, 32, 64, 4096] {
            for size in [1usize, 7, 64, 1000, 1 << 20] {
                let layout = Layout::from_size_align(size, align).unwrap();
                unsafe {
                    let p = a.alloc_zeroed(layout);
                    assert!(!p.is_null());
                    assert_eq!(p as usize % align, 0);
                    assert!((0..size).all(|i| *p.add(i) == 0));
                    ptr::write_bytes(p, 0xAB, size);
                    let q = a.realloc(p, layout, size * 3);
                    assert!(!q.is_null());
                    assert_eq!(q as usize % align, 0);
                    assert!((0..size).all(|i| *q.add(i) == 0xAB));
                    a.dealloc(q, Layout::from_size_align(size * 3, align).unwrap());
                }
            }
        }
    }

    #[test]
    fn threads_allocate_and_exit() {
        let handles: Vec<_> = (0..16)
            .map(|i| {
                std::thread::spawn(move || {
                    let v: Vec<Vec<u8>> = (0..1000).map(|j| vec![i as u8; j % 300]).collect();
                    v.iter().map(|x| x.len()).sum::<usize>()
                })
            })
            .collect();
        for h in handles {
            assert!(h.join().unwrap() > 0);
        }
    }
}
