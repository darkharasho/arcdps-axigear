//! Auto-updater: checks GitHub for a newer release and stages the
//! DLL swap on user confirmation. See
//! docs/superpowers/specs/2026-05-23-auto-updater-design.md.

#[derive(Debug, PartialEq)]
pub enum ParseOutcome {
    Newer { tag: String, asset_url: String },
    Current,
    ParseError(String),
}

const LATEST_URL: &str = "https://github.com/darkharasho/arcdps-axigear/releases/latest";
const DOWNLOAD_URL: &str = "https://github.com/darkharasho/arcdps-axigear/releases/download";
const DLL_NAME: &str = "arcdps_axigear.dll";

/// Parse where `/releases/latest` redirects to (`.../releases/tag/vX.Y.Z`)
/// and decide whether that release is newer than `current` (e.g. "0.1.1").
/// Pure; no IO.
pub fn parse_latest(location: &str, current: &str) -> ParseOutcome {
    let tag = match location.split_once("/releases/tag/") {
        Some((_, t)) if !t.trim_end_matches('/').is_empty() => t.trim_end_matches('/').to_string(),
        _ => return ParseOutcome::ParseError(format!("no release tag in {location}")),
    };
    let asset_url = format!("{DOWNLOAD_URL}/{tag}/{DLL_NAME}");

    let strip = |s: &str| s.strip_prefix('v').unwrap_or(s).to_string();
    let remote = match semver::Version::parse(&strip(&tag)) {
        Ok(v) => v,
        Err(e) => return ParseOutcome::ParseError(format!("tag semver: {e}")),
    };
    let local = match semver::Version::parse(&strip(current)) {
        Ok(v) => v,
        Err(e) => return ParseOutcome::ParseError(format!("current semver: {e}")),
    };
    if remote > local {
        ParseOutcome::Newer { tag, asset_url }
    } else {
        ParseOutcome::Current
    }
}

use std::sync::Mutex;

#[derive(Debug, Clone)]
pub enum UpdateState {
    Idle,
    Checking,
    UpToDate,
    Available    { tag: String, asset_url: String },
    Downloading  { tag: String, pct: f32 },
    Installed    { tag: String },
    Failed       { msg: String },
}

static STATE: Mutex<UpdateState> = Mutex::new(UpdateState::Idle);

pub fn snapshot() -> UpdateState {
    STATE.lock().map(|g| g.clone()).unwrap_or(UpdateState::Idle)
}

pub fn dismiss_error() {
    if let Ok(mut g) = STATE.lock() {
        if matches!(*g, UpdateState::Failed { .. }) { *g = UpdateState::Idle; }
    }
}

fn set_state(new: UpdateState) {
    if let Ok(mut g) = STATE.lock() { *g = new; }
}

/// UI helper for synchronous failure reporting (e.g. resolver
/// returned None before any thread was spawned).
pub fn set_failed(msg: &str) {
    set_state(UpdateState::Failed { msg: msg.to_string() });
}

use std::thread;
use std::time::Duration;

/// Called once on plugin init. If `enabled`, spawns a short-lived
/// background thread that asks GitHub for the latest release
/// and updates `STATE` accordingly. Cheap to call when disabled.
pub fn kick_check_on_load(enabled: bool) {
    if !enabled {
        set_state(UpdateState::Idle);
        return;
    }
    set_state(UpdateState::Checking);
    let current = env!("CARGO_PKG_VERSION").to_string();
    thread::Builder::new()
        .name("axigear-update-check".into())
        .spawn(move || {
            match http_fetch_latest() {
                Ok(location) => match parse_latest(&location, &current) {
                    ParseOutcome::Newer { tag, asset_url } =>
                        set_state(UpdateState::Available { tag, asset_url }),
                    ParseOutcome::Current =>
                        set_state(UpdateState::UpToDate),
                    ParseOutcome::ParseError(msg) =>
                        set_state(UpdateState::Failed { msg: format!("parse: {msg}") }),
                },
                Err(msg) => set_state(UpdateState::Failed { msg }),
            }
        })
        .ok();
}

/// Returns where `/releases/latest` redirects to. This is the website, not
/// api.github.com: the API allows only 60 unauthenticated calls an hour per
/// IP, shared by every plugin in every game client on the connection.
fn http_fetch_latest() -> Result<String, String> {
    let ua = format!("arcdps_axigear/{}", env!("CARGO_PKG_VERSION"));
    let agent = ureq::AgentBuilder::new()
        .redirects(0)
        .timeout(Duration::from_secs(15))
        .build();
    let resp = agent.get(LATEST_URL)
        .set("User-Agent", &ua)
        .call()
        .map_err(|e| http_error("http", e))?;
    match resp.header("Location") {
        Some(loc) => Ok(loc.to_string()),
        None => Err(format!("http: no redirect (status {})", resp.status())),
    }
}

/// GitHub answers 403 or 429 when it rate-limits a connection.
fn http_error(what: &str, e: ureq::Error) -> String {
    match e {
        ureq::Error::Status(403 | 429, _) =>
            "GitHub is rate-limiting this connection, try again later".to_string(),
        e => format!("{what}: {e}"),
    }
}

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

/// Called from the UI when the user clicks Install. No-op unless
/// `STATE` is currently `Available`. The check-and-transition is
/// atomic under `STATE`'s lock so a double-click can't spawn two
/// download threads.
pub fn start_install(dll_dir: PathBuf) {
    let (tag, asset_url) = {
        let Ok(mut g) = STATE.lock() else { return; };
        match &*g {
            UpdateState::Available { tag, asset_url, .. } => {
                let captured = (tag.clone(), asset_url.clone());
                *g = UpdateState::Downloading { tag: captured.0.clone(), pct: 0.0 };
                captured
            }
            _ => return,
        }
    };
    thread::Builder::new()
        .name("axigear-update-download".into())
        .spawn(move || {
            match download_and_swap(&dll_dir, &asset_url, &tag) {
                Ok(()) => set_state(UpdateState::Installed { tag }),
                Err(msg) => set_state(UpdateState::Failed { msg }),
            }
        })
        .ok();
}

fn download_and_swap(dll_dir: &Path, asset_url: &str, tag: &str) -> Result<(), String> {
    let dll      = dll_dir.join("arcdps_axigear.dll");
    let dll_new  = dll_dir.join("arcdps_axigear.dll.new");
    let dll_old  = dll_dir.join("arcdps_axigear.dll.old");

    // Stream into `.new`. ureq returns an io::Read.
    let ua = format!("arcdps_axigear/{}", env!("CARGO_PKG_VERSION"));
    let resp = ureq::get(asset_url)
        .set("User-Agent", &ua)
        .timeout(Duration::from_secs(120))
        .call()
        .map_err(|e| http_error("download", e))?;
    let total: Option<u64> = resp.header("Content-Length")
        .and_then(|s| s.parse().ok());
    let mut reader = resp.into_reader();
    let mut file = std::fs::File::create(&dll_new)
        .map_err(|e| format!("create .new: {e}"))?;
    let mut buf = [0u8; 64 * 1024];
    let mut read_total: u64 = 0;
    loop {
        let n = reader.read(&mut buf).map_err(|e| format!("read: {e}"))?;
        if n == 0 { break; }
        file.write_all(&buf[..n]).map_err(|e| format!("write: {e}"))?;
        read_total += n as u64;
        if let Some(t) = total {
            let pct = (read_total as f32 / t as f32) * 100.0;
            set_state(UpdateState::Downloading { tag: tag.to_string(), pct });
        }
    }
    file.sync_all().map_err(|e| format!("fsync: {e}"))?;
    drop(file);

    // Reject an empty or non-PE download before it can replace the DLL.
    let mut head = [0u8; 2];
    let got = std::fs::File::open(&dll_new).and_then(|mut f| f.read(&mut head)).unwrap_or(0);
    if !looks_like_dll(&head[..got]) {
        let _ = std::fs::remove_file(&dll_new);
        return Err("downloaded file is not a valid DLL".to_string());
    }

    // Best-effort cleanup of any leftover `.old` from a prior session;
    // ignore failure (Windows may still hold a handle).
    let _ = std::fs::remove_file(&dll_old);

    // Atomic shuffle. Rename of a loaded DLL is permitted on both
    // Windows and Linux/Wine.
    std::fs::rename(&dll, &dll_old)
        .map_err(|e| format!("rename dll -> .old: {e}"))?;
    std::fs::rename(&dll_new, &dll)
        .map_err(|e| {
            // Best-effort rollback if the second rename fails.
            let _ = std::fs::rename(&dll_old, &dll);
            format!("rename .new -> dll: {e}")
        })?;
    Ok(())
}

/// A Windows DLL starts with the "MZ" DOS header.
fn looks_like_dll(head: &[u8]) -> bool {
    head.starts_with(b"MZ")
}

/// Called from plugin init. Attempts to delete any leftover `.old`
/// from a previous update. Failure is silent — we'll retry next session.
pub fn cleanup_stale_old(dll_dir: &Path) {
    let _ = std::fs::remove_file(dll_dir.join("arcdps_axigear.dll.old"));
    let _ = std::fs::remove_file(dll_dir.join("arcdps_axigear.dll.new"));
}

#[cfg(test)]
mod tests {
    use super::*;


    #[test]
    fn dll_validation_requires_mz() {
        assert!(looks_like_dll(b"MZ\x90\x00"));
        assert!(!looks_like_dll(b""));
        assert!(!looks_like_dll(b"M"));
        assert!(!looks_like_dll(b"<html>"));
    }

    fn location(tag: &str) -> String {
        format!("https://github.com/darkharasho/arcdps-axigear/releases/tag/{tag}")
    }

    #[test]
    fn newer_release_is_detected() {
        match parse_latest(&location("v0.1.2"), "0.1.1") {
            ParseOutcome::Newer { tag, asset_url } => {
                assert_eq!(tag, "v0.1.2");
                assert_eq!(
                    asset_url,
                    "https://github.com/darkharasho/arcdps-axigear/releases/download/v0.1.2/arcdps_axigear.dll"
                );
            }
            other => panic!("expected Newer, got {other:?}"),
        }
    }

    #[test]
    fn same_version_is_current() {
        assert_eq!(parse_latest(&location("v0.1.1"), "0.1.1"), ParseOutcome::Current);
    }

    #[test]
    fn older_release_is_current() {
        assert_eq!(parse_latest(&location("v0.1.0"), "0.1.1"), ParseOutcome::Current);
    }

    #[test]
    fn redirect_without_a_tag_is_parse_error() {
        // With no releases, /releases/latest redirects to /releases.
        let loc = "https://github.com/darkharasho/arcdps-axigear/releases";
        assert!(matches!(parse_latest(loc, "0.1.1"), ParseOutcome::ParseError(_)));
        assert!(matches!(parse_latest(&location(""), "0.1.1"), ParseOutcome::ParseError(_)));
    }

    #[test]
    fn non_semver_tag_is_parse_error() {
        assert!(matches!(parse_latest(&location("nightly"), "0.1.1"), ParseOutcome::ParseError(_)));
    }

    #[test]
    fn tag_without_v_prefix_still_parses() {
        match parse_latest(&location("0.1.2"), "0.1.1") {
            ParseOutcome::Newer { tag, .. } => assert_eq!(tag, "0.1.2"),
            other => panic!("expected Newer, got {other:?}"),
        }
    }
}
