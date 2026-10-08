//! The worker's brain: owns settings, session and polling; turns commands and
//! the passage of time into fetches, checks and a `UiSnapshot` for the UI.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::checks::ApiState;
use crate::consumables::Consumables;
use crate::gamedb::GameDb;
use crate::gw2api::{self, ApiError};
use crate::http::Http;
use crate::link::AxiLink;
use crate::live::{IdentityChange, LiveEvent};
use crate::loader::{self, Fetched, Input, LoadError};
use crate::matcher;
use crate::model::{GameMode, SlotRef};
use crate::mumble::{self, MumbleSample};
use crate::report::{Badge, Category, CheckReport, SeveritySetting};
use crate::schedule::{RateLimit, Poller, API_BACKOFF, API_INTERVAL, COMP_BACKOFF, COMP_INTERVAL, MANUAL_REFRESH};
use crate::session::{slot_label, Assignment, CompOrigin, LoadedComp, Session};
use crate::settings::{BadgeSettings, Settings};
use crate::specs::SpecDb;
use crate::text;

const FLASH: Duration = Duration::from_secs(3);

#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    LoadInput(String),
    Unsubscribe,
    Pick(SlotRef),
    RefreshComp,
    RefreshApi,
    SetApiKey(String),
    TestKey,
    Live(LiveEvent),
    Mumble(MumbleSample),
    Settings(SettingsPatch),
    Shutdown,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SettingsPatch {
    Severity(Category, SeveritySetting),
    Badge(BadgeSettings),
    Hotkey(String),
    AutoUpdate(bool),
    DebugLogging(bool),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PickOption {
    pub slot: SlotRef,
    pub label: String,
    /// Fits the current spec (others are shown disabled).
    pub enabled: bool,
    pub current: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Header {
    pub comp_name: Option<String>,
    /// "code" or "link · fetched 2m ago".
    pub source: String,
    pub offline: bool,
    pub slot_label: Option<String>,
    /// Why there is no report, or a comp error worth showing.
    pub note: Option<String>,
    pub api_line: String,
}

#[derive(Debug, Clone)]
pub struct UiSnapshot {
    pub badge: Badge,
    pub report: Option<CheckReport>,
    pub header: Header,
    pub picker: Vec<PickOption>,
    pub load_error: Option<String>,
    pub key_test: Option<String>,
    pub in_combat: bool,
    pub map_mode: Option<GameMode>,
    pub comp_mode: Option<GameMode>,
    pub settings: Settings,
    /// Badge border flash after a pass→fail transition.
    pub flash: bool,
    /// A persistence write failed (settings, comp cache or item db).
    pub save_error: Option<String>,
}

pub struct Paths {
    pub config: PathBuf,
    pub itemdb: PathBuf,
    pub comp_cache: PathBuf,
}

impl Paths {
    pub fn in_dir(dir: &Path) -> Paths {
        Paths { config: dir.join("config.json"), itemdb: dir.join("itemdb.json"), comp_cache: dir.join("comp_cache.json") }
    }
}

fn unix_now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

pub struct Driver {
    http: Arc<dyn Http>,
    paths: Paths,
    settings: Settings,
    session: Session,
    /// The link being polled (resolved after the first fetch).
    subscription: Option<AxiLink>,
    comp_poll: Poller,
    api_poll: Poller,
    db_poll: Poller,
    comp_manual: RateLimit,
    api_manual: RateLimit,
    comp_error: Option<LoadError>,
    load_error: Option<String>,
    key_test: Option<String>,
    db_dirty: bool,
    was_failing: bool,
    flash_until: Option<Instant>,
    save_error: Option<String>,
}

impl Driver {
    pub fn new(http: Arc<dyn Http>, dir: &Path, _now: Instant) -> Driver {
        let paths = Paths::in_dir(dir);
        let settings = Settings::load(&paths.config);
        let mut session = Session::new(GameDb::load(&paths.itemdb));
        session.api.has_key = !settings.api_key.trim().is_empty();

        let cached: Option<LoadedComp> = std::fs::read_to_string(&paths.comp_cache)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .filter(|c: &LoadedComp| !settings.comp_input.is_empty() && c.input == settings.comp_input);
        let mut subscription = None;
        match (&cached, loader::detect(&settings.comp_input)) {
            (Some(LoadedComp { origin: CompOrigin::Link { link, .. }, .. }), _) => subscription = Some(link.clone()),
            (Some(_), _) => {}
            (None, Ok(Input::Comp { comp, key })) => {
                session.comp = Some(LoadedComp { comp, key, input: settings.comp_input.clone(), origin: CompOrigin::Code })
            }
            (None, Ok(Input::Link(link))) => subscription = Some(link),
            (None, Err(_)) => {}
        }
        if cached.is_some() {
            session.comp = cached;
        }

        Driver {
            http,
            paths,
            settings,
            session,
            subscription,
            comp_poll: Poller::new(COMP_INTERVAL, &COMP_BACKOFF),
            api_poll: Poller::new(API_INTERVAL, &API_BACKOFF),
            db_poll: Poller::new(API_INTERVAL, &API_BACKOFF),
            comp_manual: RateLimit::new(MANUAL_REFRESH),
            api_manual: RateLimit::new(MANUAL_REFRESH),
            comp_error: None,
            load_error: None,
            key_test: None,
            db_dirty: true,
            was_failing: false,
            flash_until: None,
            save_error: None,
        }
    }

    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    pub fn session(&self) -> &Session {
        &self.session
    }

    pub fn handle(&mut self, cmd: Command, now: Instant) -> bool {
        let specs = SpecDb::bundled();
        match cmd {
            Command::LoadInput(text) => self.load_input(text, now),
            Command::Unsubscribe => {
                self.subscription = None;
                self.comp_error = None;
                self.load_error = None;
                self.settings.comp_input.clear();
                self.session.set_comp(None, &self.settings.picks, specs, now);
                let _ = std::fs::remove_file(&self.paths.comp_cache);
                self.save_settings();
            }
            Command::Pick(slot) => {
                if self.session.pick(slot, &mut self.settings.picks, specs, now) {
                    self.save_settings();
                }
            }
            Command::RefreshComp => {
                if self.subscription.is_some() && self.comp_manual.try_acquire(now) {
                    self.comp_poll.reset();
                    self.poll_comp(now);
                }
            }
            Command::RefreshApi => {
                if self.api_manual.try_acquire(now) {
                    self.api_poll.reset();
                    self.poll_api(now);
                }
            }
            Command::SetApiKey(key) => {
                self.settings.api_key = key.trim().to_string();
                self.session.api = ApiState { has_key: !self.settings.api_key.is_empty(), ..Default::default() };
                self.api_poll.reset();
                self.key_test = None;
                self.save_settings();
            }
            Command::TestKey if self.settings.api_key.trim().is_empty() => {
                self.key_test = Some("no API key set".into());
            }
            Command::TestKey => {
                self.key_test = Some(match gw2api::tokeninfo(&*self.http, &self.settings.api_key) {
                    Ok(info) => match gw2api::missing_permissions(&info) {
                        missing if missing.is_empty() => format!("key ok ({})", info.name),
                        missing => format!("key is missing permissions: {}", missing.join(", ")),
                    },
                    Err(e) => e.to_string(),
                });
            }
            Command::Live(ev) => self.session.live.apply(&ev, Consumables::bundled()),
            Command::Mumble(sample) => {
                if let Some(id) = mumble::parse(&sample) {
                    match self.session.live.set_identity(id, now) {
                        IdentityChange::None => {}
                        change => {
                            if change == IdentityChange::Map {
                                self.comp_poll.note_map_change();
                            }
                            self.api_poll.note_map_change();
                            self.session.rematch(&self.settings.picks, specs, now);
                        }
                    }
                }
            }
            Command::Settings(patch) => {
                match patch {
                    SettingsPatch::Severity(c, s) => self.settings.severities.set(c, s),
                    SettingsPatch::Badge(b) => self.settings.badge = b,
                    SettingsPatch::Hotkey(h) => self.settings.hotkey = h,
                    SettingsPatch::AutoUpdate(v) => self.settings.auto_update_check = v,
                    SettingsPatch::DebugLogging(v) => self.settings.debug_logging = v,
                }
                self.save_settings();
            }
            Command::Shutdown => {
                self.save_settings();
                self.save_itemdb();
                return false;
            }
        }
        true
    }

    pub fn tick(&mut self, now: Instant) {
        if self.subscription.is_some() && self.comp_poll.due(now) {
            self.poll_comp(now);
        }
        let in_combat = self.session.live.in_combat();
        if !in_combat && self.api_poll.due(now) {
            self.poll_api(now);
        }
        if !in_combat && self.db_dirty && self.db_poll.due(now) {
            self.resolve_db(now);
        }
        let failing = self.report(now).is_some_and(|r| r.summary().fails() > 0);
        if failing && !self.was_failing {
            self.flash_until = Some(now + FLASH);
        }
        self.was_failing = failing;
    }

    pub fn snapshot(&self, now: Instant) -> UiSnapshot {
        let specs = SpecDb::bundled();
        let report = self.report(now);
        let lc = self.session.comp.as_ref();
        let identity = self.session.live.identity.as_ref();
        let header = Header {
            comp_name: lc.map(|c| c.comp.name.clone()),
            source: match lc.map(|c| &c.origin) {
                None => String::new(),
                Some(CompOrigin::Code) => "code".into(),
                Some(CompOrigin::Link { fetched_at_unix, .. }) => {
                    format!("link · fetched {}", text::ago(unix_now().saturating_sub(*fetched_at_unix)))
                }
            },
            offline: self.comp_error.as_ref().is_some_and(LoadError::retryable),
            slot_label: report.as_ref().map(|r| r.slot_label.clone()),
            note: self.note(),
            api_line: self.api_line(now),
        };
        let picker = lc
            .map(|c| {
                c.comp
                    .candidates()
                    .into_iter()
                    .map(|r| PickOption {
                        slot: r,
                        label: slot_label(&c.comp, r, specs),
                        enabled: identity.is_some_and(|id| matcher::fits(&c.comp.builds[r.build], id, specs)),
                        current: self.session.assignment.slot() == Some(r),
                    })
                    .collect()
            })
            .unwrap_or_default();
        UiSnapshot {
            badge: self.session.badge(report.as_ref()),
            report,
            header,
            picker,
            load_error: self.load_error.clone(),
            key_test: self.key_test.clone(),
            in_combat: self.session.live.in_combat(),
            map_mode: identity.map(|i| mumble::game_mode_for_map_type(i.map_type)),
            comp_mode: lc.and_then(|c| c.comp.game_mode),
            settings: self.settings.clone(),
            flash: self.flash_until.is_some_and(|t| now < t),
            save_error: self.save_error.clone(),
        }
    }

    fn report(&self, now: Instant) -> Option<CheckReport> {
        self.session.report(SpecDb::bundled(), Consumables::bundled(), &self.settings.severities, now)
    }

    fn record_save(&mut self, file: &str, result: std::io::Result<()>) {
        match result {
            Ok(()) => {
                if self.save_error.as_deref().is_some_and(|e| e.starts_with(&format!("couldn't save {file}:"))) {
                    self.save_error = None;
                }
            }
            Err(e) => self.save_error = Some(format!("couldn't save {file}: {e}")),
        }
    }

    fn save_settings(&mut self) {
        let result = self.settings.save(&self.paths.config);
        self.record_save("config.json", result);
    }

    fn save_itemdb(&mut self) {
        let result = self.session.db.save(&self.paths.itemdb);
        self.record_save("itemdb.json", result);
    }

    fn commit(&mut self, lc: LoadedComp, now: Instant) {
        self.load_error = None;
        self.settings.comp_input = lc.input.clone();
        let cached = serde_json::to_vec(&lc).map_err(std::io::Error::other).and_then(|json| crate::fsutil::write_atomic(&self.paths.comp_cache, &json));
        self.record_save("comp_cache.json", cached);
        self.session.set_comp(Some(lc), &self.settings.picks, SpecDb::bundled(), now);
        self.db_dirty = true;
        self.db_poll.reset();
        self.save_settings();
    }

    fn load_input(&mut self, text: String, now: Instant) {
        let text = text.trim().to_string();
        match loader::detect(&text) {
            Err(e) => self.load_error = Some(e.to_string()),
            Ok(Input::Comp { comp, key }) => {
                self.subscription = None;
                self.comp_error = None;
                self.commit(LoadedComp { comp, key, input: text, origin: CompOrigin::Code }, now);
            }
            Ok(Input::Link(link)) => match loader::fetch(&*self.http, &link, None) {
                Ok(Fetched::Fresh { comp, etag, link }) => {
                    self.subscription = Some(link.clone());
                    self.comp_error = None;
                    self.comp_poll.reset();
                    self.comp_poll.success(now);
                    let key = loader::link_key(&link);
                    self.commit(LoadedComp { comp, key, input: text, origin: CompOrigin::Link { link, etag, fetched_at_unix: unix_now() } }, now);
                }
                Ok(Fetched::NotModified) => self.load_error = Some("unexpected 304 on first fetch".into()),
                Err(e) => self.load_error = Some(e.to_string()),
            },
        }
    }

    fn poll_comp(&mut self, now: Instant) {
        let Some(link) = self.subscription.clone() else { return };
        let etag = match &self.session.comp {
            Some(LoadedComp { origin: CompOrigin::Link { etag, .. }, .. }) => etag.clone(),
            _ => None,
        };
        match loader::fetch(&*self.http, &link, etag.as_deref()) {
            Ok(Fetched::NotModified) => {
                self.comp_poll.success(now);
                self.comp_error = None;
                if let Some(LoadedComp { origin: CompOrigin::Link { fetched_at_unix, .. }, .. }) = &mut self.session.comp {
                    *fetched_at_unix = unix_now();
                }
            }
            Ok(Fetched::Fresh { comp, etag, link }) => {
                self.comp_poll.success(now);
                self.comp_error = None;
                self.subscription = Some(link.clone());
                let key = loader::link_key(&link);
                let input = self.settings.comp_input.clone();
                self.commit(LoadedComp { comp, key, input, origin: CompOrigin::Link { link, etag, fetched_at_unix: unix_now() } }, now);
            }
            Err(e) => {
                if e.retryable() {
                    self.comp_poll.failure(now);
                } else {
                    self.comp_poll.stop();
                }
                self.comp_error = Some(e);
            }
        }
    }

    fn poll_api(&mut self, now: Instant) {
        let Some(id) = self.session.live.identity.clone() else { return };
        if !self.session.api.has_key || self.session.build().is_none() || self.session.live.in_combat() {
            return;
        }
        match gw2api::fetch_snapshot(&*self.http, &self.settings.api_key, &id.name, now) {
            Ok(snap) => {
                self.session.api.snapshot = Some(snap);
                self.session.api.error = None;
                self.api_poll.success(now);
                self.db_dirty = true;
                self.db_poll.reset();
            }
            Err(e) => {
                if e == ApiError::CharacterNotFound {
                    self.session.api.snapshot = None;
                }
                self.session.api.error = Some(e.to_string());
                if e.backoff() {
                    self.api_poll.failure(now);
                } else {
                    self.api_poll.success(now);
                }
            }
        }
    }

    fn resolve_db(&mut self, now: Instant) {
        let wanted = self.session.db.wanted(self.session.build(), self.session.api.snapshot.as_ref(), &self.session.live.skills_cast);
        if wanted.is_empty() {
            self.db_dirty = false;
            return;
        }
        match self.session.db.resolve(&*self.http, &wanted) {
            Ok(()) => {
                self.db_dirty = false;
                self.db_poll.success(now);
                self.save_itemdb();
            }
            Err(_) => self.db_poll.failure(now),
        }
    }

    fn note(&self) -> Option<String> {
        if let Some(e) = self.comp_error.as_ref().filter(|e| !e.retryable()) {
            return Some(e.to_string());
        }
        if self.session.comp.is_none() {
            return self.subscription.as_ref().map(|_| "loading comp...".into());
        }
        match &self.session.assignment {
            Assignment::NoMatch => Some("no slot in this comp matches your spec".into()),
            Assignment::Ambiguous(_) => Some("several slots match - pick yours".into()),
            Assignment::Unassigned => Some("waiting for your character".into()),
            Assignment::Auto(_) | Assignment::Manual(_) => None,
        }
    }

    fn api_line(&self, now: Instant) -> String {
        let api = &self.session.api;
        if !api.has_key {
            return "API: needs key (characters, builds)".into();
        }
        match (&api.snapshot, &api.error) {
            (Some(s), err) => {
                let age = text::ago(now.saturating_duration_since(s.fetched_at).as_secs());
                match err {
                    Some(e) => format!("API: snapshot {age} · {e}"),
                    None => format!("API: snapshot {age}"),
                }
            }
            (None, Some(e)) => format!("API: {e}"),
            (None, None) => "API: waiting".into(),
        }
    }}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::fake::FakeHttp;
    use crate::testutil::fixture;

    const RAW: &str = "https://raw.githubusercontent.com/someone/axibuilds/main/site/comps/e4369a53.enc";
    const PAGES: &str = "https://someone.github.io/axibuilds/comps/e4369a53.enc";

    fn link() -> String {
        format!("https://someone.github.io/axibuilds/?c=e4369a53.{}", fixture("fixture.key"))
    }

    fn mumble(name: &str, profession: u8, spec: u16, map: u32) -> Command {
        Command::Mumble(MumbleSample {
            ui_tick: 1,
            identity: format!(r#"{{"name":"{name}","profession":{profession},"spec":{spec},"map_id":{map}}}"#),
            context: vec![],
        })
    }

    fn firebrand_in(map: u32) -> Command {
        mumble("Tester", 1, 62, map)
    }

    fn setup() -> (Arc<FakeHttp>, tempfile::TempDir, Instant) {
        (Arc::new(FakeHttp::new()), tempfile::tempdir().unwrap(), Instant::now())
    }

    fn driver(http: &Arc<FakeHttp>, dir: &tempfile::TempDir, t0: Instant) -> Driver {
        Driver::new(http.clone() as Arc<dyn Http>, dir.path(), t0)
    }

    fn calls_to(http: &FakeHttp, url: &str) -> usize {
        http.calls().iter().filter(|c| c.as_str() == url).count()
    }

    #[test]
    fn a_pasted_code_assigns_a_slot_and_survives_restart() {
        let (http, dir, t0) = setup();
        let mut d = driver(&http, &dir, t0);
        d.handle(Command::LoadInput(fixture("comp-tuesday.txt")), t0);
        d.handle(firebrand_in(1), t0);
        let snap = d.snapshot(t0);
        assert_eq!(snap.header.comp_name.as_deref(), Some("Tuesday Zerg"));
        assert_eq!(snap.header.source, "code");
        assert_eq!(snap.header.slot_label.as_deref(), Some("Party 1 · Firebrand"));
        assert!(snap.report.is_some());
        assert_eq!(d.settings().comp_input, fixture("comp-tuesday.txt"));

        let mut again = driver(&http, &dir, t0);
        again.handle(firebrand_in(1), t0);
        assert_eq!(again.snapshot(t0).header.slot_label.as_deref(), Some("Party 1 · Firebrand"));
    }

    #[test]
    fn a_bad_paste_reports_and_keeps_the_current_comp() {
        let (http, dir, t0) = setup();
        let mut d = driver(&http, &dir, t0);
        d.handle(Command::LoadInput(fixture("comp-tuesday.txt")), t0);
        d.handle(Command::LoadInput("hello".into()), t0);
        let snap = d.snapshot(t0);
        assert!(snap.load_error.is_some());
        assert_eq!(snap.header.comp_name.as_deref(), Some("Tuesday Zerg"));
    }

    #[test]
    fn a_link_is_fetched_then_polled_with_etag_on_interval_and_map_change() {
        let (http, dir, t0) = setup();
        http.on_etag(RAW, 200, &fixture("comp-tuesday.enc"), "\"v1\"").on(RAW, 304, "");
        let mut d = driver(&http, &dir, t0);
        d.handle(Command::LoadInput(link()), t0);
        assert_eq!(d.snapshot(t0).header.comp_name.as_deref(), Some("Tuesday Zerg"));
        assert!(d.snapshot(t0).header.source.starts_with("link · fetched"));
        assert_eq!(calls_to(&http, RAW), 1);

        d.tick(t0 + Duration::from_secs(60));
        assert_eq!(calls_to(&http, RAW), 1, "not due yet");
        d.tick(t0 + Duration::from_secs(601));
        assert_eq!(calls_to(&http, RAW), 2);
        assert_eq!(http.header(http.calls().len() - 1, "If-None-Match").as_deref(), Some("\"v1\""));

        d.handle(firebrand_in(1), t0 + Duration::from_secs(602));
        d.handle(firebrand_in(2), t0 + Duration::from_secs(603));
        d.tick(t0 + Duration::from_secs(604));
        assert_eq!(calls_to(&http, RAW), 3, "map change polls");
    }

    #[test]
    fn going_offline_keeps_the_comp_and_backs_off() {
        let (http, dir, t0) = setup();
        http.on(RAW, 200, &fixture("comp-tuesday.enc")).fail(RAW, "dns");
        http.fail(PAGES, "dns");
        let mut d = driver(&http, &dir, t0);
        d.handle(Command::LoadInput(link()), t0);
        let t1 = t0 + Duration::from_secs(601);
        d.tick(t1);
        let snap = d.snapshot(t1);
        assert!(snap.header.offline);
        assert_eq!(snap.header.comp_name.as_deref(), Some("Tuesday Zerg"));
        let before = calls_to(&http, RAW);
        d.tick(t1 + Duration::from_secs(30));
        assert_eq!(calls_to(&http, RAW), before, "backing off");
        d.tick(t1 + Duration::from_secs(61));
        assert_eq!(calls_to(&http, RAW), before + 1);
    }

    #[test]
    fn a_corrupted_publish_stops_polling_and_says_why() {
        let (http, dir, t0) = setup();
        http.on(RAW, 200, &fixture("comp-tuesday.enc")).on(RAW, 200, "!!!not base64!!!");
        let mut d = driver(&http, &dir, t0);
        d.handle(Command::LoadInput(link()), t0);
        d.tick(t0 + Duration::from_secs(601));
        let n = calls_to(&http, RAW);
        d.tick(t0 + Duration::from_secs(5000));
        assert_eq!(calls_to(&http, RAW), n);
        assert!(d.snapshot(t0).header.note.unwrap().contains("Couldn't read the comp"));
    }

    #[test]
    fn a_link_with_the_wrong_key_is_rejected_up_front() {
        let (http, dir, t0) = setup();
        http.on(RAW, 200, &fixture("comp-tuesday.enc"));
        let mut d = driver(&http, &dir, t0);
        d.handle(Command::LoadInput("https://someone.github.io/axibuilds/?c=e4369a53.AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh4".into()), t0);
        let snap = d.snapshot(t0);
        assert_eq!(snap.load_error.as_deref(), Some("Couldn't decrypt - check the link."));
        assert_eq!(snap.badge, Badge::NoComp);
    }

    #[test]
    fn api_polls_out_of_combat_only() {
        let (http, dir, t0) = setup();
        let b = gw2api::character_url("Tester", &["buildtabs", "active"]);
        let e = gw2api::character_url("Tester", &["equipmenttabs", "active"]);
        http.on(&b, 200, r#"{"build":{"profession":"Guardian","specializations":[{"id":42,"traits":[0,0,0]},{"id":46,"traits":[0,0,0]},{"id":62,"traits":[0,0,0]}],"skills":{"heal":1,"utilities":[2,3,4],"elite":5}}}"#)
            .on(&e, 200, r#"{"equipment":[]}"#);
        let mut d = driver(&http, &dir, t0);
        d.handle(Command::LoadInput(fixture("comp-tuesday.txt")), t0);
        d.handle(Command::SetApiKey("KEY".into()), t0);
        d.handle(firebrand_in(1), t0);
        d.tick(t0);
        assert_eq!((calls_to(&http, &b), calls_to(&http, &e)), (1, 1));
        assert!(d.session().api.snapshot.is_some());
        assert!(d.snapshot(t0).header.api_line.starts_with("API: snapshot"));

        d.handle(Command::Live(LiveEvent::Combat { active: true }), t0);
        d.handle(Command::RefreshApi, t0 + Duration::from_secs(31));
        d.tick(t0 + Duration::from_secs(400));
        assert_eq!(calls_to(&http, &b), 1, "never in combat");
        d.handle(Command::Live(LiveEvent::Combat { active: false }), t0);
        d.tick(t0 + Duration::from_secs(401));
        assert_eq!(calls_to(&http, &b), 2);
    }

    #[test]
    fn manual_picks_persist() {
        let (http, dir, t0) = setup();
        let mut d = driver(&http, &dir, t0);
        d.handle(Command::LoadInput(fixture("comp-tuesday.txt")), t0);
        d.handle(mumble("Tester", 8, 53, 1), t0);
        d.handle(Command::Pick(SlotRef { line: 1, slot: 1, build: 2 }), t0);
        assert_eq!(d.session().assignment, Assignment::Manual(SlotRef { line: 1, slot: 1, build: 2 }));
        let picker = d.snapshot(t0).picker;
        assert!(picker.iter().any(|p| p.current && p.slot.line == 1));
        assert!(picker.iter().any(|p| !p.enabled), "other specs shown disabled");

        let mut again = driver(&http, &dir, t0);
        again.handle(mumble("Tester", 8, 53, 1), t0);
        assert_eq!(again.session().assignment, Assignment::Manual(SlotRef { line: 1, slot: 1, build: 2 }));
    }

    #[test]
    fn badge_flashes_on_pass_to_fail() {
        let (http, dir, t0) = setup();
        let mut d = driver(&http, &dir, t0);
        d.handle(Command::LoadInput(fixture("comp-tuesday.txt")), t0);
        d.handle(firebrand_in(1), t0);
        d.handle(Command::Live(LiveEvent::BuffApply { id: 717, initial: true }), t0); // baseline, no food
        d.tick(t0 + Duration::from_secs(10));
        assert!(!d.snapshot(t0 + Duration::from_secs(10)).flash);
        let t1 = t0 + Duration::from_secs(31);
        d.tick(t1); // grace over: food is now a definite fail
        assert!(d.snapshot(t1 + Duration::from_secs(2)).flash);
        assert!(!d.snapshot(t1 + Duration::from_secs(4)).flash);
    }

    #[test]
    fn key_test_reports_missing_permissions() {
        let (http, dir, t0) = setup();
        http.on("https://api.guildwars2.com/v2/tokeninfo", 200, r#"{"name":"axigear","permissions":["account","characters"]}"#);
        let mut d = driver(&http, &dir, t0);
        d.handle(Command::SetApiKey("KEY".into()), t0);
        d.handle(Command::TestKey, t0);
        assert_eq!(d.snapshot(t0).key_test.as_deref(), Some("key is missing permissions: builds"));
    }

    #[test]
    fn a_failed_save_is_reported() {
        let (http, dir, t0) = setup();
        let blocked = dir.path().join("blocked");
        std::fs::write(&blocked, "x").unwrap();
        let mut d = Driver::new(http.clone() as Arc<dyn Http>, &blocked.join("axigear"), t0);
        assert_eq!(d.snapshot(t0).save_error, None);
        d.handle(Command::Settings(SettingsPatch::Hotkey("Ctrl+F9".into())), t0);
        assert!(d.snapshot(t0).save_error.unwrap().contains("config.json"));
    }

    #[test]
    fn test_key_without_a_key_makes_no_call() {
        let (http, dir, t0) = setup();
        let mut d = driver(&http, &dir, t0);
        d.handle(Command::SetApiKey("  ".into()), t0);
        d.handle(Command::TestKey, t0);
        assert_eq!(d.snapshot(t0).key_test.as_deref(), Some("no API key set"));
        assert!(http.calls().is_empty());
    }

    #[test]
    fn unsubscribe_clears_everything() {
        let (http, dir, t0) = setup();
        let mut d = driver(&http, &dir, t0);
        d.handle(Command::LoadInput(fixture("comp-tuesday.txt")), t0);
        d.handle(Command::Unsubscribe, t0);
        assert_eq!(d.snapshot(t0).badge, Badge::NoComp);
        assert!(!dir.path().join("comp_cache.json").exists());
        assert!(driver(&http, &dir, t0).session().comp.is_none());
    }

    #[test]
    fn shutdown_saves_and_stops() {
        let (http, dir, t0) = setup();
        let mut d = driver(&http, &dir, t0);
        d.handle(Command::Settings(SettingsPatch::Hotkey("Ctrl+F9".into())), t0);
        assert!(!d.handle(Command::Shutdown, t0));
        assert_eq!(Settings::load(&dir.path().join("config.json")).hotkey, "Ctrl+F9");
    }
}
