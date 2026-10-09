//! The worker's brain: owns settings, session and polling; turns commands and
//! the passage of time into fetches, checks and a `UiSnapshot` for the UI.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use std::collections::BTreeMap;

use crate::checks::ApiState;
use crate::comp_library;
use crate::consumables::Consumables;
use crate::gamedb::GameDb;
use crate::gw2api::{self, ApiError};
use crate::http::Http;
use crate::link::{AxiLink, LinkKind};
use crate::live::{IdentityChange, LiveEvent};
use crate::loader::{self, Fetched, Input, LoadError, MemberCache};
use crate::matcher;
use crate::model::{GameMode, SlotRef};
use crate::mumble::{self, MumbleSample};
use crate::report::{Badge, Category, CheckReport, SeveritySetting, Source, Status};
use crate::schedule::{RateLimit, Poller, API_BACKOFF, API_FAST_INTERVAL, API_INTERVAL, COMP_BACKOFF, COMP_INTERVAL, MANUAL_REFRESH};
use crate::session::{slot_label, Assignment, CompOrigin, LoadedComp, Session};
use crate::settings::{BadgeSettings, SavedComp, Settings};
use crate::specs::SpecDb;
use crate::text;

const FLASH: Duration = Duration::from_secs(3);

#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    LoadInput(String),
    /// Switch to a saved comp (by `input`), from its cached copy.
    UseComp(String),
    /// Drop a saved comp (by `input`) and its cache and picks.
    Unsubscribe(String),
    Pick(SlotRef),
    /// Re-fetch or re-decode a saved comp (by `input`).
    RefreshComp(String),
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
    LoadoutTab(crate::report::Tab),
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

/// One saved comp in the library list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompRow {
    pub input: String,
    pub name: String,
    /// "code", "link · fetched 2m ago" or "not loaded".
    pub source: String,
    pub active: bool,
    /// Last failed refresh of this (inactive) comp.
    pub error: Option<String>,
}

fn source_text(origin: &CompOrigin) -> String {
    match origin {
        CompOrigin::Code => "code".into(),
        CompOrigin::Link { fetched_at_unix, .. } => format!("link · fetched {}", text::ago(unix_now().saturating_sub(*fetched_at_unix))),
    }
}

#[derive(Debug, Clone)]
pub struct UiSnapshot {
    pub badge: Badge,
    pub report: Option<CheckReport>,
    pub header: Header,
    pub comps: Vec<CompRow>,
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
    /// Badge hover: how old the API data is, e.g. "gear as of 2m ago".
    pub badge_tooltip: String,
    /// Icon view of the assigned build; present whenever a slot is assigned.
    pub loadout: Option<crate::loadout::Loadout>,
}

impl UiSnapshot {
    /// Hidden in combat (unless configured otherwise) and, optionally, outside the comp's game mode.
    pub fn badge_visible(&self) -> bool {
        let b = &self.settings.badge;
        if b.hide_in_combat && self.in_combat {
            return false;
        }
        !(b.matching_mode_only && matches!((self.map_mode, self.comp_mode), (Some(m), Some(c)) if m != c))
    }
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
    /// Cached copy of each saved comp (same order not guaranteed).
    library: Vec<LoadedComp>,
    /// Last refresh error per saved comp that is not active.
    refresh_errors: BTreeMap<String, String>,
    /// The link being polled (resolved after the first fetch).
    subscription: Option<AxiLink>,
    /// Decrypted plaintext of the subscribed comp, in memory only: lets a comp that answers
    /// 304 still re-poll its v2 members. Until it is known (after a restart) the comp is
    /// fetched without `If-None-Match`.
    comp_plain: Option<Vec<u8>>,
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
        let mut settings = Settings::load(&paths.config);
        let mut session = Session::new(GameDb::load(&paths.itemdb));
        session.api.has_key = !settings.api_key.trim().is_empty();

        let library: Vec<LoadedComp> = comp_library::load(&paths.comp_cache)
            .into_iter()
            .filter(|c| settings.comps.iter().any(|s| s.input == c.input))
            .collect();
        for s in settings.comps.iter_mut().filter(|s| s.name.is_empty()) {
            if let Some(c) = library.iter().find(|c| c.input == s.input) {
                s.name = c.comp.name.clone();
            }
        }
        let active = settings.active_comp.clone().unwrap_or_default();
        let cached: Option<LoadedComp> = library.iter().find(|c| !active.is_empty() && c.input == active).cloned();
        let mut subscription = None;
        match (&cached, loader::detect(&active)) {
            (Some(LoadedComp { origin: CompOrigin::Link { link, .. }, .. }), _) => subscription = Some(link.clone()),
            (Some(_), _) => {}
            (None, Ok(Input::Comp { comp, key })) => {
                session.comp = Some(LoadedComp { comp, key, input: active.clone(), origin: CompOrigin::Code })
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
            library,
            refresh_errors: BTreeMap::new(),
            subscription,
            comp_plain: None,
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
            Command::UseComp(input) => self.use_comp(input, now),
            Command::Unsubscribe(input) => self.unsubscribe(&input, now),
            Command::Pick(slot) => {
                if self.session.pick(slot, &mut self.settings.picks, specs, now) {
                    self.save_settings();
                }
            }
            Command::RefreshComp(input) => {
                if self.comp_manual.try_acquire(now) {
                    self.refresh(input, now);
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
                    SettingsPatch::LoadoutTab(t) => self.settings.loadout_tab = t,
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
        let api_failing = self.report(now).is_some_and(|r| r.results.iter().any(|c| c.source == Source::Api && c.status == Status::Fail));
        self.api_poll.set_interval(if api_failing { API_FAST_INTERVAL } else { API_INTERVAL });
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
            source: lc.map(|c| source_text(&c.origin)).unwrap_or_default(),
            offline: self.comp_error.as_ref().is_some_and(LoadError::retryable),
            slot_label: report.as_ref().map(|r| r.slot_label.clone()),
            note: self.note(),
            api_line: self.api_line(now),
        };
        let comps = self
            .settings
            .comps
            .iter()
            .map(|s| {
                let cached = self.library.iter().find(|c| c.input == s.input);
                CompRow {
                    input: s.input.clone(),
                    name: s.display_name(),
                    source: cached.map_or_else(|| "not loaded".into(), |c| source_text(&c.origin)),
                    active: self.is_active(&s.input),
                    error: self.refresh_errors.get(&s.input).cloned(),
                }
            })
            .collect();
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
            comps,
            picker,
            load_error: self.load_error.clone(),
            key_test: self.key_test.clone(),
            in_combat: self.session.live.in_combat(),
            map_mode: identity.map(|i| mumble::game_mode_for_map_type(i.map_type)),
            comp_mode: lc.and_then(|c| c.comp.game_mode),
            settings: self.settings.clone(),
            flash: self.flash_until.is_some_and(|t| now < t),
            save_error: self.save_error.clone(),
            badge_tooltip: self.badge_tooltip(now),
            loadout: self.session.loadout(specs),
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

    fn is_active(&self, input: &str) -> bool {
        self.settings.active_comp.as_deref() == Some(input)
    }

    /// Save `lc` in the library and the settings list without switching to it.
    fn store(&mut self, lc: &LoadedComp) {
        match self.library.iter_mut().find(|c| c.input == lc.input) {
            Some(c) => *c = lc.clone(),
            None => self.library.push(lc.clone()),
        }
        match self.settings.comps.iter_mut().find(|s| s.input == lc.input) {
            Some(s) => s.name = lc.comp.name.clone(),
            None => self.settings.comps.insert(0, SavedComp { input: lc.input.clone(), name: lc.comp.name.clone() }),
        }
        self.save_library();
    }

    fn save_library(&mut self) {
        let keep: Vec<String> = self.settings.comps.iter().map(|s| s.input.clone()).collect();
        self.library.retain(|c| keep.contains(&c.input));
        let result = comp_library::save(&self.paths.comp_cache, &self.library);
        self.record_save("comp_cache.json", result);
    }

    /// Store `lc` and make it the active comp.
    fn commit(&mut self, lc: LoadedComp, now: Instant) {
        self.load_error = None;
        self.store(&lc);
        self.refresh_errors.remove(&lc.input);
        self.settings.active_comp = Some(lc.input.clone());
        self.session.set_comp(Some(lc), &self.settings.picks, SpecDb::bundled(), now);
        self.db_dirty = true;
        self.db_poll.reset();
        self.save_settings();
    }

    /// Switch to a cached comp: no network. Its link (if any) polls on the normal interval.
    fn activate(&mut self, lc: LoadedComp, now: Instant) {
        self.subscription = match &lc.origin {
            CompOrigin::Link { link, .. } => Some(link.clone()),
            CompOrigin::Code => None,
        };
        self.comp_plain = None;
        self.comp_error = None;
        self.load_error = None;
        self.comp_poll.reset();
        self.comp_poll.success(now);
        self.refresh_errors.remove(&lc.input);
        self.settings.active_comp = Some(lc.input.clone());
        self.session.set_comp(Some(lc), &self.settings.picks, SpecDb::bundled(), now);
        self.db_dirty = true;
        self.db_poll.reset();
        self.save_settings();
    }

    fn use_comp(&mut self, input: String, now: Instant) {
        if self.is_active(&input) && self.session.comp.is_some() {
            return;
        }
        match self.library.iter().find(|c| c.input == input).cloned() {
            Some(lc) => self.activate(lc, now),
            None if self.settings.comps.iter().any(|s| s.input == input) => self.load_input(input, now),
            None => {}
        }
    }

    fn unsubscribe(&mut self, input: &str, now: Instant) {
        let Some(idx) = self.settings.comps.iter().position(|s| s.input == input) else { return };
        self.settings.comps.remove(idx);
        if let Some(prefix) = self.library.iter().find(|c| c.input == input).map(|c| format!("{}|", c.key)) {
            self.settings.picks.retain(|k, _| !k.starts_with(&prefix));
        }
        self.refresh_errors.remove(input);
        self.save_library();
        if self.is_active(input) {
            self.subscription = None;
            self.comp_plain = None;
            self.comp_error = None;
            self.load_error = None;
            self.settings.active_comp = None;
            self.session.set_comp(None, &self.settings.picks, SpecDb::bundled(), now);
            let next = self.settings.comps.get(idx).or(self.settings.comps.last()).map(|s| s.input.clone());
            if let Some(next) = next {
                self.use_comp(next, now);
            }
        }
        self.save_settings();
    }

    fn refresh(&mut self, input: String, now: Instant) {
        if self.is_active(&input) {
            if self.subscription.is_some() {
                self.comp_poll.reset();
                self.poll_comp(now);
            } else {
                self.load_input(input, now);
            }
            return;
        }
        if !self.settings.comps.iter().any(|s| s.input == input) {
            return;
        }
        match self.fetch_input(&input) {
            Ok((lc, _)) => {
                self.refresh_errors.remove(&input);
                self.store(&lc);
                self.save_settings();
            }
            Err(e) => {
                self.refresh_errors.insert(input, e);
            }
        }
    }

    /// Decode or fetch `text` without touching the active comp. A link's
    /// first fetch never sends `If-None-Match`; `Some(plain)` is a comp link's plaintext.
    fn fetch_input(&self, text: &str) -> Result<(LoadedComp, Option<Vec<u8>>), String> {
        match loader::detect(text).map_err(|e| e.to_string())? {
            Input::Comp { comp, key } => Ok((LoadedComp { comp, key, input: text.to_string(), origin: CompOrigin::Code }, None)),
            Input::Link(link) => match loader::fetch(&*self.http, &link, None, &MemberCache::new()).map_err(|e| e.to_string())? {
                Fetched::Fresh { comp, etag, link, plain, members } => {
                    let key = loader::link_key(&link);
                    Ok((LoadedComp { comp, key, input: text.to_string(), origin: CompOrigin::Link { link, etag, fetched_at_unix: unix_now(), members } }, plain))
                }
                Fetched::NotModified => Err("unexpected 304 on first fetch".into()),
            },
        }
    }

    fn load_input(&mut self, text: String, now: Instant) {
        let text = text.trim().to_string();
        match self.fetch_input(&text) {
            Err(e) => self.load_error = Some(e),
            Ok((lc, plain)) => {
                self.subscription = match &lc.origin {
                    CompOrigin::Link { link, .. } => Some(link.clone()),
                    CompOrigin::Code => None,
                };
                self.comp_plain = plain;
                self.comp_error = None;
                self.comp_poll.reset();
                self.comp_poll.success(now);
                self.commit(lc, now);
            }
        }
    }

    fn poll_comp(&mut self, now: Instant) {
        let Some(link) = self.subscription.clone() else { return };
        let (etag, members) = match &self.session.comp {
            Some(LoadedComp { origin: CompOrigin::Link { etag, members, .. }, .. }) => (etag.clone(), members.clone()),
            _ => (None, MemberCache::new()),
        };
        // A comp link's 304 is only usable when the plaintext is at hand to re-poll members.
        let etag = etag.filter(|_| link.kind == LinkKind::Build || self.comp_plain.is_some());
        let result = loader::fetch(&*self.http, &link, etag.as_deref(), &members).and_then(|fetched| match (fetched, &self.comp_plain) {
            (Fetched::NotModified, Some(plain)) => Ok(loader::refresh_members(&*self.http, plain, &members)?.map(|(comp, members)| (comp, members, None))),
            (Fetched::NotModified, None) => Ok(None),
            (Fetched::Fresh { comp, etag, link, plain, members }, _) => Ok(Some((comp, members, Some((etag, link, plain))))),
        });
        match result {
            Ok(update) => {
                self.comp_poll.success(now);
                self.comp_error = None;
                let Some((comp, members, fresh)) = update else {
                    if let Some(LoadedComp { origin: CompOrigin::Link { fetched_at_unix, .. }, .. }) = &mut self.session.comp {
                        *fetched_at_unix = unix_now();
                    }
                    // The options row reads the library copy, so keep its age in step.
                    if let Some(active) = self.settings.active_comp.clone() {
                        if let Some(LoadedComp { origin: CompOrigin::Link { fetched_at_unix, .. }, .. }) = self.library.iter_mut().find(|c| c.input == active) {
                            *fetched_at_unix = unix_now();
                        }
                    }
                    return;
                };
                let (etag, link) = match fresh {
                    Some((etag, link, plain)) => {
                        self.comp_plain = plain;
                        (etag, link)
                    }
                    None => (etag, link), // the comp itself was 304: keep its ETag
                };
                self.subscription = Some(link.clone());
                let key = loader::link_key(&link);
                let input = self.settings.active_comp.clone().unwrap_or_default();
                self.commit(LoadedComp { comp, key, input, origin: CompOrigin::Link { link, etag, fetched_at_unix: unix_now(), members } }, now);
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
            Assignment::NoMatch => match self.session.live.identity.as_ref().map(|id| id.profession) {
                Some(p) if mumble::profession_name(p).is_none() => Some(mumble::unknown_profession(p)),
                _ => Some("no slot in this comp matches your spec".into()),
            },
            Assignment::Ambiguous(_) => Some("several slots match - pick yours".into()),
            Assignment::Unassigned => Some("waiting for your character".into()),
            Assignment::Auto(_) | Assignment::Manual(_) => None,
        }
    }

    fn badge_tooltip(&self, now: Instant) -> String {
        let api = &self.session.api;
        match (&api.snapshot, &api.error) {
            (Some(s), err) if api.has_key => {
                let age = text::ago(now.saturating_duration_since(s.fetched_at).as_secs());
                match err {
                    Some(e) => format!("gear as of {age} · {e}"),
                    None => format!("gear as of {age}"),
                }
            }
            _ => self.api_line(now),
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
    use crate::testutil::{fixture, fixture_bytes, reseal_member};

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
        assert_eq!(d.settings().active_comp, Some(fixture("comp-tuesday.txt")));

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
    fn a_304_refreshes_the_active_rows_fetched_age() {
        let (http, dir, t0) = setup();
        http.on_etag(RAW, 200, &fixture("comp-tuesday.enc"), "\"v1\"").on(RAW, 304, "");
        let mut d = driver(&http, &dir, t0);
        d.handle(Command::LoadInput(link()), t0);
        // Pretend the fetch happened long ago, in both copies.
        for c in d.library.iter_mut().chain(d.session.comp.iter_mut()) {
            if let CompOrigin::Link { fetched_at_unix, .. } = &mut c.origin {
                *fetched_at_unix = 1000;
            }
        }
        d.tick(t0 + Duration::from_secs(601));
        assert_eq!(calls_to(&http, RAW), 2);
        let snap = d.snapshot(t0 + Duration::from_secs(601));
        let row = snap.comps.iter().find(|r| r.active).unwrap();
        assert_eq!(row.source, snap.header.source);
        assert!(!row.source.contains("fetched 2"), "{}", row.source);
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

    fn api_routes(http: &FakeHttp) -> (String, String) {
        let b = gw2api::character_url("Tester", &["buildtabs", "active"]);
        let e = gw2api::character_url("Tester", &["equipmenttabs", "active"]);
        http.on(&b, 200, r#"{"build":{"profession":"Guardian","specializations":[{"id":42,"traits":[0,0,0]},{"id":46,"traits":[0,0,0]},{"id":62,"traits":[0,0,0]}],"skills":{"heal":1,"utilities":[2,3,4],"elite":5}}}"#)
            .on(&e, 200, r#"{"equipment":[]}"#);
        (b, e)
    }

    #[test]
    fn failing_api_checks_poll_every_minute() {
        let (http, dir, t0) = setup();
        let (b, _) = api_routes(&http);
        let mut d = driver(&http, &dir, t0);
        d.handle(Command::LoadInput(fixture("comp-tuesday.txt")), t0);
        d.handle(Command::SetApiKey("KEY".into()), t0);
        d.handle(firebrand_in(1), t0);
        d.tick(t0);
        assert!(d.report(t0).unwrap().results.iter().any(|r| r.source == Source::Api && r.status == Status::Fail));
        d.tick(t0 + Duration::from_secs(59));
        assert_eq!(calls_to(&http, &b), 1);
        d.tick(t0 + Duration::from_secs(60));
        assert_eq!(calls_to(&http, &b), 2);
    }

    #[test]
    fn passing_api_checks_keep_the_five_minute_interval() {
        let (http, dir, t0) = setup();
        let (b, _) = api_routes(&http);
        let mut d = driver(&http, &dir, t0);
        for c in Category::ALL.into_iter().filter(|c| c.source() == Source::Api) {
            d.handle(Command::Settings(SettingsPatch::Severity(c, SeveritySetting::Off)), t0);
        }
        d.handle(Command::LoadInput(fixture("comp-tuesday.txt")), t0);
        d.handle(Command::SetApiKey("KEY".into()), t0);
        d.handle(firebrand_in(1), t0);
        d.tick(t0);
        d.tick(t0 + Duration::from_secs(60));
        assert_eq!(calls_to(&http, &b), 1);
        d.tick(t0 + Duration::from_secs(300));
        assert_eq!(calls_to(&http, &b), 2);
    }

    #[test]
    fn badge_tooltip_says_how_old_the_gear_is() {
        let (http, dir, t0) = setup();
        api_routes(&http);
        let mut d = driver(&http, &dir, t0);
        assert_eq!(d.snapshot(t0).badge_tooltip, "API: needs key (characters, builds)");
        d.handle(Command::LoadInput(fixture("comp-tuesday.txt")), t0);
        d.handle(Command::SetApiKey("KEY".into()), t0);
        d.handle(firebrand_in(1), t0);
        d.tick(t0);
        assert_eq!(d.snapshot(t0 + Duration::from_secs(150)).badge_tooltip, "gear as of 2m ago");
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
        d.handle(Command::Unsubscribe(fixture("comp-tuesday.txt")), t0);
        assert_eq!(d.snapshot(t0).badge, Badge::NoComp);
        assert!(crate::comp_library::load(&dir.path().join("comp_cache.json")).is_empty());
        assert!(driver(&http, &dir, t0).session().comp.is_none());
    }

    #[test]
    fn loadout_tab_is_a_saved_setting() {
        let (http, dir, t0) = setup();
        let mut d = driver(&http, &dir, t0);
        assert_eq!(d.snapshot(t0).settings.loadout_tab, crate::report::Tab::Build);
        d.handle(Command::Settings(SettingsPatch::LoadoutTab(crate::report::Tab::Equipment)), t0);
        assert_eq!(d.snapshot(t0).settings.loadout_tab, crate::report::Tab::Equipment);
    }

    #[test]
    fn snapshot_carries_the_assigned_builds_loadout() {
        let (http, dir, t0) = setup();
        let mut d = driver(&http, &dir, t0);
        assert!(d.snapshot(t0).loadout.is_none());
        d.handle(Command::LoadInput(fixture("comp-tuesday.txt")), t0);
        d.handle(firebrand_in(1), t0);
        let snap = d.snapshot(t0);
        assert!(snap.report.is_some());
        let l = snap.loadout.expect("loadout with a report");
        assert_eq!(l.skills.len(), 5);
    }

    #[test]
    fn shutdown_saves_and_stops() {
        let (http, dir, t0) = setup();
        let mut d = driver(&http, &dir, t0);
        d.handle(Command::Settings(SettingsPatch::Hotkey("Ctrl+F9".into())), t0);
        assert!(!d.handle(Command::Shutdown, t0));
        assert_eq!(Settings::load(&dir.path().join("config.json")).hotkey, "Ctrl+F9");
    }

    #[test]
    fn badge_visibility_rules() {
        let (http, dir, t0) = setup();
        let mut d = driver(&http, &dir, t0);
        d.handle(Command::LoadInput(fixture("comp-tuesday.txt")), t0); // comp game mode: wvw
        d.handle(Command::Mumble(MumbleSample {
            ui_tick: 1,
            identity: r#"{"name":"Tester","profession":1,"spec":62,"map_id":1}"#.into(),
            context: { let mut c = vec![0u8; 52]; c[32..36].copy_from_slice(&5u32.to_le_bytes()); c }, // PvE map
        }), t0);
        assert!(d.snapshot(t0).badge_visible());
        d.handle(Command::Settings(SettingsPatch::Badge(BadgeSettings { matching_mode_only: true, ..Default::default() })), t0);
        assert!(!d.snapshot(t0).badge_visible(), "PvE map, WvW comp");
        d.handle(Command::Settings(SettingsPatch::Badge(BadgeSettings::default())), t0);
        d.handle(Command::Live(LiveEvent::Combat { active: true }), t0);
        assert!(!d.snapshot(t0).badge_visible(), "hidden in combat by default");
        d.handle(Command::Settings(SettingsPatch::Badge(BadgeSettings { hide_in_combat: false, ..Default::default() })), t0);
        assert!(d.snapshot(t0).badge_visible());
    }

    const MEMBER_BASE: &str = "https://raw.githubusercontent.com/teammate/axibuilds/main/site/builds/";
    const MEMBERS: [(&str, &str); 3] =
        [("aaaa0001", "member-firebrand.enc"), ("bbbb0002", "member-berserker.enc.v2"), ("cccc0003", "member-necro.enc.v2")];
    const FIREBRAND_KEY: &str = "AgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgI";

    fn member(id: &str) -> String {
        format!("{MEMBER_BASE}{id}.enc")
    }

    /// The v2 comp (200 "c1", then 304) and its members (200 "m1", then whatever `then` queues).
    fn v2_routes(http: &FakeHttp, then: impl Fn(&FakeHttp, &str)) {
        http.on_bytes_etag(RAW, 200, &fixture_bytes("comp-tuesday.v2.enc.v2"), "\"c1\"").on(RAW, 304, "");
        for (id, file) in MEMBERS {
            http.on_bytes_etag(&member(id), 200, &fixture_bytes(file), "\"m1\"");
            then(http, id);
        }
    }

    fn guardian_relic(d: &Driver) -> Option<String> {
        let comp = &d.session().comp.as_ref().unwrap().comp;
        comp.builds.iter().find(|b| b.profession == "Guardian").unwrap().equipment.relic.clone()
    }

    fn header_for(http: &FakeHttp, url: &str, name: &str) -> Vec<Option<String>> {
        let calls = http.calls();
        (0..calls.len()).filter(|i| calls[*i] == url).map(|i| http.header(i, name)).collect()
    }

    #[test]
    fn a_v2_comp_answering_304_still_picks_up_member_edits() {
        let (http, dir, t0) = setup();
        let edited = reseal_member("member-firebrand.enc", FIREBRAND_KEY, |b| b["equipment"]["relic"] = "Relic of the Monk".into());
        v2_routes(&http, |http, id| {
            if id == "aaaa0001" {
                http.on_bytes_etag(&member(id), 200, &edited, "\"m2\"");
            } else {
                http.on(&member(id), 304, "");
            }
        });
        let mut d = driver(&http, &dir, t0);
        d.handle(Command::LoadInput(link()), t0);
        assert_eq!(guardian_relic(&d).as_deref(), Some("Relic of the Flock"));

        d.tick(t0 + Duration::from_secs(601));
        assert_eq!(header_for(&http, RAW, "If-None-Match").last().cloned().flatten().as_deref(), Some("\"c1\""));
        assert_eq!(header_for(&http, &member("aaaa0001"), "If-None-Match"), [None, Some("\"m1\"".into())]);
        assert_eq!(guardian_relic(&d).as_deref(), Some("Relic of the Monk"), "member edit picked up behind a comp 304");
        assert_eq!(d.session().comp.as_ref().unwrap().comp.builds.len(), 3);

        // The next poll revalidates the edited member with its new ETag; the comp keeps "c1".
        d.tick(t0 + Duration::from_secs(1202));
        assert_eq!(header_for(&http, RAW, "If-None-Match").last().cloned().flatten().as_deref(), Some("\"c1\""));
        assert_eq!(header_for(&http, &member("aaaa0001"), "If-None-Match").last().cloned().flatten().as_deref(), Some("\"m2\""));

        // Manual refresh takes the same path.
        let mut again = driver(&http, &dir, t0);
        assert_eq!(guardian_relic(&again).as_deref(), Some("Relic of the Monk"), "persisted");
        again.handle(Command::RefreshComp(link()), t0);
        assert_eq!(header_for(&http, RAW, "If-None-Match").last().cloned().flatten(), None, "no plaintext after restart: full comp fetch");
        assert_eq!(guardian_relic(&again).as_deref(), Some("Relic of the Monk"));
    }

    #[test]
    fn a_v2_comp_and_members_all_304_change_nothing() {
        let (http, dir, t0) = setup();
        v2_routes(&http, |http, id| {
            http.on(&member(id), 304, "");
        });
        let mut d = driver(&http, &dir, t0);
        d.handle(Command::LoadInput(link()), t0);
        let before = d.session().comp.clone().unwrap();
        let cache = dir.path().join("comp_cache.json");
        std::fs::remove_file(&cache).unwrap();

        d.tick(t0 + Duration::from_secs(601));
        for (id, _) in MEMBERS {
            assert_eq!(header_for(&http, &member(id), "If-None-Match"), [None, Some("\"m1\"".into())]);
        }
        assert!(!cache.exists(), "no rebuild, no rewrite");
        assert_eq!(d.session().comp.as_ref().unwrap().comp, before.comp);
        assert!(!d.snapshot(t0).header.offline);
    }

    #[test]
    fn a_v2_member_unreachable_behind_a_comp_304_keeps_the_comp_and_backs_off() {
        let (http, dir, t0) = setup();
        v2_routes(&http, |http, id| {
            if id == "bbbb0002" {
                http.fail(&member(id), "dns");
            } else {
                http.on(&member(id), 304, "");
            }
        });
        let mut d = driver(&http, &dir, t0);
        d.handle(Command::LoadInput(link()), t0);
        let before = d.session().comp.clone().unwrap();
        let t1 = t0 + Duration::from_secs(601);
        d.tick(t1);
        assert!(d.snapshot(t1).header.offline);
        assert_eq!(d.session().comp.as_ref().unwrap().comp, before.comp);
    }

    #[test]
    fn a_comp_cache_without_member_states_still_loads() {
        let (http, dir, t0) = setup();
        v2_routes(&http, |_, _| {});
        let mut d = driver(&http, &dir, t0);
        d.handle(Command::LoadInput(link()), t0);
        let path = dir.path().join("comp_cache.json");
        let mut json: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        json["comps"][0]["origin"]["Link"].as_object_mut().unwrap().remove("members").expect("members persisted");
        std::fs::write(&path, json.to_string()).unwrap();
        assert_eq!(driver(&http, &dir, t0).session().comp.as_ref().unwrap().comp.name, "Tuesday Zerg");
    }

    #[test]
    fn an_unknown_profession_says_so_instead_of_no_slot_matches() {
        let (http, dir, t0) = setup();
        let mut d = driver(&http, &dir, t0);
        d.handle(Command::LoadInput(fixture("comp-tuesday.txt")), t0);
        d.handle(mumble("Tester", 12, 0, 1), t0);
        assert_eq!(d.snapshot(t0).header.note.as_deref(), Some("unknown profession id 12 - update axigear"));
        d.handle(mumble("Tester", 2, 0, 1), t0); // a known profession with no slot: unchanged
        assert_eq!(d.snapshot(t0).header.note.as_deref(), Some("no slot in this comp matches your spec"));
    }

    fn code_a() -> String { fixture("comp-tuesday.txt") }
    fn code_b() -> String { fixture("build-firebrand.txt") }

    #[test]
    fn loading_two_comps_keeps_both_newest_active() {
        let (http, dir, t0) = setup();
        let mut d = driver(&http, &dir, t0);
        d.handle(Command::LoadInput(code_a()), t0);
        d.handle(Command::LoadInput(code_b()), t0);
        let inputs: Vec<&str> = d.settings().comps.iter().map(|c| c.input.as_str()).collect();
        assert_eq!(inputs, [code_b().as_str(), code_a().as_str()]);
        assert_eq!(d.settings().active_comp, Some(code_b()));
        assert_eq!(d.settings().comps[1].name, "Tuesday Zerg");
    }

    #[test]
    fn reloading_the_same_input_refreshes_without_duplicating() {
        let (http, dir, t0) = setup();
        let mut d = driver(&http, &dir, t0);
        d.handle(Command::LoadInput(code_a()), t0);
        d.handle(Command::LoadInput(code_b()), t0);
        d.handle(Command::LoadInput(format!("{}\n  ", code_a())), t0);
        assert_eq!(d.settings().comps.len(), 2);
        assert_eq!(d.settings().active_comp, Some(code_a()));
    }

    #[test]
    fn use_comp_switches_from_cache_without_network() {
        let (http, dir, t0) = setup();
        http.on(RAW, 200, &fixture("comp-tuesday.enc"));
        let mut d = driver(&http, &dir, t0);
        d.handle(Command::LoadInput(link()), t0);
        d.handle(Command::LoadInput(code_b()), t0);
        let calls = http.calls().len();
        d.handle(Command::UseComp(link()), t0);
        assert_eq!(http.calls().len(), calls, "switching needs no fetch");
        assert_eq!(d.settings().active_comp, Some(link()));
        assert_eq!(d.snapshot(t0).header.comp_name.as_deref(), Some("Tuesday Zerg"));
        assert!(d.snapshot(t0).header.source.starts_with("link"));
    }

    #[test]
    fn picks_survive_switching_comps() {
        let (http, dir, t0) = setup();
        let mut d = driver(&http, &dir, t0);
        d.handle(Command::LoadInput(code_a()), t0);
        d.handle(mumble("Tester", 8, 53, 1), t0);
        let slot = SlotRef { line: 1, slot: 1, build: 2 };
        d.handle(Command::Pick(slot), t0);
        assert_eq!(d.session().assignment, Assignment::Manual(slot), "pick took effect");
        let before = d.snapshot(t0).header.slot_label.clone();
        d.handle(Command::LoadInput(code_b()), t0);
        d.handle(Command::UseComp(code_a()), t0);
        assert_eq!(d.snapshot(t0).header.slot_label, before);
        assert_eq!(d.session().assignment, Assignment::Manual(slot), "pick survived the round trip");
    }

    #[test]
    fn unsubscribing_the_active_comp_activates_the_next() {
        let (http, dir, t0) = setup();
        let mut d = driver(&http, &dir, t0);
        d.handle(Command::LoadInput(code_a()), t0);
        d.handle(Command::LoadInput(code_b()), t0);
        d.handle(Command::Unsubscribe(code_b()), t0);
        assert_eq!(d.settings().active_comp, Some(code_a()));
        assert_eq!(d.snapshot(t0).header.comp_name.as_deref(), Some("Tuesday Zerg"));
        d.handle(Command::Unsubscribe(code_a()), t0);
        assert!(d.settings().comps.is_empty() && d.settings().active_comp.is_none());
        assert_eq!(d.snapshot(t0).badge, Badge::NoComp);
        assert!(crate::comp_library::load(&dir.path().join("comp_cache.json")).is_empty());
        assert!(driver(&http, &dir, t0).session().comp.is_none());
    }

    #[test]
    fn unsubscribing_another_comp_keeps_the_active_one() {
        let (http, dir, t0) = setup();
        http.on_etag(RAW, 200, &fixture("comp-tuesday.enc"), "\"v1\"").on(RAW, 304, "");
        let mut d = driver(&http, &dir, t0);
        d.handle(Command::LoadInput(code_b()), t0);
        d.handle(Command::LoadInput(link()), t0);
        d.handle(Command::Unsubscribe(code_b()), t0);
        assert_eq!(d.settings().active_comp, Some(link()));
        d.tick(t0 + Duration::from_secs(601));
        assert_eq!(calls_to(&http, RAW), 2, "still polling the active link");
    }

    #[test]
    fn unsubscribe_drops_that_comps_picks_only() {
        let (http, dir, t0) = setup();
        let mut d = driver(&http, &dir, t0);
        d.handle(Command::LoadInput(code_a()), t0);
        let key_a = d.session().comp.as_ref().unwrap().key.clone();
        d.handle(Command::LoadInput(code_b()), t0);
        let key_b = d.session().comp.as_ref().unwrap().key.clone();
        d.settings.picks.insert(format!("{key_a}|Tester"), SlotRef { line: 0, slot: 0, build: 0 });
        d.settings.picks.insert(format!("{key_b}|Tester"), SlotRef { line: 0, slot: 0, build: 0 });
        d.handle(Command::Unsubscribe(code_a()), t0);
        assert!(!d.settings().picks.contains_key(&format!("{key_a}|Tester")));
        assert!(d.settings().picks.contains_key(&format!("{key_b}|Tester")));
    }

    #[test]
    fn refreshing_an_inactive_comp_does_not_switch() {
        let (http, dir, t0) = setup();
        http.on(RAW, 200, &fixture("comp-tuesday.enc"));
        let mut d = driver(&http, &dir, t0);
        d.handle(Command::LoadInput(link()), t0);
        d.handle(Command::LoadInput(code_b()), t0);
        d.handle(Command::RefreshComp(link()), t0);
        assert_eq!(calls_to(&http, RAW), 2);
        assert_eq!(d.settings().active_comp, Some(code_b()));
    }

    #[test]
    fn restart_resumes_the_active_link_only() {
        let (http, dir, t0) = setup();
        http.on(RAW, 200, &fixture("comp-tuesday.enc"));
        let mut d = driver(&http, &dir, t0);
        d.handle(Command::LoadInput(link()), t0);
        d.handle(Command::LoadInput(code_b()), t0);
        let mut again = driver(&http, &dir, t0);
        assert_eq!(again.settings().active_comp, Some(code_b()));
        let calls = calls_to(&http, RAW);
        again.tick(t0 + Duration::from_secs(5000));
        assert_eq!(calls_to(&http, RAW), calls, "a code comp is active: no link polling");
        again.handle(Command::UseComp(link()), t0);
        assert_eq!(again.snapshot(t0).header.comp_name.as_deref(), Some("Tuesday Zerg"));
    }

    #[test]
    fn a_v01_config_and_cache_still_load() {
        let (http, dir, t0) = setup();
        let mut d = driver(&http, &dir, t0);
        d.handle(Command::LoadInput(code_a()), t0);
        let lc = d.session().comp.clone().unwrap();
        std::fs::write(dir.path().join("comp_cache.json"), serde_json::to_vec(&lc).unwrap()).unwrap();
        std::fs::write(dir.path().join("config.json"), serde_json::json!({ "comp_input": code_a() }).to_string()).unwrap();
        let again = driver(&http, &dir, t0);
        assert_eq!(again.settings().active_comp, Some(code_a()));
        assert_eq!(again.settings().comps[0].name, "Tuesday Zerg", "name filled from cache");
        assert!(again.session().comp.is_some());
    }

    #[test]
    fn snapshot_lists_saved_comps() {
        let (http, dir, t0) = setup();
        http.on(RAW, 200, &fixture("comp-tuesday.enc")).fail(RAW, "dns");
        http.fail(PAGES, "dns");
        let mut d = driver(&http, &dir, t0);
        d.handle(Command::LoadInput(link()), t0);
        d.handle(Command::LoadInput(code_b()), t0);
        d.handle(Command::RefreshComp(link()), t0);
        let rows = d.snapshot(t0).comps;
        assert_eq!(rows.len(), 2);
        assert!(rows[0].active && rows[0].source == "code" && rows[0].input == code_b());
        assert!(!rows[1].active && rows[1].name == "Tuesday Zerg");
        assert!(rows[1].source.starts_with("link · fetched"), "{}", rows[1].source);
        assert!(rows[1].error.is_some(), "failed refresh shows on its row");
    }

    #[test]
    fn an_uncached_saved_comp_says_not_loaded() {
        let (http, dir, t0) = setup();
        std::fs::write(dir.path().join("config.json"), r#"{"comps":[{"input":"https://x.invalid/?c=1","name":""}]}"#).unwrap();
        let d = driver(&http, &dir, t0);
        let rows = d.snapshot(t0).comps;
        assert_eq!(rows[0].source, "not loaded");
        assert_eq!(rows[0].name, "https://x.invalid/?c=1");
    }

    #[test]
    fn activating_a_comp_clears_its_stale_refresh_error() {
        let (http, dir, t0) = setup();
        http.on(RAW, 200, &fixture("comp-tuesday.enc")).fail(RAW, "dns");
        http.fail(PAGES, "dns");
        let mut d = driver(&http, &dir, t0);
        d.handle(Command::LoadInput(link()), t0);
        d.handle(Command::LoadInput(code_b()), t0);
        d.handle(Command::RefreshComp(link()), t0);
        assert!(d.snapshot(t0).comps[1].error.is_some());
        d.handle(Command::UseComp(link()), t0);
        let rows = d.snapshot(t0).comps;
        assert!(rows[1].active);
        assert_eq!(rows[1].error, None);
    }
}
