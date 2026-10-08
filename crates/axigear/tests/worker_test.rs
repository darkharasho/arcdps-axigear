use std::sync::Arc;
use std::time::{Duration, Instant};

use arcdps_axigear::worker::Worker;
use axigear_core::driver::{Command, UiSnapshot};
use axigear_core::http::{Http, HttpResponse};
use axigear_core::mumble::MumbleSample;
use axigear_core::report::Badge;

struct Offline;

impl Http for Offline {
    fn get(&self, _: &str, _: &[(&str, &str)]) -> Result<HttpResponse, String> {
        Err("offline".into())
    }
}

fn wait_for(w: &Worker, pred: impl Fn(&UiSnapshot) -> bool) -> Arc<UiSnapshot> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(s) = w.try_snapshot() {
            if pred(&s) {
                return s;
            }
        }
        assert!(Instant::now() < deadline, "worker never produced the expected snapshot");
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn worker_loads_a_code_matches_the_slot_and_saves_on_shutdown() {
    let dir = tempfile::tempdir().unwrap();
    let code = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/comp-tuesday.txt")).unwrap();
    let mumble = || {
        Some(MumbleSample {
            ui_tick: 1,
            identity: r#"{"name":"Tester","profession":1,"spec":62,"map_id":1}"#.into(),
            context: vec![],
        })
    };
    let w = Worker::spawn(Arc::new(Offline), dir.path().to_path_buf(), mumble).unwrap();
    assert_eq!(wait_for(&w, |_| true).badge, Badge::NoComp);

    w.send(Command::LoadInput(code.trim().into()));
    let snap = wait_for(&w, |s| s.header.slot_label.is_some());
    assert_eq!(snap.header.slot_label.as_deref(), Some("Party 1 · Firebrand"));
    assert!(!w.failed());

    w.shutdown(Duration::from_secs(2));
    assert!(dir.path().join("config.json").exists());
}
