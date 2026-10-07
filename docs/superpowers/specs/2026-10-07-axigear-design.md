# axigear — Design Spec

**Date:** 2026-10-07
**Status:** Draft, awaiting review

## 1. Goal

axigear is an arcdps plugin that checks, live and in game, whether a player's build, equipment and consumables match their assigned slot in an AxiForge comp (or a single AxiForge build).

### v1 scope

- **Audience:** each squad member checks themselves against their slot. Nothing is sent anywhere.
- **Comp input:** a pasted AxiCode (`<AxiForge:Comp:…>` or a build code, works offline) **or** a subscription to a published AxiForge comp link, fetched and re-polled.
- **Slot assignment:** auto-match by spec, with a manual pick from the comp's party lines as fallback, remembered per comp.
- **Checks:** live arcdps/MumbleLink signals (spec, weapon set, food/utility buffs, skills cast) plus GW2 API gear/build checks using a player-supplied API key (`characters` + `builds` scopes).
- **UX:** a small always-visible badge (hidden in combat by default) that opens a full checklist window.
- **No game memory reads.**

### Out of scope (follow-ups)

- **v2 commander reporting:** squad members send their `CheckReport` to the commander. v1 keeps `CheckReport` serializable so this is purely additive.
- **axiam API key handoff:** axiam passes the account's API key when launching the game. Transport (env var vs. a file next to the DLL) to be decided in that follow-up.
- Toast alerts, ascended/exotic rarity checks, a Nexus addon build.

## 2. Architecture

A new repo `arcdps-axigear`, a Cargo workspace with two crates, using the same stack as `arcdps-axipulse` (vendored patched arcdps bindings, ImGui, `ureq` with rustls, serde, mimalloc, axilog-api, updater, `release.yml`).

```
arcdps-axigear/
  crates/
    axigear-core/        # pure Rust, no Windows/game deps, cargo test on Linux
      model.rs           # Build, Comp, PartyLine, Slot, Equipment…
      axicode.rs         # decoder for <AxiForge:Comp:…> and build codes
      link.rs            # port of axiforge parseAxiLink: ?c=, ?b=, #id.key, /r/<id>/
      publish.rs         # .enc fetch + AES-256-GCM decrypt + schemaVersion JSON parse
      matcher.rs         # spec → slot, manual fallback
      itemdb.rs          # item/itemstat ID ↔ name resolution, disk cache
      checks/            # rules engine → CheckReport
    axigear/             # cdylib, the arcdps plugin
      plugin.rs          # arcdps exports (init, release, combat, imgui, options)
      signals.rs         # combat events → ObservedLive
      mumble.rs          # MumbleLink reader (identity, map, uiState)
      worker.rs          # background thread: comp polling, GW2 API, item lookups
      gw2api.rs          # characters / buildtabs / tokeninfo client
      config.rs, updater.rs, hotkey.rs
      ui/ badge.rs, checklist.rs, settings.rs
  vendor/arcdps/         # same patched bindings as axipulse
  fixtures/              # real AxiCodes + .enc comps exported from axiforge
```

### Threading

- **arcdps callbacks** (combat, imgui) never block. They update `ObservedLive` and read the latest `Arc<CheckReport>`. Locks are taken with `try_lock`; a contended frame is skipped.
- **One worker thread** owns all I/O: HTTP, decryption, API polling, item lookups. Communication via channels.
- **The check engine** runs on the worker whenever an input changes (comp, slot, API snapshot, live signal) and publishes a new `Arc<CheckReport>`.

### Core types

```rust
struct CheckResult {
    id: CheckId,            // e.g. Sigil { set: A, hand: Main, index: 0 }
    category: Category,     // Spec, Specializations, Traits, Skills, SkillsSeen,
                            // Weapons, Stats, Runes, Sigils, Relic, Infusions, Food, Utility
    severity: Severity,     // Required | Advisory (from config)
    status: Status,         // Pass | Fail | Unknown
    expected: String,
    actual: Option<String>,
    reason: Option<String>, // why Unknown, e.g. "needs API key"
    source: Source,         // Live | Api
    observed_at: Option<SystemTime>,
}

struct CheckReport {
    comp_key: String,
    slot: SlotRef,
    results: Vec<CheckResult>,
    // derived: pass / fail_required / fail_advisory / unknown counts
}
```

Display mapping: `Fail` + `Required` → ✗, `Fail` + `Advisory` → ⚠, `Unknown` → greyed `?`. Unknowns never count as pass or fail.

## 3. Data flow

### 3.1 Loading a comp

```
paste box ──► detect input type
              ├─ <AxiForge:Comp:…>  → axicode::decode_comp      (offline)
              ├─ build code         → treated as a one-slot comp (offline)
              └─ URL                → link::parse
                                       ├─ /r/<id>/  → resolve redirect → ?c=<id>.<key>
                                       └─ ?c=<id>.<key>
                                          → GET https://raw.githubusercontent.com/<owner>/axibuilds/main/site/comps/<id>.enc
                                          → base64 → iv(12) | ciphertext | tag(16)
                                          → AES-256-GCM decrypt with base64url key (32 bytes)
                                          → publish::parse (by schemaVersion)
```

- **Schema versions:** missing `schemaVersion` = v1 (today's `serializeCompForPublish` output). A version newer than supported → "Comp made with newer AxiForge — update axigear"; keep the last good comp.
- **Subscription polling:** re-fetch on map change (MumbleLink `mapId` change) and at most every 10 minutes otherwise, sending `If-None-Match` with the last ETag. On change: re-match slot, re-run checks.
- **Offline cache:** last decoded comp persisted to `comp_cache.json`.
- AxiForge reuses `publishedFileId`/`publishedKey` on republish, so a subscribed URL stays valid.

### 3.2 Slot matching

- **Identity:** self agent from arcdps (profession, elite spec), cross-checked with MumbleLink identity JSON (`name`, `profession`, `spec`).
- **Filter** comp builds by elite spec (profession for core builds):
  - exactly one → auto-assigned;
  - none → warning "no slot in this comp matches your spec" + manual picker;
  - several → manual picker.
- **Remembered picks** keyed by `(comp key, character name)`. Comp key = fileId for links, content hash for codes.
- Re-match on spec change (character swap, re-spec).

### 3.3 Live signals (arcdps events + MumbleLink only)

| Signal | Source | Used for |
|---|---|---|
| Profession / elite spec | self agent, MumbleLink | Spec check |
| Food / utility buffs | `buffapply` / buffinitial statechange on self | Food, Utility checks |
| Skills cast | `cbtevent.skillid` with self as source | Skills seen; weapon-skill evidence for Weapons |
| Weapon set | weapon-swap statechange | which set is active |
| In combat | MumbleLink `uiState` / arcdps enter/exit combat | badge hiding, pausing API polls |

arcdps does not expose equipped weapon types; Weapons is an API check, with cast weapon skills as supporting evidence.

### 3.4 GW2 API

- Character name from MumbleLink identity.
- Endpoints: `/v2/characters/<name>/equipment` (active equipment tab), `/v2/characters/<name>/buildtabs?tab=active`, `/v2/tokeninfo` (settings test).
- **Polling:** on map change, and every 5 minutes while out of combat. Never in combat. Exponential backoff on 429/5xx.
- **itemdb:** batch-resolve item and itemstat IDs via `/v2/items?ids=…` and `/v2/itemstats?ids=…`; cache in `itemdb.json` without expiry.
- **Unverified assumption:** the API refreshes character data on map change or after a few minutes. Verify during implementation and adjust polling.

### 3.5 Check engine

```
Expected (Build from matched slot)
  + ObservedLive
  + ApiSnapshot { data, fetched_at }
  + Config (severities)
      → checks::run() → CheckReport
```

- Every API result carries its snapshot age.
- **Staleness:** if live data contradicts the snapshot (e.g. spec changed after it was taken), API results become Unknown "stale — waiting for API refresh" instead of false ✗.

## 4. Check catalog

General rules:

- Fields the build leaves unspecified produce **no** check (not a pass).
- Each category's severity is configurable: Required / Advisory / Off.
- Compare canonical IDs (spec, trait, skill, item) where AxiForge stores them; where it stores names (stats, food, relic), compare normalized names (case/punctuation-insensitive) via itemdb.

| # | Check | Source | Compares | Default |
|---|---|---|---|---|
| 1 | Spec | Live | profession + elite spec vs slot build | Required |
| 2 | Specializations | API | 3 spec lines as a set; elite line must be in slot 3 | Required |
| 3 | Traits | API | 3 major choices per line; one result per line | Required |
| 4 | Skill bar | API | heal and elite exact; 3 utilities as a set (order ignored) | Required |
| 5 | Skills seen in use | Live | each expected utility/elite cast at least once this map. Pass when seen, otherwise Unknown; never Fail | Advisory |
| 6 | Weapons | API (+Live evidence) | weapon types per set (A1/A2, B1/B2; two-handers fill both); skip sets the build doesn't define | Required |
| 7 | Stats | API | itemstat name per armor/trinket/weapon slot vs build's stat package (global or per-slot) | Required |
| 8 | Runes | API | 6 armor upgrades vs expected rune; one result ("6/6 Scholar" or wrong slots listed) | Required |
| 9 | Sigils | API | each weapon's sigils vs build's sigils for that set | Required |
| 10 | Relic | API | equipped relic vs expected | Required |
| 11 | Infusions | API | totals by type (e.g. "18× +9 Concentration") | Advisory |
| 12 | Food | Live | active nourishment buff vs expected. Missing → Fail at category severity; wrong food → always shown as ⚠ (Advisory) | Required |
| 13 | Utility | Live | active enhancement buff vs expected; same logic as Food | Required |

**Grace periods:**
- Food/Utility are Unknown for 30 s after a map load or slot assignment.
- API checks are Unknown "waiting for API" until the first snapshot.

**To verify during implementation:**
- Food/utility buff matching: arcdps names these buffs after the item; match by name, plus a curated buff-ID table for exceptions. Verify against real logs.
- Whether AxiForge's `statPackage` is global or per-slot — confirm from the axicode model before writing check 7.
- Exact shape of AxiForge sigil/rune/relic/food/infusion fields (IDs vs names).

## 5. UI

### 5.1 Badge

Small, title-bar-less, draggable window; position saved.

| State | Text | Color |
|---|---|---|
| No comp | `axigear: no comp` | grey (click → settings) |
| No slot | `axigear: pick slot` | amber |
| All pass | `axigear ✓ 14/14` | green |
| Advisory failures only | `axigear ⚠ 2` | amber |
| Any Required failure | `axigear ✗ 3` | red |

- Unknowns appear as a dim `· 2?` suffix and never affect color.
- A pass→fail transition flashes the badge border for 3 s.
- Hidden in combat by default (setting: keep visible in combat).
- Settings: lock position, scale, "only in matching game mode" (off by default; compares map mode to the comp's `gameMode`).

### 5.2 Checklist window

Opened by clicking the badge, from arcdps's window list, or a hotkey (default `Ctrl+Shift+G`, configurable).

```
┌ axigear ─────────────────────────────────────────────┐
│ Comp: "Tuesday Zerg" (link · fetched 2m ago)  [⟳]    │
│ Slot: Party 2 · Firebrand (Quickness)   [Change slot]│
│ API: ✓ key ok · snapshot 3m ago        [Refresh API] │
├──────────────────────────────────────────────────────┤
│ ✗ Gear (2)                                           │
│    ✗ Runes       expected 6× Monk  · actual 4/6 Monk │
│    ✗ Sigils A1   expected Force    · actual Accuracy │
│ ⚠ Infusions (1)                                      │
│ ✓ Spec · Traits · Skill bar · Weapons · Stats (9)    │
│ ? Skills seen (2)  Mantra of Potence, Tome of Courage│
└──────────────────────────────────────────────────────┘
```

- Grouped by category, failures first; failed groups expanded, passing groups collapsed.
- Each row: icon, check, expected vs actual; tooltip with source and age.
- Refresh API is rate-limited to once per 30 s.
- Change slot opens the party-line picker; builds not matching your spec are shown but disabled.

### 5.3 Settings (arcdps options tab)

- **Comp:** paste box for code or link, Load, current source, Unsubscribe.
- **API key:** masked field, Test button (`/v2/tokeninfo`, reports missing `characters`/`builds` permissions).
- **Severity** dropdown per category.
- **Badge:** hide in combat, lock position, matching game mode only, scale.
- **Hotkey**, **Check for updates** (axipulse updater).

### 5.4 Storage

`addons/axigear/` in the arcdps addons folder: `config.json`, `itemdb.json`, `comp_cache.json`. The API key is stored in plain text in `config.json` (same as axiam and arcdps), documented in the README.

## 6. Error handling

Principle: never crash the game, never show a false ✗, always say why something is Unknown.

| Failure | Behavior |
|---|---|
| Panic in any callback | All exports wrapped in `catch_unwind`; log via axilog; plugin disables itself; badge shows `axigear: error (see log)` |
| Bad code/link pasted | Inline error in settings; current comp untouched |
| Network down / 404 / GitHub outage | Keep cached comp; header shows "fetched Xm ago · offline"; retry backoff 1 → 2 → 5 → 10 min (capped) |
| Decrypt failure | "Couldn't decrypt — check the link"; no retry until link changes |
| Unknown `schemaVersion` | "Update axigear"; keep last good comp |
| API key missing / invalid / missing scopes | API checks Unknown "needs API key (characters, builds)"; live checks still run |
| API 429 / 5xx | Exponential backoff; snapshot age stays visible |
| Character not on the key's account | Unknown "character not on this API key" |
| Item lookup failure | That check Unknown "couldn't resolve item N"; retried next poll |
| MumbleLink unavailable | Spec from arcdps; name-dependent API checks Unknown |
| Wine/Proton | Same stack as axipulse; `ureq` with rustls avoids system TLS |

## 7. Testing

- **axigear-core unit tests** (`cargo test`, Linux, no game):
  - AxiCode decoding against fixtures exported from AxiForge (multiple professions, core + elite, multi-line comps); parity with `@mks.haro/axicode` via a Node script in `fixtures/` that regenerates expected JSON.
  - `link.rs`: every form `parseAxiLink` accepts.
  - `publish.rs`: real `.enc` + key decrypts to expected comp; wrong key / truncated file rejected cleanly.
  - Matcher: 0 / 1 / several builds per spec; remembered pick; re-spec.
  - Checks: table-driven `(expected, observed, api snapshot) → results` per catalog row, including grace periods, staleness, unspecified fields, severity Off.
  - itemdb: recorded `/v2/items` and `/v2/itemstats` responses, no network.
- **Plugin crate:** CI build for `x86_64-pc-windows-msvc` (copy of axipulse `release.yml`); smoke test only, since logic lives in core.
- **Manual in-game checklist:**
  1. Load by code, by link, by short `/r/` link.
  2. Auto-match; manual pick when ambiguous; pick remembered after restart.
  3. Swap a sigil → ✗ appears after API refresh.
  4. Let food expire → ✗ after grace period.
  5. Badge hides in combat; setting keeps it visible.
  6. No API key → API checks Unknown, live checks work.
  7. Disconnect network → comp still works from cache.

## 8. Changes outside axigear

- **axiforge:** add `schemaVersion: 1` to the payload produced by `serializeCompForPublish`. Small separate PR.
- **axiam (follow-up):** API key handoff at game launch; transport TBD.
- **v2 (follow-up):** commander reporting built on serialized `CheckReport`.
