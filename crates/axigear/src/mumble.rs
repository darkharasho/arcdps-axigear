#![cfg(windows)]
//! Copies GW2's MumbleLink shared memory into a `MumbleSample` once a second
//! (called from the worker thread). Parsing lives in axigear-core.

use std::time::{Duration, Instant};

use axigear_core::mumble::MumbleSample;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::Memory::{MapViewOfFile, OpenFileMappingW, UnmapViewOfFile, FILE_MAP_READ, MEMORY_MAPPED_VIEW_ADDRESS};

/// https://wiki.guildwars2.com/wiki/API:MumbleLink
#[repr(C)]
#[derive(Clone, Copy)]
struct LinkedMem {
    ui_version: u32,
    ui_tick: u32,
    avatar_pos: [f32; 3],
    avatar_front: [f32; 3],
    avatar_top: [f32; 3],
    name: [u16; 256],
    camera_pos: [f32; 3],
    camera_front: [f32; 3],
    camera_top: [f32; 3],
    identity: [u16; 256],
    context_len: u32,
    context: [u8; 256],
    description: [u16; 2048],
}

struct Handle {
    file: HANDLE,
    view: *const LinkedMem,
}

impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            if !self.view.is_null() {
                let _ = UnmapViewOfFile(MEMORY_MAPPED_VIEW_ADDRESS { Value: self.view as *mut _ });
            }
            if !self.file.is_invalid() {
                let _ = CloseHandle(self.file);
            }
        }
    }
}

// Read-only snapshots of a mapping the game owns; only the worker touches it.
unsafe impl Send for Handle {}

const RETRY: Duration = Duration::from_secs(10);

pub struct Reader {
    handle: Option<Handle>,
    last_try: Option<Instant>,
}

impl Reader {
    pub fn new() -> Self {
        Reader { handle: None, last_try: None }
    }

    fn open() -> Option<Handle> {
        // Honour `-mumble <name>`: multi-client launchers give each client its own link.
        let args = std::env::args_os().map(|a| a.to_string_lossy().into_owned());
        let name: Vec<u16> = axigear_core::mumble::link_name(args).encode_utf16().chain(std::iter::once(0)).collect();
        let file = match unsafe { OpenFileMappingW(FILE_MAP_READ.0, false, PCWSTR(name.as_ptr())) } {
            Ok(h) if !h.is_invalid() => h,
            _ => return None,
        };
        let view = unsafe { MapViewOfFile(file, FILE_MAP_READ, 0, 0, std::mem::size_of::<LinkedMem>()) };
        if view.Value.is_null() {
            unsafe {
                let _ = CloseHandle(file);
            }
            return None;
        }
        Some(Handle { file, view: view.Value as *const LinkedMem })
    }

    pub fn sample(&mut self) -> Option<MumbleSample> {
        if self.handle.is_none() {
            let now = Instant::now();
            if self.last_try.is_some_and(|t| now.duration_since(t) < RETRY) {
                return None;
            }
            self.last_try = Some(now);
            self.handle = Self::open();
            if self.handle.is_none() {
                log::warn!("axigear: MumbleLink not available yet; retrying");
            }
        }
        let h = self.handle.as_ref()?;
        let mem: LinkedMem = unsafe { std::ptr::read_volatile(h.view) };
        let end = mem.identity.iter().position(|c| *c == 0).unwrap_or(mem.identity.len());
        let ctx_len = (mem.context_len as usize).min(mem.context.len());
        Some(MumbleSample {
            ui_tick: mem.ui_tick,
            identity: String::from_utf16_lossy(&mem.identity[..end]),
            context: mem.context[..ctx_len].to_vec(),
        })
    }
}
