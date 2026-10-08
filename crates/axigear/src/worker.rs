//! The one thread that does I/O. arcdps callbacks only push `Command`s and
//! `try_snapshot()`; neither ever waits on this thread.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use axigear_core::driver::{Command, Driver, UiSnapshot};
use axigear_core::http::Http;
use axigear_core::mumble::MumbleSample;

const TICK: Duration = Duration::from_millis(250);
const MUMBLE_EVERY: Duration = Duration::from_secs(1);

type Shared = Arc<Mutex<Option<Arc<UiSnapshot>>>>;

pub struct Worker {
    tx: Sender<Command>,
    shared: Shared,
    failed: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl Worker {
    pub fn spawn<M>(http: Arc<dyn Http>, dir: PathBuf, mut mumble: M) -> std::io::Result<Worker>
    where
        M: FnMut() -> Option<MumbleSample> + Send + 'static,
    {
        let (tx, rx) = mpsc::channel();
        let shared: Shared = Arc::new(Mutex::new(None));
        let failed = Arc::new(AtomicBool::new(false));
        let (out, flag) = (shared.clone(), failed.clone());
        let handle = std::thread::Builder::new().name("axigear-worker".into()).spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run(http, dir, rx, out, &mut mumble)));
            if result.is_err() {
                log::error!("axigear: worker thread panicked; checks stopped");
                flag.store(true, Ordering::Relaxed);
            }
        })?;
        Ok(Worker { tx, shared, failed, handle: Some(handle) })
    }

    pub fn sender(&self) -> Sender<Command> {
        self.tx.clone()
    }

    pub fn send(&self, cmd: Command) {
        let _ = self.tx.send(cmd);
    }

    /// Latest snapshot; `None` if none yet or the worker is publishing right now.
    pub fn try_snapshot(&self) -> Option<Arc<UiSnapshot>> {
        self.shared.try_lock().ok().and_then(|g| g.clone())
    }

    pub fn failed(&self) -> bool {
        self.failed.load(Ordering::Relaxed)
    }

    /// Ask the worker to save and stop; wait up to `wait` (an HTTP call in
    /// flight can take up to the ureq timeout).
    /// Returns true if the thread joined, false if it timed out (still running).
    pub fn shutdown(mut self, wait: Duration) -> bool {
        let _ = self.tx.send(Command::Shutdown);
        if let Some(h) = self.handle.take() {
            let deadline = Instant::now() + wait;
            while !h.is_finished() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(20));
            }
            if h.is_finished() {
                let _ = h.join();
                return true;
            }
            return false;
        }
        true
    }
}

fn run(http: Arc<dyn Http>, dir: PathBuf, rx: Receiver<Command>, out: Shared, mumble: &mut dyn FnMut() -> Option<MumbleSample>) {
    let mut driver = Driver::new(http, &dir, Instant::now());
    let mut last_mumble: Option<Instant> = None;
    loop {
        let first = match rx.recv_timeout(TICK) {
            Ok(cmd) => Some(cmd),
            Err(RecvTimeoutError::Timeout) => None,
            Err(RecvTimeoutError::Disconnected) => return,
        };
        let now = Instant::now();
        for cmd in first.into_iter().chain(rx.try_iter()) {
            if !driver.handle(cmd, now) {
                return;
            }
        }
        if last_mumble.map_or(true, |t| now.duration_since(t) >= MUMBLE_EVERY) {
            last_mumble = Some(now);
            if let Some(sample) = mumble() {
                if !driver.handle(Command::Mumble(sample), now) {
                    return;
                }
            }
        }
        driver.tick(now);
        let snap = Arc::new(driver.snapshot(now));
        match out.lock() {
            Ok(mut g) => *g = Some(snap),
            Err(poisoned) => *poisoned.into_inner() = Some(snap),
        }
    }
}
