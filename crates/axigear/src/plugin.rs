#![cfg(windows)]
//! arcdps callbacks. Every body runs inside `guard` (catch_unwind); the
//! per-frame callbacks take locks with `try_lock` only (SENDER is an RwLock
//! read with `try_read`); all real work happens on the worker thread.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use arcdps::imgui::Ui;
use arcdps::{Agent, Event};
use axigear_core::driver::{Command, UiSnapshot};

use crate::http::UreqHttp;
use crate::mumble::Reader;
use crate::ui::state::UiState;
use crate::worker::Worker;

static WORKER: Mutex<Option<Worker>> = Mutex::new(None);
static SENDER: RwLock<Option<Sender<Command>>> = RwLock::new(None);
static LAST: Mutex<Option<Arc<UiSnapshot>>> = Mutex::new(None);
pub(crate) static UI_STATE: Mutex<Option<UiState>> = Mutex::new(None);
static DISABLED: AtomicBool = AtomicBool::new(false);
pub static DEBUG_SIGNALS: AtomicBool = AtomicBool::new(false);

/// Queue a command for the worker. Never blocks. Readers share the lock, so
/// overlapping callbacks don't drop events; only init/release write.
pub fn send(cmd: Command) {
    use std::sync::TryLockError;
    let g = match SENDER.try_read() {
        Ok(g) => g,
        Err(TryLockError::Poisoned(p)) => p.into_inner(),
        Err(TryLockError::WouldBlock) => return,
    };
    if let Some(tx) = g.as_ref() {
        let _ = tx.send(cmd);
    }
}

fn panic_message(p: &Box<dyn std::any::Any + Send>) -> String {
    p.downcast_ref::<&str>()
        .map(|s| s.to_string())
        .or_else(|| p.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "unknown panic".into())
}

/// Run a callback body; on panic, log it and disable the plugin for the session.
pub(crate) fn guard<R>(name: &str, fallback: R, f: impl FnOnce() -> R) -> R {
    if DISABLED.load(Ordering::Relaxed) {
        return fallback;
    }
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)) {
        Ok(r) => r,
        Err(p) => {
            DISABLED.store(true, Ordering::Relaxed);
            log::error!("axigear: {name} panicked, disabling: {}", panic_message(&p));
            fallback
        }
    }
}

pub(crate) fn disabled() -> bool {
    DISABLED.load(Ordering::Relaxed)
}

/// Freshest snapshot, or the last one if the worker is mid-publish.
pub(crate) fn latest() -> Option<Arc<UiSnapshot>> {
    let fresh = WORKER.try_lock().ok().and_then(|w| {
        let w = w.as_ref()?;
        if w.failed() {
            DISABLED.store(true, Ordering::Relaxed);
        }
        w.try_snapshot()
    });
    let mut last = LAST.try_lock().ok()?;
    if fresh.is_some() {
        *last = fresh;
    }
    last.clone()
}

pub fn init() -> Result<(), Option<String>> {
    let dir = crate::paths::data_dir();
    if let Some(dll) = crate::paths::dll_dir() {
        crate::updater::cleanup_stale_old(&dll);
    }
    crate::ui::textures::allow_worker();
    let mut reader = Reader::new();
    let worker = Worker::spawn(Arc::new(UreqHttp::new()), dir, move || reader.sample())
        .map_err(|e| Some(format!("axigear: couldn't start worker thread: {e}")))?;
    *SENDER.write().unwrap_or_else(|p| p.into_inner()) = Some(worker.sender());
    *WORKER.lock().unwrap_or_else(|p| p.into_inner()) = Some(worker);
    *UI_STATE.lock().unwrap_or_else(|p| p.into_inner()) = Some(UiState::default());
    Ok(())
}

pub fn release() {
    let _ = std::panic::catch_unwind(|| {
        // Signal the icon worker first so it winds down while the main
        // worker saves; both then share one 2 s budget.
        let icons = crate::ui::textures::stop_worker();
        let deadline = Instant::now() + Duration::from_secs(2);
        SENDER.write().unwrap_or_else(|p| p.into_inner()).take();
        let mut pinned = false;
        if let Some(w) = WORKER.lock().unwrap_or_else(|p| p.into_inner()).take() {
            if !w.shutdown(Duration::from_secs(2)) {
                pin_module();
                pinned = true;
                log::warn!("axigear: worker did not stop in time; module pinned so its code stays mapped");
            }
        }
        // A ureq icon download in flight can run up to 20 s: don't wait it
        // out, pin instead (same policy as the main worker).
        if let Some(h) = icons {
            if !crate::ui::textures::join_by(h, deadline) && !pinned {
                pin_module();
                log::warn!("axigear: icon worker did not stop in time; module pinned so its code stays mapped");
            }
        }
    });
}

/// Keep this DLL mapped for the process lifetime so a still-running worker
/// thread never executes unmapped code.
fn pin_module() {
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::HMODULE;
    use windows::Win32::System::LibraryLoader::{
        GetModuleHandleExW, GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS, GET_MODULE_HANDLE_EX_FLAG_PIN,
    };
    let mut hmod = HMODULE::default();
    let anchor = pin_module as *const () as *const u16;
    let res = unsafe {
        GetModuleHandleExW(GET_MODULE_HANDLE_EX_FLAG_PIN | GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS, PCWSTR(anchor), &mut hmod)
    };
    match res {
        Ok(()) => log::info!("axigear: module pinned"),
        Err(e) => log::error!("axigear: failed to pin module: {e}"),
    }
}

pub fn combat(ev: Option<&Event>, src: Option<&Agent>, dst: Option<&Agent>, skill_name: Option<&str>, _id: u64, _revision: u64) {
    guard("combat", (), || {
        if let Some(live) = crate::signals::translate(ev, src, dst) {
            if DEBUG_SIGNALS.load(Ordering::Relaxed) {
                log::debug!("axigear: {live:?} ({})", skill_name.unwrap_or("?"));
            }
            send(Command::Live(live));
        }
    })
}

pub fn imgui(ui: &Ui, not_loading: bool) {
    if !not_loading {
        return;
    }
    crate::keys::tick_frame();
    if disabled() {
        // No locks, no snapshot: still panic-safe so the user sees the plugin died.
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| crate::ui::badge::render_error(ui)));
        return;
    }
    guard("imgui", (), || {
        crate::ui::textures::drain_pending();
        let Some(snap) = latest() else { return };
        let Ok(mut guard) = UI_STATE.try_lock() else { return };
        let Some(state) = guard.as_mut() else { return };
        DEBUG_SIGNALS.store(snap.settings.debug_logging, Ordering::Relaxed);
        state.sync(&snap);
        crate::ui::badge::render(ui, &snap, state);
        crate::ui::checklist::render(ui, &snap, state);
    })
}

pub fn options_end(ui: &Ui) {
    guard("options_end", (), || {
        crate::keys::mark_options_frame();
        let Some(snap) = latest() else { return };
        let Ok(mut guard) = UI_STATE.try_lock() else { return };
        if let Some(state) = guard.as_mut() {
            crate::ui::settings::render(ui, &snap, state);
        }
    })
}

/// Adds "axigear" to arcdps's window list (toggles the checklist).
pub fn options_windows(ui: &Ui, window_name: Option<&str>) -> bool {
    if window_name.is_none() {
        guard("options_windows", (), || {
            if let Ok(mut guard) = UI_STATE.try_lock() {
                if let Some(state) = guard.as_mut() {
                    ui.checkbox("axigear", &mut state.checklist_open);
                }
            }
        });
    }
    false
}

pub fn wnd_nofilter(key: usize, key_down: bool, prev_key_down: bool) -> bool {
    guard("wnd_nofilter", true, || crate::keys::on_key(key, key_down, prev_key_down))
}
