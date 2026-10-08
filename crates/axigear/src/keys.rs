#![cfg(windows)]
//! WndProc side of the hotkey. Runs on GW2's window thread, so it only
//! touches atomics and tiny try_lock'd slots; the UI thread does the rest.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use crate::hotkey::{self, Hotkey};

static HOTKEY: Mutex<Option<Hotkey>> = Mutex::new(None);
static BINDING: AtomicBool = AtomicBool::new(false);
static BOUND: Mutex<Option<String>> = Mutex::new(None);
static TOGGLE: AtomicBool = AtomicBool::new(false);

#[link(name = "user32")]
extern "system" {
    fn GetAsyncKeyState(vk: i32) -> i16;
}

fn down(vk: i32) -> bool {
    unsafe { (GetAsyncKeyState(vk) as u16 & 0x8000) != 0 }
}

pub fn set_hotkey(s: &str) {
    if let Ok(mut g) = HOTKEY.try_lock() {
        *g = Hotkey::parse(s);
    }
}

pub fn start_binding() {
    BINDING.store(true, Ordering::Relaxed);
}

pub fn cancel_binding() {
    BINDING.store(false, Ordering::Relaxed);
}

pub fn binding() -> bool {
    BINDING.load(Ordering::Relaxed)
}

pub fn take_bound() -> Option<String> {
    BOUND.try_lock().ok()?.take()
}

pub fn take_toggle() -> bool {
    TOGGLE.swap(false, Ordering::Relaxed)
}

/// Returns false to swallow the key (it was ours).
pub fn on_key(key: usize, key_down: bool, prev_down: bool) -> bool {
    if !key_down || prev_down {
        return true;
    }
    let (ctrl, shift, alt) = (down(0x11), down(0x10), down(0x12));
    if BINDING.load(Ordering::Relaxed) {
        if let Some(text) = hotkey::format_keypress(key as u32, ctrl, shift, alt) {
            if let Ok(mut b) = BOUND.try_lock() {
                *b = Some(text);
            }
            BINDING.store(false, Ordering::Relaxed);
            return false;
        }
        return true;
    }
    let hit = HOTKEY.try_lock().ok().and_then(|g| *g).is_some_and(|hk| hotkey::matches(&hk, key as u32, ctrl, shift, alt));
    if hit {
        TOGGLE.store(true, Ordering::Relaxed);
        return false;
    }
    true
}
