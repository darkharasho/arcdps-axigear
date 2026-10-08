#![cfg(windows)]
//! arcdps callbacks. Every body runs inside `guard` (catch_unwind); locks are
//! `try_lock`; all real work happens on the worker thread.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use arcdps::imgui::{Ui, WindowFlags};
use arcdps::{Agent, Event};
use axigear_core::driver::{Command, UiSnapshot};
use axigear_core::report::Badge;

use crate::http::UreqHttp;
use crate::mumble::Reader;
use crate::worker::Worker;

static WORKER: Mutex<Option<Worker>> = Mutex::new(None);
static SENDER: Mutex<Option<Sender<Command>>> = Mutex::new(None);
static LAST: Mutex<Option<Arc<UiSnapshot>>> = Mutex::new(None);
static DISABLED: AtomicBool = AtomicBool::new(false);
pub static DEBUG_SIGNALS: AtomicBool = AtomicBool::new(false);

/// Queue a command for the worker. Never blocks; a contended send is dropped.
pub fn send(cmd: Command) {
    if let Ok(g) = SENDER.try_lock() {
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
    *SENDER.lock().unwrap_or_else(|p| p.into_inner()) = Some(worker.sender());
    *WORKER.lock().unwrap_or_else(|p| p.into_inner()) = Some(worker);
    Ok(())
}

pub fn release() {
    let _ = std::panic::catch_unwind(|| {
        SENDER.lock().unwrap_or_else(|p| p.into_inner()).take();
        if let Some(w) = WORKER.lock().unwrap_or_else(|p| p.into_inner()).take() {
            w.shutdown(Duration::from_secs(2));
        }
    });
}

pub fn combat(ev: Option<&Event>, src: Option<&Agent>, dst: Option<&Agent>, skill_name: Option<&str>, _id: u64, _revision: u64) {
    guard("combat", (), || {
        if let Some(live) = crate::signals::translate(ev, src, dst) {
            if DEBUG_SIGNALS.load(Ordering::Relaxed) {
                log::warn!("axigear: {live:?} ({})", skill_name.unwrap_or("?"));
            }
            send(Command::Live(live));
        }
    })
}

pub fn imgui(ui: &Ui, not_loading: bool) {
    if !not_loading {
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
