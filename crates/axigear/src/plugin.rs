#![cfg(windows)]
//! arcdps callbacks. Every body runs inside `guard` (catch_unwind); locks are
//! `try_lock`; all real work happens on the worker thread.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use arcdps::imgui::{Ui, WindowFlags};
use arcdps::{Agent, Event};
use axigear_core::driver::{Command, UiSnapshot};
use axigear_core::report::Badge;

use crate::http::UreqHttp;
use crate::mumble::Reader;
use crate::worker::Worker;

static WORKER: Mutex<Option<Worker>> = Mutex::new(None);
static SENDER: RwLock<Option<Sender<Command>>> = RwLock::new(None);
static LAST: Mutex<Option<Arc<UiSnapshot>>> = Mutex::new(None);
static DISABLED: AtomicBool = AtomicBool::new(false);
pub static DEBUG_SIGNALS: AtomicBool = AtomicBool::new(false);

/// Queue a command for the worker. Never blocks. Readers share the lock, so
/// overlapping callbacks don't drop events; only init/release write.
pub fn send(cmd: Command) {
    if let Ok(g) = SENDER.try_read() {
        if let Some(tx) = g.as_ref() {
            let _ = tx.send(cmd);
        }
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
    let mut reader = Reader::new();
    let worker = Worker::spawn(Arc::new(UreqHttp::new()), dir, move || reader.sample())
        .map_err(|e| Some(format!("axigear: couldn't start worker thread: {e}")))?;
    *SENDER.write().unwrap_or_else(|p| p.into_inner()) = Some(worker.sender());
    *WORKER.lock().unwrap_or_else(|p| p.into_inner()) = Some(worker);
    Ok(())
}

pub fn release() {
    let _ = std::panic::catch_unwind(|| {
        SENDER.write().unwrap_or_else(|p| p.into_inner()).take();
        if let Some(w) = WORKER.lock().unwrap_or_else(|p| p.into_inner()).take() {
            if !w.shutdown(Duration::from_secs(2)) {
                pin_module();
                log::warn!("axigear: worker did not stop in time; module pinned so its code stays mapped");
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
    unsafe {
        let _ = GetModuleHandleExW(GET_MODULE_HANDLE_EX_FLAG_PIN | GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS, PCWSTR(anchor), &mut hmod);
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
    if disabled() {
        // Minimal, still panic-safe draw so the user sees the plugin died.
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            ui.window("##axigear-badge")
                .flags(WindowFlags::NO_TITLE_BAR | WindowFlags::ALWAYS_AUTO_RESIZE | WindowFlags::NO_FOCUS_ON_APPEARING)
                .build(|| ui.text(Badge::Error.text()));
        }));
        return;
    }
    guard("imgui", (), || {
        let Some(snap) = latest() else { return };
        DEBUG_SIGNALS.store(snap.settings.debug_logging, Ordering::Relaxed);
        // Plain-text badge; Task 21 replaces this with the axi-design badge.
        let text = if disabled() { Badge::Error.text() } else { snap.badge.text() };
        ui.window("##axigear-badge")
            .flags(WindowFlags::NO_TITLE_BAR | WindowFlags::ALWAYS_AUTO_RESIZE | WindowFlags::NO_FOCUS_ON_APPEARING)
            .build(|| ui.text(text));
    })
}
