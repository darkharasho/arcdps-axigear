# Comp library, saved API key, Equipment tidy-up — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Users can save several comps and switch, refresh, or unsubscribe each one. The API key saves itself. The Equipment tab shows rune and sigil names, per-slot infusions, aligned trinket rows, and real food and utility names.

**Architecture:** Comp state moves from the single `Settings.comp_input` to a saved list, `Settings.comps` plus `Settings.active_comp`. Every comp's cached copy goes into one `comp_cache.json`, through a new `comp_library` module. `Driver` gains a `library: Vec<LoadedComp>` and splits saving a comp (`store`) from switching to it (`activate` and `commit`). Equipment work stays in `axigear-core`: per-slot infusions in the model, item-ID consumables resolved through `GameDb`, and per-ID infusion marks. `Loadout` exposes rows the UI only lays out.

**Tech Stack:** Rust workspace. `axigear-core` is host-tested with `cargo test`. `axigear` is the arcdps plugin, with an imgui UI that is Windows-only and checked with `cargo dll-check`.

**Spec:** `docs/superpowers/specs/2026-10-08-comp-library-equipment-design.md`

## Global Constraints

- **Branch:** `feat/loadout-view`. Commit after each task, and end every commit message with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`. Never push.
- **Tests:** `cargo test --workspace` must pass after every task. Any task that touches `crates/axigear/src/ui` must also pass `cargo dll-check` (cargo-xwin, msvc). Never build windows-gnu.
- **Colours:** only from `crates/axigear/src/ui/theme.rs`. Raised elements go through `ui/axi.rs`, only one draw list may be live at a time, and `tests/axi_guard_test.rs` must pass. Status icons are drawn, never glyphs.
- **The API key:** never logged, printed, or shown in plain text. The field stays `.password(true)`.
- **Old files:** old `config.json` (with `comp_input`) and old `comp_cache.json` (a single `LoadedComp`) must load.
- **`comp_cache.json` format:** `{ "comps": [LoadedComp, ...] }`.
- **Section headings, exact text:** `ARMOR`, `WEAPONS · SET A`, `WEAPONS · SET B`, `TRINKETS`, `CONSUMABLES`.
- **Strings, exact text:**
  - consumable reason: `"looking up item name"`;
  - unknown item tile: `"Item {id}"`;
  - comp row with no cache: `"not loaded"`;
  - key status: `"Saved"`.
- **Equipment columns:** `COL_W = 340.0`, gap `12.0`.

## Review Focus

1. **Restarting with an active link comp:** it must resume polling that link, and only that link. Pinned in Task 3: `restart_resumes_the_active_link_only`.
2. **Unsubscribing a comp that isn't active:** the active comp, its subscription and its poll schedule must not change. Pinned in Task 3: `unsubscribing_another_comp_keeps_the_active_one`.
3. **The same comp pasted with surrounding whitespace or newlines** counts as the same saved comp. Pinned in Task 3: `reloading_the_same_input_refreshes_without_duplicating`, which loads with trailing `"\n  "`.
4. **A food ID that the item DB knows by name, while the live buff is the "Mists-Infused" variant,** still passes. Pinned in Task 6: `numeric_food_matches_mists_infused_variant`.
5. **A `config.json` with both `comp_input` and an already-filled `comps`:** `comps` wins and is not overwritten. Pinned in Task 1: `migration_does_not_clobber_existing_comps`.

---

### Task 1: Saved-comp settings with migration

**Files:**
- Modify: `crates/axigear-core/src/settings.rs`

**Interfaces:**
- Produces:
  - `pub struct SavedComp { pub input: String, pub name: String }` (derives Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default);
  - `impl SavedComp { pub fn display_name(&self) -> String }`;
  - `Settings.comps: Vec<SavedComp>`;
  - `Settings.active_comp: Option<String>`.
  - `Settings.comp_input` stays, but is legacy and read-only: `#[serde(default, skip_serializing)]`. It is emptied on load after migration.

- [ ] **Step 1: Write the failing tests.** Add them to `settings.rs` `mod tests`:

```rust
    #[test]
    fn old_comp_input_migrates_to_the_list() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        std::fs::write(&path, r#"{"comp_input":"CODE1"}"#).unwrap();
        let s = Settings::load(&path);
        assert_eq!(s.comps, vec![SavedComp { input: "CODE1".into(), name: String::new() }]);
        assert_eq!(s.active_comp.as_deref(), Some("CODE1"));
        assert!(s.comp_input.is_empty());
        s.save(&path).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(!text.contains("comp_input"), "{text}");
        assert_eq!(Settings::load(&path), s);
    }

    #[test]
    fn migration_does_not_clobber_existing_comps() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        std::fs::write(&path, r#"{"comp_input":"OLD","comps":[{"input":"NEW","name":"N"}],"active_comp":"NEW"}"#).unwrap();
        let s = Settings::load(&path);
        assert_eq!(s.comps.len(), 1);
        assert_eq!(s.comps[0].input, "NEW");
        assert_eq!(s.active_comp.as_deref(), Some("NEW"));
    }

    #[test]
    fn display_name_falls_back_to_a_short_input() {
        let named = SavedComp { input: "x".into(), name: "Tuesday".into() };
        assert_eq!(named.display_name(), "Tuesday");
        let long = SavedComp { input: "https://someone.github.io/axibuilds/?c=abc".into(), name: String::new() };
        assert_eq!(long.display_name(), "https://someone.github.i…");
        let short = SavedComp { input: "abc".into(), name: String::new() };
        assert_eq!(short.display_name(), "abc");
    }
```

- [ ] **Step 2: Run the tests and check they fail.** Run `cargo test -p axigear-core settings::`. Expected: compile errors, because `SavedComp`, `comps` and `active_comp` don't exist yet.

- [ ] **Step 3: Implement.** In `settings.rs`:

```rust
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SavedComp {
    /// Exactly what was loaded (trimmed): a code or a link.
    pub input: String,
    /// Comp name from the last successful load; "" until then.
    pub name: String,
}

impl SavedComp {
    /// The name, or the first 24 characters of the input when no name is known yet.
    pub fn display_name(&self) -> String {
        if !self.name.is_empty() {
            return self.name.clone();
        }
        match self.input.char_indices().nth(24) {
            Some((i, _)) => format!("{}…", &self.input[..i]),
            None => self.input.clone(),
        }
    }
}
```

In `Settings`, replace the `comp_input` field and its doc comment with:

```rust
    /// v0.1.x single comp; read once and moved into `comps`, never written.
    #[serde(skip_serializing)]
    pub comp_input: String,
    /// Saved comps, newest first.
    pub comps: Vec<SavedComp>,
    /// `input` of the comp in use.
    pub active_comp: Option<String>,
```

Add `comps: Vec::new(), active_comp: None,` to `Default`. Replace `load` with:

```rust
    pub fn load(path: &Path) -> Settings {
        let mut s: Settings = std::fs::read_to_string(path).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        let legacy = std::mem::take(&mut s.comp_input);
        let legacy = legacy.trim();
        if !legacy.is_empty() && s.comps.is_empty() {
            s.comps.push(SavedComp { input: legacy.to_string(), name: String::new() });
            s.active_comp = Some(legacy.to_string());
        }
        s
    }
```

`driver.rs` still reads and writes `settings.comp_input`. Leave those uses for now: they still compile, and Task 3 replaces them. Because `comp_input` is no longer saved, any existing driver test that restarts the driver and expects the comp back will now fail. Known cases are `a_pasted_code_assigns_a_slot_and_survives_restart` and the v2 member test that checks `"persisted"`. Mark each one that fails `#[ignore = "re-enabled in Task 3"]` and list them in the report.

- [ ] **Step 4: Run the tests and check they pass.** Run `cargo test --workspace`. Expected: PASS (with the ignored test listed).

- [ ] **Step 5: Commit.**

```bash
git add crates/axigear-core/src/settings.rs crates/axigear-core/src/driver.rs
git commit -m "feat(settings): saved comp list with v0.1 comp_input migration

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 2: comp_library — one cache file for every saved comp

**Files:**
- Create: `crates/axigear-core/src/comp_library.rs`
- Modify: `crates/axigear-core/src/lib.rs` (add `pub mod comp_library;`)

**Interfaces:**
- Consumes: `session::LoadedComp` (Serialize/Deserialize/Clone/PartialEq), and `fsutil::write_atomic(path, &[u8]) -> io::Result<()>`.
- Produces:
  - `pub fn load(path: &Path) -> Vec<LoadedComp>`;
  - `pub fn save(path: &Path, comps: &[LoadedComp]) -> std::io::Result<()>`.

- [ ] **Step 1: Write the failing tests.** Create the file with the tests at the bottom. You need a `LoadedComp` value: build one from the fixture with `crate::loader::detect(&crate::testutil::fixture("comp-tuesday.txt"))`.

```rust
//! `comp_cache.json`: the cached copy of every saved comp, so switching
//! comps or restarting needs no network. v0.1.x wrote a single comp.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::session::LoadedComp;

#[derive(Serialize, Deserialize)]
struct File {
    comps: Vec<LoadedComp>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::loader::{detect, Input};
    use crate::session::CompOrigin;
    use crate::testutil::fixture;

    fn code_comp(input: &str) -> LoadedComp {
        let Ok(Input::Comp { comp, key }) = detect(&fixture("comp-tuesday.txt")) else { panic!("fixture") };
        LoadedComp { comp, key, input: input.into(), origin: CompOrigin::Code }
    }

    #[test]
    fn round_trips_several_comps() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("comp_cache.json");
        let comps = vec![code_comp("A"), code_comp("B")];
        save(&path, &comps).unwrap();
        assert_eq!(load(&path), comps);
        assert!(std::fs::read_to_string(&path).unwrap().starts_with("{\"comps\":"));
    }

    #[test]
    fn reads_the_v01_single_comp_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("comp_cache.json");
        std::fs::write(&path, serde_json::to_vec(&code_comp("OLD")).unwrap()).unwrap();
        assert_eq!(load(&path), vec![code_comp("OLD")]);
    }

    #[test]
    fn missing_or_broken_file_is_empty() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("comp_cache.json");
        assert!(load(&path).is_empty());
        std::fs::write(&path, "{nope").unwrap();
        assert!(load(&path).is_empty());
    }
}
```

- [ ] **Step 2: Run the tests and check they fail.** Run `cargo test -p axigear-core comp_library`. Expected: FAIL, because `load` and `save` are not defined.

- [ ] **Step 3: Implement.** Add above the tests:

```rust
/// Every cached comp. Accepts the v0.1.x single-comp file; anything
/// unreadable is treated as empty (comps then reload on use).
pub fn load(path: &Path) -> Vec<LoadedComp> {
    let Ok(text) = std::fs::read_to_string(path) else { return Vec::new() };
    if let Ok(f) = serde_json::from_str::<File>(&text) {
        return f.comps;
    }
    serde_json::from_str::<LoadedComp>(&text).map(|c| vec![c]).unwrap_or_default()
}

pub fn save(path: &Path, comps: &[LoadedComp]) -> std::io::Result<()> {
    let json = serde_json::to_vec(&File { comps: comps.to_vec() }).map_err(std::io::Error::other)?;
    crate::fsutil::write_atomic(path, &json)
}
```

Add `pub mod comp_library;` to `lib.rs`, in alphabetical position. If `loader::Input` or `session::CompOrigin` are not `pub`, check `lib.rs` and `loader.rs`: `driver.rs` already imports both, so they are reachable inside the crate.

- [ ] **Step 4: Run the tests and check they pass.** Run `cargo test --workspace`. Expected: PASS.

- [ ] **Step 5: Commit.**

```bash
git add crates/axigear-core/src/comp_library.rs crates/axigear-core/src/lib.rs
git commit -m "feat(core): comp_library cache file for all saved comps

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 3: Driver — library, Load/Use/Refresh/Unsubscribe per comp

**Files:**
- Modify: `crates/axigear-core/src/driver.rs`
- Modify (call sites only): `crates/axigear/src/ui/checklist.rs`, `crates/axigear/src/ui/settings.rs`

**Interfaces:**
- Consumes:
  - from Task 1: `SavedComp`, `Settings.comps` and `Settings.active_comp`;
  - from Task 2: `comp_library::{load, save}`.
- Produces:
  - `Command` variants:
    - `UseComp(String)` (new);
    - `RefreshComp(String)` (was `RefreshComp`);
    - `Unsubscribe(String)` (was `Unsubscribe`).
    The `String` is the saved `input`.
  - Driver behaviour that Task 4's snapshot reads: `self.library: Vec<LoadedComp>` and `self.refresh_errors: BTreeMap<String, String>`, the latter keyed by input and holding errors from refreshing a comp that isn't active.

- [ ] **Step 1: Write the failing tests.** Add them to `driver.rs` `mod tests`. The second code comp is `fixture("build-firebrand.txt")`, an `<AxiForge:` build code that loads as a one-build comp.

```rust
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
        d.handle(firebrand_in(1), t0);
        let pick = d.snapshot(t0).picker.iter().find(|p| p.enabled && !p.current).map(|p| p.slot);
        if let Some(slot) = pick {
            d.handle(Command::Pick(slot), t0);
        }
        let before = d.snapshot(t0).header.slot_label.clone();
        d.handle(Command::LoadInput(code_b()), t0);
        d.handle(Command::UseComp(code_a()), t0);
        assert_eq!(d.snapshot(t0).header.slot_label, before);
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
```

Update the existing tests:
- remove every `#[ignore = "re-enabled in Task 3"]` added in Task 1;
- `a_pasted_code_assigns_a_slot_and_survives_restart`:
  - replace `assert_eq!(d.settings().comp_input, fixture("comp-tuesday.txt"));` with `assert_eq!(d.settings().active_comp, Some(fixture("comp-tuesday.txt")));`.
- `unsubscribe_clears_everything`:
  - use `Command::Unsubscribe(fixture("comp-tuesday.txt"))`;
  - replace the `!exists()` assert with `assert!(crate::comp_library::load(&dir.path().join("comp_cache.json")).is_empty());`.
- The v2 test that calls `again.handle(Command::RefreshComp, t0)` uses `Command::RefreshComp(link())`.

`d.settings` is a private field. The tests module is a child module, so it can reach it. If the borrow checker complains, use `d.settings.picks.insert`, as written above.

- [ ] **Step 2: Run the tests and check they fail.** Run `cargo test -p axigear-core driver::`. Expected: compile errors, because `UseComp` doesn't exist and `RefreshComp`/`Unsubscribe` don't take arguments yet.

- [ ] **Step 3: Implement.**

Imports: add `use std::collections::BTreeMap;` if it is not present, `use crate::settings::SavedComp;`, and `use crate::comp_library;`.

`Command`:

```rust
pub enum Command {
    LoadInput(String),
    /// Switch to a saved comp (by `input`), from its cached copy.
    UseComp(String),
    /// Drop a saved comp (by `input`) and its cache and picks.
    Unsubscribe(String),
    Pick(SlotRef),
    /// Re-fetch or re-decode a saved comp (by `input`).
    RefreshComp(String),
    // ...rest unchanged
}
```

`Driver` fields: add

```rust
    /// Cached copy of each saved comp (same order not guaranteed).
    library: Vec<LoadedComp>,
    /// Last refresh error per saved comp that is not active.
    refresh_errors: BTreeMap<String, String>,
```

`Driver::new`: replace the `cached` block with the code below. Keep the `subscription` match, but match on `active` instead of `settings.comp_input`.

```rust
        let mut settings = Settings::load(&paths.config);
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
```

The original `let settings = Settings::load(...)` line moves up, and the `session.api.has_key` line must still come after it. Initialise `library` and `refresh_errors: BTreeMap::new()` in the struct literal.

`handle`, replacing the `Unsubscribe` and `RefreshComp` arms:

```rust
            Command::UseComp(input) => self.use_comp(input, now),
            Command::Unsubscribe(input) => self.unsubscribe(&input, now),
            Command::RefreshComp(input) => {
                if self.comp_manual.try_acquire(now) {
                    self.refresh(input, now);
                }
            }
```

Replace `commit` and `load_input`, and add the helpers:

```rust
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
```

In `poll_comp`, replace `let input = self.settings.comp_input.clone();` with `let input = self.settings.active_comp.clone().unwrap_or_default();`.

There must be no remaining `comp_input` use in `driver.rs`.

UI call sites. They are Windows-only, so they must compile under `cargo dll-check`:
- `checklist.rs` `header`: replace `send(Command::RefreshComp);` with:
  ```rust
  if let Some(active) = &snap.settings.active_comp { send(Command::RefreshComp(active.clone())); }
  ```
- `settings.rs`: in the Unsubscribe button, replace `send(Command::Unsubscribe);` with:
  ```rust
  if let Some(active) = &s.active_comp { send(Command::Unsubscribe(active.clone())); }
  ```
- `state.rs`: replace `self.comp_input = snap.settings.comp_input.clone();` with `self.comp_input.clear();`. (The input box is for pasting a new comp; saved comps are listed below it.)

Task 5 replaces these UI call sites properly.

- [ ] **Step 4: Run the tests and check they pass.** Run `cargo test --workspace`, then `cargo dll-check`. Expected: both PASS.

- [ ] **Step 5: Commit.**

```bash
git add crates/axigear-core/src/driver.rs crates/axigear/src/ui/checklist.rs crates/axigear/src/ui/settings.rs crates/axigear/src/ui/state.rs
git commit -m "feat(driver): saved comp library with use/refresh/unsubscribe per comp

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 4: Snapshot rows for saved comps

**Files:**
- Modify: `crates/axigear-core/src/driver.rs`

**Interfaces:**
- Consumes: from Task 3, `library`, `refresh_errors`, and `settings.comps`/`active_comp`.
- Produces:
  - `pub struct CompRow { pub input: String, pub name: String, pub source: String, pub active: bool, pub error: Option<String> }` (Debug, Clone, PartialEq, Eq);
  - `UiSnapshot.comps: Vec<CompRow>`, in `settings.comps` order.

- [ ] **Step 1: Write the failing test.**

```rust
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
```

- [ ] **Step 2: Run the tests and check they fail.** Run `cargo test -p axigear-core driver::tests::snapshot_lists`. Expected: compile error, because `comps` is not a field of `UiSnapshot`.

- [ ] **Step 3: Implement.** Add `CompRow` next to `Header`. Add `pub comps: Vec<CompRow>,` to `UiSnapshot`, after `header`. Pull the source wording out into a free function and use it in both places:

```rust
fn source_text(origin: &CompOrigin) -> String {
    match origin {
        CompOrigin::Code => "code".into(),
        CompOrigin::Link { fetched_at_unix, .. } => format!("link · fetched {}", text::ago(unix_now().saturating_sub(*fetched_at_unix))),
    }
}
```

In `snapshot`, set the header's `source` to `lc.map(|c| source_text(&c.origin)).unwrap_or_default()`, and build:

```rust
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
```

- [ ] **Step 4: Run the tests and check they pass.** Run `cargo test --workspace`. Expected: PASS.

- [ ] **Step 5: Commit.**

```bash
git add crates/axigear-core/src/driver.rs
git commit -m "feat(driver): saved comp rows in the UI snapshot

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 5: Options-tab comp list, self-saving API key, header comp dropdown

**Files:**
- Create: `crates/axigear/src/ui/key_edit.rs` (host-compilable, no `Ui`)
- Modify:
  - `crates/axigear/src/ui/mod.rs` (`pub mod key_edit;`, alongside `focus`, not cfg-gated);
  - `crates/axigear/src/ui/settings.rs`;
  - `crates/axigear/src/ui/checklist.rs`.

**Interfaces:**
- Consumes:
  - Task 3's `Command::{UseComp, RefreshComp, Unsubscribe}(String)`;
  - Task 4's `UiSnapshot.comps: Vec<CompRow>`.
- Produces:
  - `key_edit::commit_commands(edit: &str, saved: &str, test: bool) -> Vec<Command>`;
  - `key_edit::status(edit: &str, saved: &str, key_test: Option<&str>) -> Option<(String, bool)>`, where the bool is true for "ok-toned".

- [ ] **Step 1: Write the failing tests.** Create `key_edit.rs`:

```rust
//! API key field logic, host-testable: when an edit is saved, and what the
//! status line under the field says. The key itself is never formatted.

use axigear_core::driver::Command;

#[cfg(test)]
mod tests {
    use super::*;

    fn is_set(c: &Command, k: &str) -> bool { matches!(c, Command::SetApiKey(x) if x == k) }

    #[test]
    fn leaving_the_field_saves_only_a_changed_key() {
        assert!(commit_commands(" NEW ", "OLD", false).iter().any(|c| is_set(c, " NEW ")));
        assert!(commit_commands("OLD", "OLD", false).is_empty());
        assert!(commit_commands(" OLD ", "OLD", false).is_empty(), "trim-equal is unchanged");
    }

    #[test]
    fn test_saves_first_then_tests() {
        let cmds = commit_commands("NEW", "OLD", true);
        assert_eq!(cmds.len(), 2);
        assert!(is_set(&cmds[0], "NEW"));
        assert!(matches!(cmds[1], Command::TestKey));
        assert!(matches!(commit_commands("OLD", "OLD", true).as_slice(), [Command::TestKey]));
    }

    #[test]
    fn status_line() {
        assert_eq!(status("", "", None), None);
        assert_eq!(status("K", "K", None), Some(("Saved".into(), true)));
        assert_eq!(status("K2", "K", None), None, "unsaved edit: say nothing yet");
        assert_eq!(status("K", "K", Some("key ok (main)")), Some(("key ok (main)".into(), true)));
        assert_eq!(status("K", "K", Some("HTTP 401")), Some(("HTTP 401".into(), false)));
    }
}
```

- [ ] **Step 2: Run the tests and check they fail.** Run `cargo test -p arcdps_axigear key_edit`. Expected: FAIL, because the functions are not defined.

- [ ] **Step 3: Implement the helpers.** Put them above the tests:

```rust
/// Commands for the key field: save when the trimmed edit differs from the
/// saved key, then test when asked.
pub fn commit_commands(edit: &str, saved: &str, test: bool) -> Vec<Command> {
    let mut out = Vec::new();
    if edit.trim() != saved.trim() {
        out.push(Command::SetApiKey(edit.to_string()));
    }
    if test {
        out.push(Command::TestKey);
    }
    out
}

/// The line under the field: a test result wins; else "Saved" when the
/// field matches a non-empty saved key. `true` = ok tone.
pub fn status(edit: &str, saved: &str, key_test: Option<&str>) -> Option<(String, bool)> {
    if let Some(t) = key_test {
        return Some((t.to_string(), t.starts_with("key ok")));
    }
    (!saved.trim().is_empty() && edit.trim() == saved.trim()).then(|| ("Saved".to_string(), true))
}
```

Add `pub mod key_edit;` to `ui/mod.rs`, next to `pub mod focus;`.

- [ ] **Step 4: Run the tests and check they pass.** Run `cargo test --workspace`. Expected: PASS.

- [ ] **Step 5: Options tab.** In `settings.rs`, replace everything from `heading(ui, "COMP");` up to, but not including, `heading(ui, "SEVERITY");` with:

```rust
    heading(ui, "COMP");
    ui.input_text_multiline("##axigear-comp", &mut state.comp_input, [420.0, 60.0]).build();
    if ui.button("Load") {
        send(Command::LoadInput(state.comp_input.clone()));
        state.comp_input.clear();
    }
    if let Some(e) = &snap.load_error {
        ui.text_colored(theme::DANGER, e);
    }
    ui.text_colored(theme::TEXT_FAINT, "Paste an AxiForge comp or build code, or a published comp link.");
    if snap.comps.is_empty() {
        ui.text_colored(theme::TEXT_FAINT, "No saved comps.");
    }
    for row in &snap.comps {
        let _id = ui.push_id(row.input.as_str());
        ui.text_colored(if row.active { theme::GOLD } else { theme::TEXT }, &row.name);
        ui.same_line();
        ui.text_colored(theme::TEXT_FAINT, &row.source);
        if row.active {
            ui.same_line();
            ui.text_colored(theme::GOLD, "· in use");
        } else {
            ui.same_line();
            if ui.small_button("Use") {
                send(Command::UseComp(row.input.clone()));
            }
        }
        ui.same_line();
        if ui.small_button("Refresh") {
            send(Command::RefreshComp(row.input.clone()));
        }
        ui.same_line();
        if ui.small_button("Unsubscribe") {
            send(Command::Unsubscribe(row.input.clone()));
        }
        if let Some(e) = &row.error {
            ui.text_colored(theme::WARN, e);
        }
    }

    heading(ui, "GW2 API KEY");
    ui.set_next_item_width(320.0);
    ui.input_text("##axigear-key", &mut state.api_key).password(true).build();
    if ui.is_item_deactivated_after_edit() {
        for c in super::key_edit::commit_commands(&state.api_key, &s.api_key, false) {
            send(c);
        }
    }
    ui.same_line();
    if ui.button("Test##key") {
        for c in super::key_edit::commit_commands(&state.api_key, &s.api_key, true) {
            send(c);
        }
    }
    if let Some((line, ok)) = super::key_edit::status(&state.api_key, &s.api_key, snap.key_test.as_deref()) {
        ui.text_colored(if ok { theme::OK } else { theme::WARN }, line);
    }
    ui.text_colored(theme::TEXT_FAINT, "Saved when you leave the field. Needs the characters and builds permissions. Stored in plain text in addons/axigear/config.json.");
```

If `ui.push_id` doesn't accept `&str` in this imgui version, use `ui.push_id_ptr` or `ui.push_id_int` with the row index. Keep the token alive (`let _id =`) until the end of the loop body. If the returned token is `#[must_use]` and must be ended explicitly, call `_id.end()` / `.pop()` as that API requires.

- [ ] **Step 6: Header dropdown.** In `checklist.rs` `header`, replace the `match &h.comp_name { ... }` block with:

```rust
    if snap.comps.is_empty() {
        ui.text_colored(theme::TEXT_FAINT, "Comp: none");
    } else {
        ui.text("Comp:");
        ui.same_line();
        let names: Vec<&str> = snap.comps.iter().map(|r| r.name.as_str()).collect();
        let mut idx = snap.comps.iter().position(|r| r.active).unwrap_or(0);
        ui.set_next_item_width(220.0);
        if super::axi::combo(ui, "##axigear-comp-pick", &names, &mut idx, theme::GOLD) && !snap.comps[idx].active {
            send(Command::UseComp(snap.comps[idx].input.clone()));
        }
        if h.offline {
            ui.same_line();
            ui.text_colored(theme::WARN, "offline");
        }
        if h.source.starts_with("link") {
            ui.same_line();
            if ui.small_button("Refresh##comp") {
                if let Some(active) = &snap.settings.active_comp {
                    send(Command::RefreshComp(active.clone()));
                }
            }
        }
    }
```

`axi::combo` already draws the raised control and popup, so it satisfies the design contract. Check its accent parameter: if `theme::GOLD` isn't a sensible accent there, use whatever accent the other `axi::combo` call sites pass (grep `axi::combo(`).

- [ ] **Step 7: Verify.** Run `cargo test --workspace`, then `cargo dll-check`. Expected: both PASS, including `tests/axi_guard_test.rs`.

- [ ] **Step 8: Commit.**

```bash
git add crates/axigear/src/ui/key_edit.rs crates/axigear/src/ui/mod.rs crates/axigear/src/ui/settings.rs crates/axigear/src/ui/checklist.rs
git commit -m "feat(ui): saved comp list, comp dropdown, self-saving API key

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 6: Food and utility given as item IDs

**Files:**
- Modify:
  - `crates/axigear-core/src/model.rs` (helper);
  - `crates/axigear-core/src/gamedb.rs` (`wanted`);
  - `crates/axigear-core/src/checks/consumables.rs`.

**Interfaces:**
- Produces: `model::consumable_item_id(label: &str) -> Option<u32>`. It returns `Some(id)` when the trimmed label is all ASCII digits and parses to a non-zero `u32`. Task 8's loadout uses it.

- [ ] **Step 1: Write the failing tests.**

In `model.rs` tests:

```rust
    #[test]
    fn consumable_labels_that_are_item_ids() {
        assert_eq!(consumable_item_id("91835"), Some(91835));
        assert_eq!(consumable_item_id(" 91835 "), Some(91835));
        assert_eq!(consumable_item_id("0"), None);
        assert_eq!(consumable_item_id("Plate of Beef Rendang"), None);
        assert_eq!(consumable_item_id("12a"), None);
        assert_eq!(consumable_item_id(""), None);
    }
```

In `gamedb.rs` tests:

```rust
    #[test]
    fn numeric_food_and_utility_are_wanted() {
        let mut b = crate::testutil::firebrand();
        b.equipment.food = Some("91835".into());
        b.equipment.utility = Some("Superior Sharpening Stone".into());
        let w = GameDb::default().wanted(Some(&b), None, &BTreeSet::new());
        assert!(w.items.contains(&91835));
    }
```

In `checks/consumables.rs` tests (`ItemInfo` is in `crate::gamedb`):

```rust
    #[test]
    fn numeric_food_passes_once_its_name_is_known() {
        let label = firebrand().equipment.food.clone().unwrap();
        let mut w = World::matching(firebrand());
        w.build.equipment.food = Some("91835".into());
        assert_eq!(w.result("food").status, Status::Unknown);
        assert_eq!(w.result("food").reason.as_deref(), Some("looking up item name"));
        w.db.items.insert(91835, crate::gamedb::ItemInfo { name: label, ..Default::default() });
        assert_eq!(w.result("food").status, Status::Pass);
    }

    #[test]
    fn numeric_food_matches_mists_infused_variant() {
        let mut w = World::matching(firebrand());
        w.build.equipment.food = Some("91835".into());
        w.db.items.insert(91835, crate::gamedb::ItemInfo { name: "Peppercorn-Crusted Sous-Vide Steak".into(), ..Default::default() });
        w.live.active.clear();
        w.live.active.insert(Consumables::bundled().find("Mists-Infused Peppercorn-Crusted Sous-Vide Steak").unwrap());
        assert_eq!(w.result("food").status, Status::Pass);
    }
```

`CheckResult` stores its reason in a field. If it is not named `reason`, check `report.rs`'s `with_reason` and use that field name. The expected-text column (`want`) shows the resolved name once it is known.

- [ ] **Step 2: Run the tests and check they fail.** Run `cargo test -p axigear-core consumable`. Expected: FAIL. The helper is missing, and the numeric label is compared as a name.

- [ ] **Step 3: Implement.**

In `model.rs` (free function, near `is_two_handed`):

```rust
/// AxiForge links can give food/utility as an item ID ("91835") instead of a name.
pub fn consumable_item_id(label: &str) -> Option<u32> {
    let t = label.trim();
    (!t.is_empty() && t.bytes().all(|b| b.is_ascii_digit())).then(|| t.parse::<u32>().ok()).flatten().filter(|v| *v != 0)
}
```

In `gamedb.rs` `wanted`, inside `if let Some(b) = build`, after the infusions line:

```rust
            w.items.extend([&e.food, &e.utility].into_iter().flatten().filter_map(|l| crate::model::consumable_item_id(l)));
```

In `checks/consumables.rs` `check`, right after `let Some(want) = want else { return Vec::new() };`:

```rust
    let resolved;
    let want = match crate::model::consumable_item_id(want) {
        None => want,
        Some(id) => match ctx.db.item_name(id) {
            Some(name) => {
                resolved = name.to_string();
                resolved.as_str()
            }
            None => {
                let (cat, id_s, label) = match kind {
                    ConsumableKind::Food => (Category::Food, "food", "Food"),
                    ConsumableKind::Utility => (Category::Utility, "utility", "Utility"),
                };
                return vec![CheckResult::new(cat, id_s, label, Status::Unknown, format!("Item {id}")).with_reason("looking up item name")];
            }
        },
    };
```

If the borrow checker rejects the deferred `resolved` binding, make `want` a `String` (`let want: String = ...`) and pass `&want` from there on.

- [ ] **Step 4: Run the tests and check they pass.** Run `cargo test --workspace`. Expected: PASS.

- [ ] **Step 5: Commit.**

```bash
git add crates/axigear-core/src/model.rs crates/axigear-core/src/gamedb.rs crates/axigear-core/src/checks/consumables.rs
git commit -m "fix(checks): resolve food/utility item IDs to names before matching

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 7: Per-slot infusions and per-ID infusion marks

**Files:**
- Modify:
  - `crates/axigear-core/src/model.rs` (`Equipment.infusions_by_slot`);
  - `crates/axigear-core/src/report.rs` (`SlotKey::Infusion(u32)`);
  - `crates/axigear-core/src/checks/gear.rs` (`infusions`).

**Interfaces:**
- Produces:
  - `Equipment.infusions_by_slot: BTreeMap<GearSlot, Vec<u32>>`: per slot, in AxiForge order, with capacity applied, and only slots with at least one infusion;
  - `SlotKey::Infusion(u32)`, an item ID; on a failing infusion check it gets a Fail mark for each under-worn ID.

- [ ] **Step 1: Write the failing tests.**

In `model.rs` tests, next to `infusions_respect_real_slot_counts`:

```rust
    #[test]
    fn infusions_are_kept_per_slot() {
        let b = crate::testutil::firebrand();
        let e = &b.equipment;
        let total: usize = e.infusions_by_slot.values().map(Vec::len).sum();
        assert_eq!(total, e.infusions.len());
        let mut flat: Vec<u32> = e.infusions_by_slot.values().flatten().copied().collect();
        flat.sort_unstable();
        assert_eq!(flat, e.infusions);
        assert!(e.infusions_by_slot.get(&GearSlot::Amulet).is_none());
        assert!(e.infusions_by_slot.values().all(|v| !v.is_empty()));
    }
```

In `checks/gear.rs` tests:

```rust
    #[test]
    fn a_short_infusion_is_marked_by_id() {
        let mut w = World::matching(firebrand());
        let want = firebrand().equipment.infusions.clone();
        let a = want[0];
        let b = 49_999u32;
        w.db.items.insert(b, crate::gamedb::ItemInfo { name: "Other Infusion".into(), ..Default::default() });
        let head = w.item_mut(GearSlot::Head);
        let pos = head.infusions.iter().position(|x| *x == a).unwrap();
        head.infusions[pos] = b;
        let r = w.result("infusions");
        assert_eq!(r.status, Status::Fail);
        assert_eq!(r.marks[0].key, SlotKey::Infusions, "row mark stays first for problem clicks");
        assert!(r.marks.iter().any(|m| m.key == SlotKey::Infusion(a) && m.status == Status::Fail));
        assert!(!r.marks.iter().any(|m| m.key == SlotKey::Infusion(b)));
    }
```

`World::matching` puts every wanted infusion on the Head item, so `item_mut(Head)` holds them. If `item_mut` has a different signature, check `testutil.rs`.

- [ ] **Step 2: Run the tests and check they fail.** Run `cargo test -p axigear-core infusion`. Expected: compile errors, because `infusions_by_slot` and `SlotKey::Infusion` don't exist.

- [ ] **Step 3: Implement.**

`report.rs`: add the variant after `Infusions`:

```rust
    Infusions,
    /// One wanted infusion item (Equipment tab chips); marked only when short.
    Infusion(u32),
```

Fix any exhaustive `match` on `SlotKey` that the compiler reports in core or UI. Map `Infusion(_)` the same way as `Infusions`.

`model.rs` `Equipment`: add the field after `infusions`, with its doc comment:

```rust
    /// Land infusion IDs per slot (capacity applied); slots without any are absent.
    pub infusions_by_slot: BTreeMap<GearSlot, Vec<u32>>,
```

In `from_raw`, change the loop body to:

```rust
            if let Some(v) = raw.infusions.get(slot.axiforge_key()) {
                let ids: Vec<u32> = v.as_slice().iter().take(capacity).filter_map(|s| id(s)).collect();
                if !ids.is_empty() {
                    infusions.extend(ids.iter().copied());
                    infusions_by_slot.insert(slot, ids);
                }
            }
```

Declare `let mut infusions_by_slot = BTreeMap::new();` before the loop, and add `infusions_by_slot,` to the struct literal. Fix any other `Equipment { .. }` literals the compiler reports (tests in `model.rs`, `axicode`) by adding `infusions_by_slot: Default::default()`.

`checks/gear.rs` `infusions`: replace the final `None =>` arm with:

```rust
        None => {
            let mut r = row(Status::Fail).mark(SlotKey::Infusions, Status::Fail, Some(actual.clone()));
            let mut seen = Vec::new();
            for id in want {
                let k = key(id);
                if seen.contains(&k) {
                    continue;
                }
                let (w, h) = (want_keys.iter().filter(|x| **x == k).count(), have_keys.iter().filter(|x| **x == k).count());
                if h < w {
                    r = r.mark(SlotKey::Infusion(*id), Status::Fail, Some(format!("wearing {h} of {w}")));
                }
                seen.push(k);
            }
            vec![r]
        }
```

- [ ] **Step 4: Run the tests and check they pass.** Run `cargo test --workspace`, then `cargo dll-check` (the UI may match on `SlotKey`). Expected: both PASS.

- [ ] **Step 5: Commit.**

```bash
git add crates/axigear-core/src crates/axigear/src
git commit -m "feat(core): per-slot infusions and per-ID infusion marks

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 8: Loadout rows and the Equipment tab layout

**Files:**
- Modify:
  - `crates/axigear-core/src/loadout.rs`;
  - `crates/axigear/src/ui/equipment_tab.rs` (rewrite of `render`, `gear_row` and `trinket_row`).

**Interfaces:**
- Consumes:
  - from Task 6: `model::consumable_item_id`;
  - from Task 7: `Equipment.infusions_by_slot` and `SlotKey::Infusion(u32)`.
- Produces:
  - `GearRow { tile, upgrades, infusions: Vec<Tile> }`, where the infusion tiles use keys `SlotKey::Infusion(id)`;
  - `Loadout.trinkets: Vec<GearRow>` (was `Vec<Tile>`);
  - `Loadout.infusions` removed.

- [ ] **Step 1: Write the failing tests.** Add them to `loadout.rs` tests. In `db_with_icons`, the existing loop over `b.equipment.infusions` still covers every infusion ID.

```rust
    #[test]
    fn rows_carry_their_own_infusions() {
        let b = firebrand();
        let l = Loadout::of(&b, &db_with_icons(&b), SpecDb::bundled());
        let all_rows = l.armor.iter().chain(l.trinkets.iter()).chain(l.weapons.iter().flat_map(|s| std::iter::once(&s.main).chain(s.off.iter())));
        let mut ids: Vec<u32> = all_rows.flat_map(|r| r.infusions.iter()).map(|t| match t.key { SlotKey::Infusion(id) => id, k => panic!("{k:?}") }).collect();
        ids.sort_unstable();
        assert_eq!(ids, b.equipment.infusions);
        assert!(l.trinkets.iter().find(|r| r.tile.key == SlotKey::Gear(GearSlot::Amulet)).unwrap().infusions.is_empty());
    }

    #[test]
    fn trinkets_are_rows_in_slot_order() {
        let b = firebrand();
        let l = Loadout::of(&b, &GameDb::default(), SpecDb::bundled());
        let keys: Vec<SlotKey> = l.trinkets.iter().map(|r| r.tile.key).collect();
        assert_eq!(keys, [GearSlot::Back, GearSlot::Accessory1, GearSlot::Accessory2, GearSlot::Amulet, GearSlot::Ring1, GearSlot::Ring2].map(SlotKey::Gear));
    }

    #[test]
    fn empty_set_b_has_no_rows() {
        let mut b = firebrand();
        b.equipment.weapons.b1 = None;
        b.equipment.weapons.b2 = None;
        let l = Loadout::of(&b, &GameDb::default(), SpecDb::bundled());
        assert!(l.weapons.iter().all(|s| s.label != "B"));
    }

    #[test]
    fn numeric_food_tile_shows_the_item_name() {
        let mut b = firebrand();
        b.equipment.food = Some("91835".into());
        let mut db = GameDb::default();
        let l = Loadout::of(&b, &db, SpecDb::bundled());
        assert_eq!(l.food.name, "Item 91835");
        db.items.insert(91835, ItemInfo { name: "Plate of Beef Rendang".into(), icon: Some("https://render.guildwars2.com/i/91835.png".into()), icon_checked: true, ..Default::default() });
        let l = Loadout::of(&b, &db, SpecDb::bundled());
        assert_eq!(l.food.name, "Plate of Beef Rendang");
        assert!(l.food.icon.is_some());
    }
```

Remove or update any existing loadout test that reads `l.infusions` or treats `l.trinkets[i]` as a `Tile`: use `l.trinkets[i].tile`.

- [ ] **Step 2: Run the tests and check they fail.** Run `cargo test -p axigear-core loadout`. Expected: compile errors, because `infusions` is not a field of `GearRow`.

- [ ] **Step 3: Implement the loadout.**
  - Change `GearRow` to `pub struct GearRow { pub tile: Tile, pub upgrades: Vec<Tile>, pub infusions: Vec<Tile> }`.
  - Change the `Loadout` fields to `pub trinkets: Vec<GearRow>, // Back, Acc1, Acc2, Amulet, Ring1, Ring2`, and delete the `infusions` field.
  - In `Loadout::of`, define this before `armor`:

```rust
        let infs = |slot: GearSlot| -> Vec<Tile> {
            e.infusions_by_slot.get(&slot).map(|v| v.iter().map(|id| item(SlotKey::Infusion(*id), "Infusion", *id)).collect()).unwrap_or_default()
        };
```

  - Then:
    - **Armor rows:** `GearRow { tile: gear(slot), upgrades: ..., infusions: infs(slot) }`.
    - **`weapon_row`:** add `infusions: infs(slot)`. For the empty main row, add `infusions: vec![]`.
    - **Trinkets:** `[...].map(|slot| GearRow { tile: gear(slot), upgrades: vec![], infusions: infs(slot) }).to_vec()`.
    - Delete the `let infusions = ...` line and the `infusions,` struct field.
    - **Unknown items:** the `item` closure's fallback name becomes `format!("Item {id}")`, to match the spec. Update any test that asserted `"item {id}"`.
    - **Food and utility:** replace `named_tile` for these two with:

```rust
        let consumable = |key: SlotKey, label: &str, kind: NamedKind, want: &Option<String>| {
            let name: Option<String> = want.as_ref().map(|w| match crate::model::consumable_item_id(w) {
                Some(id) => db.item_name(id).map(String::from).unwrap_or_else(|| format!("Item {id}")),
                None => w.clone(),
            });
            let hit = name.as_deref().and_then(|n| named(kind, n));
            let icon = hit.map(|h| h.icon.clone()).or_else(|| want.as_deref().and_then(crate::model::consumable_item_id).and_then(|id| db.item_icon(id)).map(String::from));
            let mut t = tile(key, label, name, icon);
            t.sub = hit.map(|h| h.buff.clone()).filter(|b| !b.is_empty());
            t
        };
```

    and use `food: consumable(SlotKey::Food, "Food", NamedKind::Food, &e.food), utility: consumable(SlotKey::Utility, "Utility", NamedKind::Utility, &e.utility),`. The relic keeps `named_tile`.

- [ ] **Step 4: Run the core tests.** Run `cargo test -p axigear-core`. Expected: PASS.

- [ ] **Step 5: Rewrite `equipment_tab.rs`.** Replace the whole file with:

```rust
//! Equipment tab: fixed-width columns. Left: armor and weapon sets; right:
//! trinkets, relic, consumables. Each row: icon, name, upgrade names, and the
//! slot's infusion chips at the right edge.

use arcdps::imgui::Ui;
use axigear_core::loadout::{GearRow, Loadout, Tile};
use axigear_core::report::{CheckReport, SlotKey};

use super::axi;
use super::tile::{self, TileStyle};
use super::theme;

const COL_W: f32 = 340.0;
const GAP: f32 = 12.0;
const ROW_ICON: f32 = 40.0;
const TRINKET: f32 = 32.0;
const UPGRADE: f32 = 16.0;
const INF: f32 = 16.0;

pub fn render(ui: &Ui, l: &Loadout, report: &CheckReport, focus: Option<SlotKey>) {
    let o = ui.cursor_screen_pos();
    let avail = ui.content_region_avail()[0];
    let side_by_side = avail >= 2.0 * COL_W + GAP;
    let left_bottom = column(ui, o, |ui| {
        section(ui, "ARMOR");
        for row in &l.armor {
            gear_row(ui, row, report, focus, None);
        }
        for set in &l.weapons {
            section(ui, &format!("WEAPONS · SET {}", set.label));
            gear_row(ui, &set.main, report, focus, set.two_handed.then_some("two-handed"));
            if let Some(off) = &set.off {
                gear_row(ui, off, report, focus, None);
            }
        }
    });
    let right_at = if side_by_side { [o[0] + COL_W + GAP, o[1]] } else { [o[0], left_bottom + 8.0] };
    let right_bottom = column(ui, right_at, |ui| {
        section(ui, "TRINKETS");
        for row in &l.trinkets {
            small_row(ui, row, report, focus);
        }
        small_row(ui, &plain(&l.relic), report, focus);
        section(ui, "CONSUMABLES");
        for t in [&l.food, &l.utility] {
            small_row(ui, &plain(t), report, focus);
        }
    });
    ui.set_cursor_screen_pos([o[0], left_bottom.max(right_bottom)]);
    ui.dummy([avail.min(2.0 * COL_W + GAP), 0.0]);
}

fn plain(t: &Tile) -> GearRow {
    GearRow { tile: t.clone(), upgrades: vec![], infusions: vec![] }
}

/// Draws `f` with the cursor at `at`, inside a group; returns the bottom y.
fn column(ui: &Ui, at: [f32; 2], f: impl FnOnce(&Ui)) -> f32 {
    ui.set_cursor_screen_pos(at);
    let g = ui.begin_group();
    f(ui);
    g.end();
    ui.item_rect_max()[1]
}

/// Eyebrow label with a hairline rule under it, one column wide.
fn section(ui: &Ui, s: &str) {
    ui.dummy([0.0, 6.0]);
    axi::label(ui, s);
    let y = ui.item_rect_max()[1] + 2.0;
    let x = ui.item_rect_min()[0];
    axi::rule(ui, [x, y], [x + COL_W, y]);
    ui.dummy([0.0, 4.0]);
}

fn pulse(focus: Option<SlotKey>, key: SlotKey) -> bool {
    focus == Some(key) || (matches!(key, SlotKey::Infusion(_)) && focus == Some(SlotKey::Infusions))
}

/// Infusion chips right-aligned at the end of the row, vertically centred on `mid`.
fn infusion_chips(ui: &Ui, row: &GearRow, o: [f32; 2], mid: f32, report: &CheckReport, focus: Option<SlotKey>) -> f32 {
    let w = row.infusions.len() as f32 * (INF + 2.0);
    for (i, t) in row.infusions.iter().enumerate() {
        ui.set_cursor_screen_pos([o[0] + COL_W - w + i as f32 * (INF + 2.0), mid - INF / 2.0]);
        tile::icon(ui, t, Some(report), TileStyle { pulse: pulse(focus, t.key), ..TileStyle::new(INF) });
    }
    w
}

/// Armor or weapon: 40 px icon, name (+ tag), one line per rune/sigil with its icon.
fn gear_row(ui: &Ui, row: &GearRow, report: &CheckReport, focus: Option<SlotKey>, tag: Option<&str>) {
    let o = ui.cursor_screen_pos();
    let line = ui.text_line_height();
    let lines = 1 + row.upgrades.len().max(row.tile.sub.is_some() as usize);
    let h = ROW_ICON.max(lines as f32 * (line + 2.0));
    ui.set_cursor_screen_pos([o[0], o[1] + (h - ROW_ICON) / 2.0]);
    tile::icon(ui, &row.tile, Some(report), TileStyle { pulse: pulse(focus, row.tile.key), ..TileStyle::new(ROW_ICON) });
    let chips_w = infusion_chips(ui, row, o, o[1] + h / 2.0, report, focus);
    let text_x = o[0] + ROW_ICON + 8.0;
    let text_w = (COL_W - ROW_ICON - 8.0 - chips_w - 6.0).max(40.0);
    let top = o[1] + (h - lines as f32 * (line + 2.0)) / 2.0;
    ui.set_cursor_screen_pos([text_x, top]);
    let name = if row.tile.empty { "-".to_string() } else { match tag { Some(t) => format!("{} · {t}", row.tile.name), None => row.tile.name.clone() } };
    tile::clipped_text(ui, &name, text_w, if row.tile.empty { theme::TEXT_FAINT } else { theme::TEXT });
    for (i, u) in row.upgrades.iter().enumerate() {
        let y = top + (i + 1) as f32 * (line + 2.0);
        ui.set_cursor_screen_pos([text_x, y + (line - UPGRADE) / 2.0]);
        tile::icon(ui, u, Some(report), TileStyle { pulse: pulse(focus, u.key), ..TileStyle::new(UPGRADE) });
        ui.set_cursor_screen_pos([text_x + UPGRADE + 4.0, y]);
        tile::clipped_text(ui, &u.name, text_w - UPGRADE - 4.0, theme::TEXT_DIM);
    }
    if row.upgrades.is_empty() {
        if let Some(sub) = &row.tile.sub {
            ui.set_cursor_screen_pos([text_x, top + line + 2.0]);
            tile::clipped_text(ui, sub, text_w, theme::TEXT_FAINT);
        }
    }
    ui.set_cursor_screen_pos([o[0], o[1] + h + 4.0]);
    ui.dummy([0.0, 0.0]);
}

/// Trinket, relic, food or utility: 32 px icon centred on two lines (name, slot or buff).
fn small_row(ui: &Ui, row: &GearRow, report: &CheckReport, focus: Option<SlotKey>) {
    let o = ui.cursor_screen_pos();
    let line = ui.text_line_height();
    let h = TRINKET.max(2.0 * line + 2.0);
    ui.set_cursor_screen_pos([o[0], o[1] + (h - TRINKET) / 2.0]);
    tile::icon(ui, &row.tile, Some(report), TileStyle { pulse: pulse(focus, row.tile.key), ..TileStyle::new(TRINKET) });
    let chips_w = infusion_chips(ui, row, o, o[1] + h / 2.0, report, focus);
    let text_x = o[0] + TRINKET + 8.0;
    let text_w = (COL_W - TRINKET - 8.0 - chips_w - 6.0).max(40.0);
    let top = o[1] + (h - (2.0 * line + 2.0)) / 2.0;
    ui.set_cursor_screen_pos([text_x, top]);
    tile::clipped_text(ui, if row.tile.empty { "-" } else { &row.tile.name }, text_w, if row.tile.empty { theme::TEXT_FAINT } else { theme::TEXT });
    ui.set_cursor_screen_pos([text_x, top + line + 2.0]);
    let sub = row.tile.sub.clone().unwrap_or_else(|| row.tile.label.clone());
    tile::clipped_text(ui, &sub, text_w, theme::TEXT_DIM);
    ui.set_cursor_screen_pos([o[0], o[1] + h + 4.0]);
    ui.dummy([0.0, 0.0]);
}
```

Notes for the implementer:
- **Sigils:** the spec allows two sigils on one line when they fit. This plan rules on one line per sigil, which is simpler, never overflows, and is covered by the spec's "otherwise one per line". Record that ruling in the report.
- **`axi::label` and `axi::rule`** each take the window draw list. `tile::icon` also takes one, but releases it before returning, so no two lists are live at once. Don't hold a draw list across these calls.
- **Trinket sub-lines** show the slot label ("Back", "Acc 1", "Ring 1"). That label comes from `short_label`.
- **Clipped text:** `tile::clipped_text` adds the "…" via `axi::truncate_to_width`.

- [ ] **Step 6: Verify.** Run `cargo test --workspace`, then `cargo dll-check`. Expected: both PASS, including `axi_guard_test`.

- [ ] **Step 7: Commit.**

```bash
git add crates/axigear-core/src/loadout.rs crates/axigear/src/ui/equipment_tab.rs
git commit -m "feat(ui): equipment tab rows with upgrade names and per-slot infusions

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 9: Build and deploy for in-game verification

**Files:** none changed, unless the build surfaces fixes.

- [ ] **Step 1:** Run `cargo test --workspace`. Expected: PASS.
- [ ] **Step 2:** Run `cargo dll`. Expected: the build finishes, producing `arcdps_axigear.dll`.
- [ ] **Step 3:** Run `scripts/deploy.sh`. Expected: the DLL is copied into the GW2 `addons` / arcdps folder.
- [ ] **Step 4:** Read `config.json` without exposing the key, by masking it:

```bash
sed -E 's/("api_key": *")[^"]*"/\1***"/' "<addons>/axigear/config.json"
```

Find `<addons>` from `scripts/deploy.sh`. Confirm that `comp_input` is gone once the game has saved, and that `comps`/`active_comp` are present.
- [ ] **Step 5:** Hand over the in-game checklist below to the user. Do not tag a release.
  - The existing comp is listed in the options tab and is active.
  - Loading a second comp adds a row. Use, Refresh and Unsubscribe each work.
  - The main-window Comp: dropdown switches comps.
  - The API key, typed and then clicked away from, shows "Saved" and survives a restart.
  - The Equipment tab:
    - rune and sigil names under each item;
    - trinket icons centred on their two lines;
    - infusion chips on their own slots;
    - food and utility show names, not numbers;
    - no empty Set B.
