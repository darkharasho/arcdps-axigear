# Loadout View Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the axigear text checklist with AxiForge-style tabs that show real icons. The comp's target build is drawn, a problems-only summary sits on top, and every slot carries a ✗/⚠ chip when its check fails.

**Architecture:**
- axigear-core gains three pieces:
  1. Icon URLs: in the game-DB cache, the bundled spec data, and new bundled URL tables.
  2. Per-slot `marks` on each `CheckResult`.
  3. A pure `Loadout` view model in `UiSnapshot`, so that all "what goes in which tile" logic is host-testable.
- The Windows-only UI crate gains three pieces:
  1. A texture cache ported from arcdps-axipulse.
  2. A tile primitive.
  3. A checklist window rebuilt into header → problems panel → Build/Equipment tabs.

**Tech Stack:** Rust 2021, arcdps-imgui 0.13 (cheahjs fork), windows 0.62 D3D11, `image` 0.25, ureq 2, serde_json, Python 3 generator scripts.

**Spec:** `docs/superpowers/specs/2026-10-08-loadout-view-design.md`. Read it. This plan refines two spec details; see "Rulings" below. Task 1 amends the spec in the same commit.

## Rulings (refinements of the spec, decided while planning)

1. **Bundled URL tables replace the ~45 bundled PNGs.**
   - AxiForge itself draws armor, trinket and weapon tiles from fixed URLs:
     - `LEGENDARY_ARMOR_ICONS` and `EQUIP_TRINKET_SLOTS[].filledIcon` in `axiforge/src/renderer/modules/constants.js`;
     - `GW2_WEAPONS[].icon` in `axiforge/packages/forge-render/src/weapons.js`.
   - We copy those URLs into a Rust table, and they go through the same texture cache.
   - The allowed hosts are therefore `render.guildwars2.com` **and** `wiki.guildwars2.com`; weapon icons are on the wiki.
   - Cost if wrong: about 20 extra first-run downloads, which are then disk-cached.
2. **Relic, food and utility icons come from a bundled table.**
   - The build stores these three as *names*, not item IDs, so the spec's "item ID" route cannot reach them.
   - A new generator, `scripts/gen-named-icons.py`, reads AxiForge's `RELIC_ITEM_IDS` / `FOOD_ITEM_IDS` / `UTILITY_ITEM_IDS` (`axiforge/src/main/gw2Data/upgradeIds.json`). It fetches `/v2/items` once at generation time and writes `crates/axigear-core/data/named_icons.json` as `[{kind, name, icon, buff}]`.
3. **Trait choices with no selection.** The spec's "`TraitSel::Any`" is `TraitSel::None` in code: no major chosen for that tier means "any".
4. **Accent colour.** The underline and eyebrow accent is a new `theme::GOLD = #ffc53d` (axi-gold), per spec. Existing badge accents are untouched.
5. **Skills seen and chips.** `SkillsSeen` marks never drive a skill tile's chip. They only drive the "not cast yet" dot. Otherwise every unseen utility would carry a ? chip all the time.
6. **Overlapping weapon marks.** Weapon slots receive marks from both Weapons (type) and Stats. The tile shows the worst mark, and its tooltip lists every mark for the key.

## Global Constraints

- Run all core tests with `cargo test -p axigear-core`, and UI host tests with `cargo test -p arcdps_axigear`. Check the Windows build with `cargo dll-check`; the `xwin` alias is in `.cargo/config.toml`.
- Existing report text (`expected`, `actual`, `reason`, `detail()`, `Badge::text()`, `groups()`) must not change. Existing tests must pass **unmodified**, except where a struct literal needs the new `..` fields or `marks: vec![]`.
- `MAX_UPLOADS_PER_FRAME = 4`. SRVs live for the life of the process. Only `https://render.guildwars2.com/` and `https://wiki.guildwars2.com/` URLs are fetched.
- The disk cache is `<data_dir>/icons/<fnv64 hex of url>.img`, written as tmp then renamed.
- Window min width is `520.0`. Icon sizes:

  | Element | Size |
  |---|---|
  | Skill tile | 48 |
  | Gear row icon | 40 |
  | Trinket tile | 32 |
  | Upgrade chip | 24 |
  | Major trait | 32 |
  | Minor trait | 26 |
  | Spec emblem | 56 |
  | Status chip | 14 |

- The dark tint for faded icons is `[0.35, 0.35, 0.35, 1.0]`. The problem-click pulse lasts `1.5` s.
- Commit messages end with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- No push and no release; v0.2.0 is tagged by the user after in-game checks.

## Review Focus

Each line below names an input class that no task's happy-path test covers. It gets a pinned test in its owning task.

1. **An old `itemdb.json` with no `icon` fields.** Every cached item and skill must be re-wanted exactly once, then never again, even when the API returns no icon. Owner: Task 1 test `old_entries_are_rewanted_once_for_icons`.
2. **A published build whose skills carry names.** `skill_names` is pre-filled, but the IDs must still be fetched for icons. Today `wanted()` skips them. Owner: Task 1 test `named_build_skills_are_still_wanted_for_icons`.
3. **Off-host or malformed icon URLs** (`http://`, `""`, another domain). These must never be fetched. Owner: Task 9 test `only_gw2_hosts_are_allowed`.
4. **A two-handed main hand with an off-hand defined in the comp.** The off hand renders "Two-Handed" faded, with no mark. Owner: Task 7 test `two_handed_set_has_no_offhand_tile`.
5. **A trait line where no major is chosen (`TraitSel::None`).** No tile is `selected`, and the column is flagged `any`. Owner: Task 7 test `unchosen_tier_is_any`.

---

## File Structure

**axigear-core (host-testable)**

- `src/gamedb.rs` (modify): `icon`, `icon_checked` on `ItemInfo`/`SkillInfo`; migration in `wanted()`.
- `src/specs.rs` (modify), `data/specializations.json` (regenerate), `scripts/gen-specdb.py` (modify): emblem, background, minors, trait name and icon.
- `src/icons.rs` (create): bundled URL tables (armor by weight, trinkets, weapons) and `named_icons.json` lookups.
- `data/named_icons.json` (create), `scripts/gen-named-icons.py` (create).
- `src/report.rs` (modify): `SlotKey`, `SlotMark`, `Tab`, `CheckResult.marks`, `CheckReport::marks_for/worst`.
- `src/checks/build.rs`, `src/checks/gear.rs`, `src/checks/consumables.rs` (modify): emit marks.
- `src/loadout.rs` (create): `Loadout` view model.
- `src/session.rs`, `src/driver.rs`, `src/settings.rs` (modify): wire `loadout` into `UiSnapshot`; `loadout_tab` setting.

**arcdps_axigear UI crate**

- `Cargo.toml` (modify): `image`, windows D3D features.
- `src/ui/texture_rules.rs` (create, host-testable): URL allow-list, cache file name.
- `src/ui/textures.rs` (create, windows): worker, disk cache, upload, `get`.
- `src/ui/tile.rs` (create, windows): one tile primitive (icon or fallback, chip, dashed, tint, tooltip).
- `src/ui/problems.rs` (create, windows): the problems panel.
- `src/ui/build_tab.rs`, `src/ui/equipment_tab.rs` (create, windows).
- `src/ui/checklist.rs` (modify): header, problems, tabs, pulse state.
- `src/ui/state.rs`, `src/ui/theme.rs`, `src/ui/mod.rs`, `src/plugin.rs` (modify).

---

### Task 1: Icon fields in the game DB (+ spec amendment)

**Files:**
- Modify: `crates/axigear-core/src/gamedb.rs`
- Modify: `docs/superpowers/specs/2026-10-08-loadout-view-design.md` (apply Rulings 1 and 2)
- Test: inline `#[cfg(test)] mod tests` in `gamedb.rs`

**Interfaces:**
- Produces:
  - `ItemInfo { name, weapon_type, default_stats, icon: Option<String>, icon_checked: bool }`
  - `SkillInfo { name, weapon_type, icon: Option<String>, icon_checked: bool }`
  - `GameDb::item_icon(&self, id: u32) -> Option<&str>`
  - `GameDb::skill_icon(&self, id: u32) -> Option<&str>`

- [ ] **Step 1: Amend the spec.**
  - In the spec's "Stat-only slots" bullet, replace the bundled-PNG text with Ruling 1, including the two allowed hosts.
  - Add a bullet with Ruling 2 under "Icon URLs".
  - In "Texture loading", change "Only URLs on `https://render.guildwars2.com/`" to include `https://wiki.guildwars2.com/`.
  - Remove "The DLL grows about 1 MB" if present.

- [ ] **Step 2: Write failing tests** (append to `gamedb.rs` tests):

```rust
#[test]
fn icons_are_parsed_and_marked_checked() {
    let http = FakeHttp::default()
        .on("https://api.guildwars2.com/v2/items?ids=24842", 200,
            r#"[{"id":24842,"name":"Superior Rune of the Scholar","type":"UpgradeComponent","icon":"https://render.guildwars2.com/file/A/1.png","details":{}}]"#)
        .on("https://api.guildwars2.com/v2/skills?ids=9153", 200,
            r#"[{"id":9153,"name":"Mantra of Potence","icon":"https://render.guildwars2.com/file/B/2.png"}]"#);
    let mut db = GameDb::default();
    let w = Wanted { items: [24842].into(), skills: [9153].into(), ..Default::default() };
    db.resolve(&http, &w).unwrap();
    assert_eq!(db.item_icon(24842), Some("https://render.guildwars2.com/file/A/1.png"));
    assert_eq!(db.skill_icon(9153), Some("https://render.guildwars2.com/file/B/2.png"));
    assert!(db.items[&24842].icon_checked && db.skills[&9153].icon_checked);
}

#[test]
fn old_entries_are_rewanted_once_for_icons() {
    // An itemdb.json written by v0.1.x: no icon / icon_checked fields.
    let mut db: GameDb = serde_json::from_str(
        r#"{"items":{"24842":{"name":"Rune"}},"skills":{"9153":{"name":"Mantra"}}}"#).unwrap();
    let mut b = firebrand();
    b.equipment.runes.clear();
    b.equipment.runes.insert(GearSlot::Head, 24842);
    let w = db.wanted(Some(&b), None, &BTreeSet::new());
    assert!(w.items.contains(&24842), "old item without icon_checked is wanted");
    // The API answers with no icon at all; it must not be wanted again.
    let http = FakeHttp::default()
        .on(&format!("https://api.guildwars2.com/v2/items?ids={}", join(&w.items)), 200, r#"[{"id":24842,"name":"Rune"}]"#)
        .on(&format!("https://api.guildwars2.com/v2/skills?ids={}", join(&w.skills)), 200, r#"[{"id":9153,"name":"Mantra"}]"#);
    db.resolve(&http, &Wanted { itemstats: Default::default(), ..w.clone() }).unwrap();
    let again = db.wanted(Some(&b), None, &BTreeSet::new());
    assert!(!again.items.contains(&24842));
    assert!(!again.skills.contains(&9153));
}

#[test]
fn named_build_skills_are_still_wanted_for_icons() {
    let mut b = firebrand();
    b.skill_names.insert(b.skills.heal, "Named Heal".into());
    let w = GameDb::default().wanted(Some(&b), None, &BTreeSet::new());
    assert!(w.skills.contains(&b.skills.heal));
}

fn join(ids: &BTreeSet<u32>) -> String {
    ids.iter().map(u32::to_string).collect::<Vec<_>>().join(",")
}
```

  Add `use crate::model::GearSlot;` to the test module if it is not already imported. Check how `FakeHttp` matches URLs in `src/http.rs`. If `.on()` needs exact URLs, the `join` helper above builds them in the same `BTreeSet` order that `fetch_all` uses. If `resolve` also requests itemstats for items with `default_stats`, none are present here.

- [ ] **Step 3: Run the tests and verify they fail.**

  Run: `cargo test -p axigear-core gamedb`

  Expected: compile errors (`item_icon` and `icon_checked` don't exist).

- [ ] **Step 4: Implement.**
  - Add `#[serde(default)] pub icon: Option<String>` and `#[serde(default)] pub icon_checked: bool` to both structs. `ItemInfo` and `SkillInfo` are built with struct literals in tests: fix those literals by adding `..Default::default()`. `SkillInfo` derives `Default` already; check `ItemInfo` does too.
  - In `resolve`, set `icon: v["icon"].as_str().filter(|s| !s.is_empty()).map(String::from)` and `icon_checked: true` for items and skills.
  - In `wanted`:
    - Build skills: drop the `!b.skill_names.contains_key(id)` filter, so a build's skills are always candidates.
    - Change the final `retain`s to keep IDs that are missing **or** not icon-checked:

```rust
w.items.retain(|id| self.items.get(id).map_or(true, |i| !i.icon_checked));
w.itemstats.retain(|id| !self.itemstats.contains_key(id));
w.skills.retain(|id| self.skills.get(id).map_or(true, |s| !s.icon_checked));
```

  - Add the accessors:

```rust
pub fn item_icon(&self, id: u32) -> Option<&str> {
    self.items.get(&id)?.icon.as_deref()
}

pub fn skill_icon(&self, id: u32) -> Option<&str> {
    self.skills.get(&id)?.icon.as_deref()
}
```

  - The existing `known_ids_are_not_wanted_again` test inserts entries without `icon_checked`. Update its two literals to set `icon_checked: true`. This is the one allowed test edit; the test's intent (known means not wanted) is preserved.

- [ ] **Step 5: Run the tests and verify they pass.**

  Run: `cargo test -p axigear-core`

  Expected: all pass.

- [ ] **Step 6: Commit.**

```bash
git add crates/axigear-core/src/gamedb.rs docs/superpowers/specs/2026-10-08-loadout-view-design.md
git commit -m "feat(core): cache item and skill icon URLs

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 2: Spec emblems, minors and trait icons in the bundled spec DB

**Files:**
- Modify: `scripts/gen-specdb.py`, `crates/axigear-core/src/specs.rs`
- Regenerate: `crates/axigear-core/data/specializations.json`

**Interfaces:**
- Produces:
  - `SpecInfo { id, name, profession, elite, majors: [Vec<u32>; 3], icon: String, background: String, minors: Vec<u32>, traits: HashMap<u32, TraitInfo> }`
  - `TraitInfo { name: String, icon: String }`
  - `SpecDb::trait_info(&self, spec: u16, trait_id: u32) -> Option<&TraitInfo>`

- [ ] **Step 1: Write failing tests** (append to `specs.rs` tests):

```rust
#[test]
fn every_spec_has_art_minors_and_trait_icons() {
    let db = SpecDb::bundled();
    let mut n = 0;
    for s in db.by_id.values() {
        n += 1;
        assert!(s.icon.starts_with("https://render.guildwars2.com/"), "{} icon", s.name);
        assert!(s.background.starts_with("https://render.guildwars2.com/"), "{} background", s.name);
        assert_eq!(s.minors.len(), 3, "{} minors", s.name);
        for t in s.minors.iter().chain(s.majors.iter().flatten()) {
            let info = s.traits.get(t).unwrap_or_else(|| panic!("{} trait {t}", s.name));
            assert!(!info.name.is_empty() && info.icon.starts_with("https://render.guildwars2.com/"), "{} trait {t}", s.name);
        }
    }
    assert!(n > 60);
}

#[test]
fn trait_info_lookup() {
    let db = SpecDb::bundled();
    let t = db.trait_info(62, 2086).expect("Firebrand adept bottom");
    assert!(!t.name.is_empty());
    assert!(db.trait_info(62, 1).is_none());
}
```

- [ ] **Step 2: Run the tests and verify they fail.**

  Run: `cargo test -p axigear-core specs`

  Expected: compile errors (no field `icon`).

- [ ] **Step 3: Extend the generator.** In `scripts/gen-specdb.py`, replace `tier_of = ...` and the row building with:

```python
traits = {t["id"]: t for t in paged("traits")}
rows = []
for s in sorted(specs, key=lambda s: s["id"]):
    majors = [[], [], []]
    for tid in s["major_traits"]:
        tier = traits.get(tid, {}).get("tier", 0)
        if 1 <= tier <= 3:
            majors[tier - 1].append(tid)
    used = list(s["minor_traits"]) + [t for tier in majors for t in tier]
    rows.append({"id": s["id"], "name": s["name"], "profession": s["profession"],
                 "elite": s["elite"], "majors": majors,
                 "icon": s["icon"], "background": s["background"],
                 "minors": s["minor_traits"],
                 "traits": {str(t): {"name": traits[t]["name"], "icon": traits[t]["icon"]}
                            for t in used if t in traits}})
```

  Update the docstring to mention `icon`, `background`, `minors` and `traits`.

- [ ] **Step 4: Regenerate the data.**

  Run: `python3 scripts/gen-specdb.py`

  Expected: `N specializations -> crates/axigear-core/data/specializations.json`. Then run `git diff --stat`: only the data file and the script should change. Spot-check with `grep -c '"icon"' crates/axigear-core/data/specializations.json`, which should be a large number.

- [ ] **Step 5: Extend `SpecInfo`.** In `specs.rs`:

```rust
#[derive(Debug, Clone, Default, Deserialize)]
pub struct TraitInfo {
    pub name: String,
    pub icon: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SpecInfo {
    pub id: u16,
    pub name: String,
    pub profession: String,
    pub elite: bool,
    pub majors: [Vec<u32>; 3],
    #[serde(default)]
    pub icon: String,
    #[serde(default)]
    pub background: String,
    #[serde(default)]
    pub minors: Vec<u32>,
    /// Minor and major traits of this spec by ID (JSON keys are strings; serde_json parses them as u32).
    #[serde(default)]
    pub traits: HashMap<u32, TraitInfo>,
}
```

  Add a method to `SpecDb`:

```rust
pub fn trait_info(&self, spec: u16, trait_id: u32) -> Option<&TraitInfo> {
    self.get(spec)?.traits.get(&trait_id)
}
```

  The test reads `db.by_id` directly. It is a private field, but the test is in the same module, so that's fine.

- [ ] **Step 6: Run the tests and verify they pass.**

  Run: `cargo test -p axigear-core`

  Expected: all pass.

- [ ] **Step 7: Commit.**

```bash
git add scripts/gen-specdb.py crates/axigear-core/src/specs.rs crates/axigear-core/data/specializations.json
git commit -m "feat(core): bundle spec emblems, minors and trait icons

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 3: Bundled icon tables (slots, weapons, relic/food/utility)

**Files:**
- Create: `scripts/gen-named-icons.py`, `crates/axigear-core/data/named_icons.json`, `crates/axigear-core/src/icons.rs`
- Modify: `crates/axigear-core/src/lib.rs` (add `pub mod icons;`)

**Interfaces:**
- Produces, all in `axigear_core::icons`:
  - `pub enum Weight { Light, Medium, Heavy }`
  - `pub fn weight_of(profession: &str) -> Option<Weight>`
  - `pub fn gear_icon(slot: GearSlot, weight: Option<Weight>) -> Option<&'static str>`: armor and trinkets only; `None` for weapons.
  - `pub fn weapon_icon(weapon: &str) -> Option<&'static str>`: AxiForge weapon id, lowercase (`"greatsword"`, `"shortbow"`).
  - `pub struct Named { pub name: String, pub icon: String, pub buff: String }`
  - `pub fn named(kind: NamedKind, name: &str) -> Option<&'static Named>`, with `pub enum NamedKind { Relic, Food, Utility }`. Matching uses `crate::text::norm` on both sides, and also accepts the `consumables` alias labels.

- [ ] **Step 1: Write the generator** `scripts/gen-named-icons.py`:

```python
#!/usr/bin/env python3
"""Regenerates crates/axigear-core/data/named_icons.json.

Builds name the relic, food and utility (not an item ID), so the overlay looks
their icons up by name. IDs come from AxiForge's upgradeIds.json, the same
catalog its pickers use; names/icons/descriptions from /v2/items.
Usage: AXIFORGE=../axiforge scripts/gen-named-icons.py
"""
import json, os, sys, urllib.request

API = "https://api.guildwars2.com/v2"
here = os.path.dirname(os.path.abspath(__file__))
axiforge = os.environ.get("AXIFORGE", os.path.join(here, "..", "..", "axiforge"))
ids = json.load(open(os.path.join(axiforge, "src/main/gw2Data/upgradeIds.json")))
out_path = sys.argv[1] if len(sys.argv) > 1 else os.path.join(here, "..", "crates/axigear-core/data/named_icons.json")

def items(id_list):
    out = []
    for i in range(0, len(id_list), 200):
        chunk = ",".join(str(x) for x in id_list[i:i + 200])
        with urllib.request.urlopen(f"{API}/items?ids={chunk}", timeout=30) as r:
            out += json.load(r)
    return out

rows, seen = [], set()
for kind, key in (("relic", "RELIC_ITEM_IDS"), ("food", "FOOD_ITEM_IDS"), ("utility", "UTILITY_ITEM_IDS")):
    for it in items(ids[key]):
        name, icon = it.get("name", ""), it.get("icon", "")
        if not name or not icon or (kind, name) in seen:
            continue
        seen.add((kind, name))
        buff = (it.get("details") or {}).get("description", "") or it.get("description", "")
        rows.append({"kind": kind, "name": name, "icon": icon, "buff": " ".join(buff.split())})
rows.sort(key=lambda r: (r["kind"], r["name"]))
with open(out_path, "w") as f:
    json.dump(rows, f, indent=1)
    f.write("\n")
print(f"{len(rows)} named icons -> {out_path}")
```

  If `upgradeIds.json` values are not plain int lists, for example `{ids: [...]}`, adapt the `ids[key]` access. Print `type(ids[key])` once to check.

- [ ] **Step 2: Run the generator.**

  Run: `chmod +x scripts/gen-named-icons.py && scripts/gen-named-icons.py`

  Expected: `N named icons -> …`, with N > 500. Strip GW2 markup from `buff` if present: if `grep -c '<c=' crates/axigear-core/data/named_icons.json` is non-zero, add `re.sub(r"<[^>]+>", "", buff)` in the script and rerun.

- [ ] **Step 3: Write failing tests** in a new `crates/axigear-core/src/icons.rs` (tests at the bottom; the implementation comes in Step 5):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::GearSlot;

    #[test]
    fn weights_follow_profession() {
        assert_eq!(weight_of("Guardian"), Some(Weight::Heavy));
        assert_eq!(weight_of("thief"), Some(Weight::Medium));
        assert_eq!(weight_of("Necromancer"), Some(Weight::Light));
        assert_eq!(weight_of("Bogus"), None);
    }

    #[test]
    fn every_armor_and_trinket_slot_has_an_icon() {
        for w in [Weight::Light, Weight::Medium, Weight::Heavy] {
            for s in GearSlot::ARMOR {
                assert!(gear_icon(s, Some(w)).is_some_and(|u| u.starts_with("https://render.guildwars2.com/")), "{s:?} {w:?}");
            }
        }
        for s in [GearSlot::Back, GearSlot::Amulet, GearSlot::Ring1, GearSlot::Ring2, GearSlot::Accessory1, GearSlot::Accessory2] {
            assert!(gear_icon(s, None).is_some(), "{s:?}");
        }
        assert!(gear_icon(GearSlot::Head, None).is_none(), "armor needs a weight");
        assert!(gear_icon(GearSlot::WeaponA1, Some(Weight::Heavy)).is_none());
    }

    #[test]
    fn weapons_resolve_by_axiforge_id() {
        for w in ["axe", "dagger", "mace", "pistol", "sword", "scepter", "focus", "shield", "torch", "warhorn",
                  "greatsword", "hammer", "longbow", "rifle", "shortbow", "staff", "spear"] {
            assert!(weapon_icon(w).is_some_and(|u| u.starts_with("https://wiki.guildwars2.com/")), "{w}");
        }
        assert!(weapon_icon("Greatsword").is_some(), "case-insensitive");
        assert!(weapon_icon("short bow").is_some(), "the API spelling");
        assert!(weapon_icon("banana").is_none());
    }

    #[test]
    fn named_lookups_normalise_and_follow_aliases() {
        let relic = named(NamedKind::Relic, "relic of the thief").expect("relic");
        assert!(relic.icon.starts_with("https://render.guildwars2.com/"));
        assert!(named(NamedKind::Food, "Plate of Truffle Steak Dinner").is_some());
        assert!(named(NamedKind::Utility, "Superior Sharpening Stone").is_some());
        assert!(named(NamedKind::Food, "Superior Sharpening Stone").is_none(), "kinds are separate");
    }
}
```

  If "Relic of the Thief" or "Plate of Truffle Steak Dinner" is absent from the generated JSON, run `grep -i` on it and swap in a relic or food name that is present. The test checks the mechanism, not a specific item.

- [ ] **Step 4: Run the tests and verify they fail.**

  Run: `cargo test -p axigear-core icons`

  Expected: compile errors.

- [ ] **Step 5: Implement `icons.rs`.** Copy the URL strings **verbatim** from AxiForge:
  - `sed -n '/LEGENDARY_ARMOR_ICONS/,/^};/p' ../axiforge/src/renderer/modules/constants.js`
  - `grep filledIcon ../axiforge/src/renderer/modules/constants.js`
  - `grep 'id: ' ../axiforge/packages/forge-render/src/weapons.js`

  The code below uses `/* … */` for the URL strings. Replace each one with the real URL from those files.

```rust
//! Icon URLs the build doesn't carry: armor/trinket/weapon art (copied from
//! AxiForge's constants.js / weapons.js) and relic/food/utility by name
//! (scripts/gen-named-icons.py).

use std::collections::HashMap;

use once_cell::sync::Lazy;
use serde::Deserialize;

use crate::model::GearSlot;
use crate::text::norm;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Weight { Light, Medium, Heavy }

pub fn weight_of(profession: &str) -> Option<Weight> {
    Some(match profession.to_ascii_lowercase().as_str() {
        "elementalist" | "mesmer" | "necromancer" => Weight::Light,
        "engineer" | "ranger" | "thief" => Weight::Medium,
        "guardian" | "warrior" | "revenant" => Weight::Heavy,
        _ => return None,
    })
}

const R: &str = "https://render.guildwars2.com/file";
// LEGENDARY_ARMOR_ICONS, in GearSlot::ARMOR order (head, shoulders, chest, hands, legs, feet).
const LIGHT: [&str; 6] = [/* six full URLs from constants.js `light` */];
const MEDIUM: [&str; 6] = [/* `medium` */];
const HEAVY: [&str; 6] = [/* `heavy` */];
// EQUIP_TRINKET_SLOTS filledIcon.
const BACK: &str = /* back */;
const AMULET: &str = /* amulet */;
const RING: &str = /* ring1 */;
const ACCESSORY: &str = /* accessory1 */;

pub fn gear_icon(slot: GearSlot, weight: Option<Weight>) -> Option<&'static str> {
    let armor = |i: usize| weight.map(|w| match w { Weight::Light => LIGHT[i], Weight::Medium => MEDIUM[i], Weight::Heavy => HEAVY[i] });
    match slot {
        GearSlot::Back => Some(BACK),
        GearSlot::Amulet => Some(AMULET),
        GearSlot::Ring1 | GearSlot::Ring2 => Some(RING),
        GearSlot::Accessory1 | GearSlot::Accessory2 => Some(ACCESSORY),
        s if s.is_weapon() => None,
        s => armor(GearSlot::ARMOR.iter().position(|a| *a == s)?),
    }
}

// GW2_WEAPONS from forge-render/src/weapons.js (land weapons).
const WEAPONS: [(&str, &str); 17] = [
    ("axe", /* url */), ("dagger", /* url */), ("mace", /* url */), ("pistol", /* url */),
    ("sword", /* url */), ("scepter", /* url */), ("focus", /* url */), ("shield", /* url */),
    ("torch", /* url */), ("warhorn", /* url */), ("greatsword", /* url */), ("hammer", /* url */),
    ("longbow", /* url */), ("rifle", /* url */), ("shortbow", /* url */), ("staff", /* url */),
    ("spear", /* url */),
];

pub fn weapon_icon(weapon: &str) -> Option<&'static str> {
    let key: String = weapon.to_ascii_lowercase().chars().filter(|c| !c.is_whitespace()).collect();
    WEAPONS.iter().find(|(id, _)| *id == key).map(|(_, url)| *url)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NamedKind { Relic, Food, Utility }

#[derive(Debug, Clone, Deserialize)]
pub struct Named {
    pub kind: NamedKind,
    pub name: String,
    pub icon: String,
    #[serde(default)]
    pub buff: String,
}

static NAMED: Lazy<HashMap<(NamedKind, String), Named>> = Lazy::new(|| {
    let rows: Vec<Named> = serde_json::from_str(include_str!("../data/named_icons.json")).expect("bundled named_icons.json");
    rows.into_iter().map(|r| ((r.kind, norm(&r.name)), r)).collect()
});

pub fn named(kind: NamedKind, name: &str) -> Option<&'static Named> {
    NAMED.get(&(kind, norm(name)))
}
```

  Remove the unused `R` const if every URL is written out in full; otherwise use `concat!` with literal prefixes. Simplest: paste the full URLs and delete `R`.

  The spec asked for consumable aliases to apply. `Plate of Truffle Steak Dinner` is AxiForge's own label, and it *is* the item name, so `named()` needs no alias. The aliases in `consumables.rs` map labels to **buff** names, not item names. No alias handling is needed here. Don't add any.

- [ ] **Step 6: Run the tests and verify they pass.**

  Run: `cargo test -p axigear-core`

  Expected: all pass.

- [ ] **Step 7: Commit.**

```bash
git add scripts/gen-named-icons.py crates/axigear-core/data/named_icons.json crates/axigear-core/src/icons.rs crates/axigear-core/src/lib.rs
git commit -m "feat(core): bundled icon tables for gear, weapons, relics and consumables

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 4: Slot marks on check results

**Files:**
- Modify: `crates/axigear-core/src/report.rs`

**Interfaces:**
- Produces, all in `axigear_core::report`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum SlotKey {
    Gear(GearSlot),
    Rune(GearSlot),
    Sigil(GearSlot, u8),
    Infusions,
    Skill(u8),               // 0 heal, 1..=3 utilities, 4 elite
    Trait { line: u8, tier: u8 }, // line = index into Build.specs (0..3); tier 0..3
    Spec(u8),                // line 0..3
    Relic,
    Food,
    Utility,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Tab { #[default] Build, Equipment }

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlotMark { pub key: SlotKey, pub status: Status, pub detail: Option<String> }
```

  - `CheckResult.marks: Vec<SlotMark>` with `#[serde(default)]`.
  - `CheckResult::mark(self, key: SlotKey, status: Status, detail: Option<String>) -> Self`: a builder that pushes one mark.
  - `CheckResult::mark_tone(&self, m: &SlotMark) -> Tone`: same table as `tone()`, but using `m.status`.
  - `SlotKey::tab(self) -> Tab`: Skill, Trait and Spec give `Build`; everything else gives `Equipment`.
  - `CheckReport::marks_for(&self, key: SlotKey) -> Vec<(&CheckResult, &SlotMark)>`.
  - `CheckReport::worst(&self, key: SlotKey, skip: &[Category]) -> Option<Tone>`: the lowest `Tone::rank` among matching marks whose result category is not in `skip`.

- [ ] **Step 1: Write failing tests** (append to `report.rs` tests):

```rust
fn report_with(results: Vec<CheckResult>) -> CheckReport {
    CheckReport { comp_key: "k".into(), comp_name: "c".into(), slot: Default::default(), slot_label: "s".into(), results }
}

#[test]
fn marks_follow_status_and_severity() {
    use crate::model::GearSlot;
    let mut r = CheckResult::new(Category::Runes, "runes", "Runes", Status::Fail, "x")
        .mark(SlotKey::Rune(GearSlot::Head), Status::Pass, None)
        .mark(SlotKey::Rune(GearSlot::Feet), Status::Fail, Some("Scholar".into()));
    assert_eq!(r.mark_tone(&r.marks[1]), Tone::Danger);
    r.severity = Severity::Advisory;
    assert_eq!(r.mark_tone(&r.marks[1]), Tone::Warn);
    assert_eq!(r.mark_tone(&r.marks[0]), Tone::Ok);
}

#[test]
fn worst_mark_wins_and_skip_filters_categories() {
    use crate::model::GearSlot;
    let k = SlotKey::Gear(GearSlot::WeaponA1);
    let a = CheckResult::new(Category::Weapons, "weapons.A", "Weapons A", Status::Pass, "x").mark(k, Status::Pass, None);
    let b = CheckResult::new(Category::Stats, "stats", "Stats", Status::Fail, "x").mark(k, Status::Fail, Some("Rampager's".into()));
    let rep = report_with(vec![a, b]);
    assert_eq!(rep.worst(k, &[]), Some(Tone::Danger));
    assert_eq!(rep.worst(k, &[Category::Stats]), Some(Tone::Ok));
    assert_eq!(rep.marks_for(k).len(), 2);
    assert_eq!(rep.worst(SlotKey::Relic, &[]), None);
}

#[test]
fn keys_know_their_tab() {
    use crate::model::GearSlot;
    assert_eq!(SlotKey::Skill(0).tab(), Tab::Build);
    assert_eq!(SlotKey::Trait { line: 1, tier: 2 }.tab(), Tab::Build);
    assert_eq!(SlotKey::Spec(2).tab(), Tab::Build);
    assert_eq!(SlotKey::Rune(GearSlot::Head).tab(), Tab::Equipment);
    assert_eq!(SlotKey::Food.tab(), Tab::Equipment);
}
```

  If `SlotRef` has no `Default`, build it with its fields (`SlotRef { line: 0, slot: 0, build: 0 }`).

- [ ] **Step 2: Run the tests and verify they fail.**

  Run: `cargo test -p axigear-core report`

  Expected: compile errors.

- [ ] **Step 3: Implement.**
  - Add the types above.
  - Add `marks: Vec::new()` in `CheckResult::new`, and the `#[serde(default)] pub marks: Vec<SlotMark>` field.
  - Add the methods:

```rust
impl CheckResult {
    pub fn mark(mut self, key: SlotKey, status: Status, detail: Option<String>) -> Self {
        self.marks.push(SlotMark { key, status, detail });
        self
    }

    pub fn mark_tone(&self, m: &SlotMark) -> Tone {
        match (m.status, self.severity) {
            (Status::Pass, _) => Tone::Ok,
            (Status::Fail, Severity::Required) => Tone::Danger,
            (Status::Fail, Severity::Advisory) => Tone::Warn,
            (Status::Unknown, _) => Tone::Neutral,
        }
    }
}

impl SlotKey {
    pub fn tab(self) -> Tab {
        match self {
            SlotKey::Skill(_) | SlotKey::Trait { .. } | SlotKey::Spec(_) => Tab::Build,
            _ => Tab::Equipment,
        }
    }
}

impl CheckReport {
    pub fn marks_for(&self, key: SlotKey) -> Vec<(&CheckResult, &SlotMark)> {
        self.results.iter().flat_map(|r| r.marks.iter().filter(move |m| m.key == key).map(move |m| (r, m))).collect()
    }

    pub fn worst(&self, key: SlotKey, skip: &[Category]) -> Option<Tone> {
        self.marks_for(key).into_iter().filter(|(r, _)| !skip.contains(&r.category)).map(|(r, m)| r.mark_tone(m)).min_by_key(|t| t.rank())
    }
}
```

  Import `crate::model::GearSlot` at the top. Fix any `CheckResult { … }` struct literals elsewhere (grep `CheckResult {`) by adding `marks: vec![]`.

- [ ] **Step 4: Run the tests and verify they pass.**

  Run: `cargo test -p axigear-core`

  Expected: all pass.

- [ ] **Step 5: Commit.**

```bash
git add crates/axigear-core/src/report.rs
git commit -m "feat(core): per-slot marks on check results

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 5: Build checks emit marks

**Files:**
- Modify: `crates/axigear-core/src/checks/build.rs` (functions `spec`, `specializations`, `traits`, `skill_bar`, `skills_seen`)
- Test: same file's tests module

**Interfaces:**
- Consumes: `CheckResult::mark` and `SlotKey` from Task 4.
- Produces the marks below. Later tasks read them through `CheckReport::marks_for` and `worst`.

| Check | Marks |
|---|---|
| `spec` | `Spec(2)`, carrying the result's own status and `actual` as detail. No mark when Unknown. |
| `specializations` | `Spec(i)` for each `i` where `build.specs[i].id != 0`. Pass if that id is in `have`. For `i == 2` it also requires `elite_ok`. Fail detail is the name of `snap.build.specs[i]` (`spec_name`), or `"empty"`. |
| `traits` | `Trait { line: i, tier: t }` for each chosen tier (not `TraitSel::None`), using the line's index `i` in `build.specs`. Pass/Fail/Unknown as computed per tier (`ok`). Fail detail is the worn trait's name from `ctx.specs.trait_info(line.id, api.traits[t])`, falling back to its position word. |
| `skill_bar` | `Skill(0)` heal and `Skill(4)` elite: Pass or Fail, detail is the worn skill name. `Skill(1 + u)` for each non-zero wanted utility at index `u`: Pass if consumed from the pool, else Fail with detail `"not slotted"`. No marks for the Revenant Unknown case. |
| `skills_seen` | `Skill(1 + u)` per utility index and `Skill(4)` for the elite, with the row's status. |

- [ ] **Step 1: Write failing tests** (append to `build.rs` tests):

```rust
use crate::report::SlotKey;

fn mark(w: &World, id: &str, key: SlotKey) -> Option<(Status, Option<String>)> {
    w.result(id).marks.into_iter().find(|m| m.key == key).map(|m| (m.status, m.detail))
}

#[test]
fn matching_build_marks_every_build_slot_pass() {
    let w = World::matching(firebrand());
    for i in 0..3 {
        assert_eq!(mark(&w, "specs", SlotKey::Spec(i)).map(|m| m.0), Some(Status::Pass), "spec line {i}");
    }
    assert_eq!(mark(&w, "spec", SlotKey::Spec(2)).map(|m| m.0), Some(Status::Pass));
    for k in 0..5 {
        assert_eq!(mark(&w, "skills", SlotKey::Skill(k)).map(|m| m.0), Some(Status::Pass), "skill {k}");
    }
    assert_eq!(mark(&w, "traits.42", SlotKey::Trait { line: 0, tier: 0 }).map(|m| m.0), Some(Status::Pass));
}

#[test]
fn a_wrong_trait_marks_only_its_tier() {
    let mut w = World::matching(firebrand());
    let wanted = w.snap_mut().build.specs[0].traits[0];
    let other = crate::specs::SpecDb::bundled().get(42).unwrap().majors[0].iter().copied().find(|t| *t != wanted).unwrap();
    w.snap_mut().build.specs[0].traits[0] = other;
    let (st, detail) = mark(&w, "traits.42", SlotKey::Trait { line: 0, tier: 0 }).unwrap();
    assert_eq!(st, Status::Fail);
    assert!(detail.is_some_and(|d| !d.is_empty()));
    assert_eq!(mark(&w, "traits.42", SlotKey::Trait { line: 0, tier: 1 }).map(|m| m.0), Some(Status::Pass));
}

#[test]
fn a_missing_utility_and_wrong_heal_are_marked() {
    let mut w = World::matching(firebrand());
    w.snap_mut().build.skills.heal = 12345;
    w.snap_mut().build.skills.utilities[0] = 54321;
    assert_eq!(mark(&w, "skills", SlotKey::Skill(0)).map(|m| m.0), Some(Status::Fail));
    let fails = w.result("skills").marks.iter().filter(|m| matches!(m.key, SlotKey::Skill(1..=3)) && m.status == Status::Fail).count();
    assert_eq!(fails, 1);
}

#[test]
fn a_wrong_line_marks_that_spec_card() {
    let mut w = World::matching(firebrand());
    w.snap_mut().build.specs[1].id = 16;
    assert_eq!(mark(&w, "specs", SlotKey::Spec(1)).map(|m| m.0), Some(Status::Fail));
    assert_eq!(mark(&w, "specs", SlotKey::Spec(0)).map(|m| m.0), Some(Status::Pass));
}

#[test]
fn unseen_skills_mark_unknown() {
    let mut w = World::matching(firebrand());
    w.live.skills_cast.clear();
    let r = w.result("seen.9153");
    assert!(r.marks.iter().any(|m| matches!(m.key, SlotKey::Skill(_)) && m.status == Status::Unknown));
}
```

  Firebrand's fixture fills all three utilities. If it does not, `Skill(1..=3)` in the first test needs to skip empty ones; check `firebrand().skills` first. The same applies to "line 0 tier 0 is chosen": if the fixture's line 0 has `TraitSel::None` at tier 0, pick a chosen tier.

- [ ] **Step 2: Run the tests and verify they fail.**

  Run: `cargo test -p axigear-core checks::build`

  Expected: the new tests fail (no marks).

- [ ] **Step 3: Implement.** Make these edits:

  - `spec`: in the final line, `let st = if ok {…}`, then `vec![row(st).with_actual(actual.clone()).mark(SlotKey::Spec(2), st, Some(actual))]`.
  - `specializations`: after building the result `r`, loop:

```rust
let mut r = /* existing result */;
for (i, line) in ctx.build.specs.iter().enumerate().filter(|(_, l)| l.id != 0) {
    let present = have.contains(&line.id) && (i != 2 || elite_ok);
    let worn = snap.build.specs.get(i).map(|s| s.id).filter(|id| *id != 0).map_or("empty".to_string(), |id| spec_name(ctx, id));
    r = r.mark(SlotKey::Spec(i as u8), if present { Status::Pass } else { Status::Fail }, (!present).then_some(worn));
}
vec![r]
```

  - `traits`: change the loop to `for (i, line) in ctx.build.specs.iter().enumerate().filter(|(_, l)| l.id != 0)`. Collect `let mut marks = Vec::new();`. Inside the tier loop, after computing `ok` and `have_pos`:

```rust
let st = match ok { Some(true) => Status::Pass, Some(false) => Status::Fail, None => Status::Unknown };
let worn = ctx.specs.trait_info(line.id, api.traits[t]).map(|t| t.name.clone())
    .unwrap_or_else(|| format!("{} {}", TIERS[t], have_pos.map_or("?", position_word)));
marks.push((SlotKey::Trait { line: i as u8, tier: t as u8 }, st, (st == Status::Fail).then_some(worn)));
```

    After building `r`, apply each mark: `for (k, s, d) in marks { r = r.mark(k, s, d); }`.
  - `skill_bar`:
    - Build marks alongside `problems`. For heal: `if want.heal != 0 { marks.push((SlotKey::Skill(0), pass_or_fail(want.heal == have.heal), (want.heal != have.heal).then(|| skill_name(ctx, have.heal)))) }`. Do the same for the elite with `Skill(4)`.
    - For utilities, iterate `want.utilities.iter().enumerate().filter(|(_, u)| **u != 0)`, and push `Skill(1 + idx)` Pass when consumed, else Fail with `Some("not slotted".into())`.
    - Apply the marks to the returned row in both the Pass and Fail branches.
    - Add `fn pass_or_fail(ok: bool) -> Status { if ok { Status::Pass } else { Status::Fail } }` at file scope.
  - `skills_seen`: switch to `s.utilities.iter().copied().enumerate().map(|(i, id)| (1 + i as u8, id)).chain([(4u8, s.elite)]).filter(|(_, id)| *id != 0)`. Mark each row: `.mark(SlotKey::Skill(k), status, None)`.

  Import `crate::report::SlotKey`.

- [ ] **Step 4: Run the tests and verify they pass.**

  Run: `cargo test -p axigear-core`

  Expected: all pass, including every pre-existing test unchanged.

- [ ] **Step 5: Commit.**

```bash
git add crates/axigear-core/src/checks/build.rs
git commit -m "feat(core): build checks mark spec, trait and skill slots

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 6: Gear and consumable checks emit marks

**Files:**
- Modify: `crates/axigear-core/src/checks/gear.rs` (`weapons`, `stats`, `runes`, `sigils`, `relic`, `infusions`)
- Modify: `crates/axigear-core/src/checks/consumables.rs` (`check`)

**Interfaces:**
- Consumes: Task 4's types.
- Produces these marks:

| Check | Marks |
|---|---|
| `weapons` | `Gear(slot)` for each wanted slot in the set. `"empty"` gives Fail. An unresolved type gives Unknown. A type mismatch gives Fail with the worn type, lowercase. A match gives Pass. |
| `stats` | `Gear(slot)` for every wanted slot. Empty gives Fail `"empty"`. An unresolved stat gives Unknown. Consumed from the pool gives Pass. Leftover gives Fail with the worn stat name. |
| `runes` | `Rune(slot)` per wanted slot. None gives Fail `"none"`. `same_upgrade` true gives Pass, false gives Fail with the item label, `None` gives Unknown. |
| `sigils` | `Sigil(slot, idx)` per wanted sigil, where `idx` is its position in `s.get(slot)`. Matched gives Pass. Missing gives Fail with detail `"missing"`. Unresolved gives Unknown. |
| `relic` | `Relic`, carrying the row's status (none when it's the "API doesn't report" Unknown); detail is the worn name on Fail. |
| `infusions` | `Infusions`, carrying the row's status; detail is the `actual` summary on Fail. |
| food / utility (`consumables::check`) | `Food` / `Utility`, carrying the row status. Detail is the worn name or `"none"` on Fail. No mark for the grace-period Unknown. |

- [ ] **Step 1: Write failing tests** (append to `gear.rs` tests):

```rust
use crate::report::SlotKey;

fn mark_of(w: &World, id: &str, key: SlotKey) -> Option<(Status, Option<String>)> {
    w.result(id).marks.into_iter().find(|m| m.key == key).map(|m| (m.status, m.detail))
}

#[test]
fn matching_gear_marks_pass() {
    let w = World::matching(firebrand());
    assert_eq!(mark_of(&w, "stats", SlotKey::Gear(GearSlot::Head)).map(|m| m.0), Some(Status::Pass));
    let rune_slot = *firebrand().equipment.runes.keys().next().unwrap();
    assert_eq!(mark_of(&w, "runes", SlotKey::Rune(rune_slot)).map(|m| m.0), Some(Status::Pass));
    assert_eq!(mark_of(&w, "weapons.A", SlotKey::Gear(GearSlot::WeaponA1)).map(|m| m.0), Some(Status::Pass));
}

#[test]
fn an_empty_slot_marks_that_slot_only() {
    let mut w = World::matching(firebrand());
    w.snap_mut().equipment.retain(|i| i.slot != "Helm");
    assert_eq!(mark_of(&w, "stats", SlotKey::Gear(GearSlot::Head)), Some((Status::Fail, Some("empty".into()))));
    assert_eq!(mark_of(&w, "stats", SlotKey::Gear(GearSlot::Feet)).map(|m| m.0), Some(Status::Pass));
}

#[test]
fn a_missing_rune_marks_its_slot() {
    let mut w = World::matching(firebrand());
    let slot = *firebrand().equipment.runes.keys().next().unwrap();
    w.item_mut(slot).upgrades.clear();
    assert_eq!(mark_of(&w, "runes", SlotKey::Rune(slot)), Some((Status::Fail, Some("none".into()))));
}

#[test]
fn sigils_mark_each_wanted_sigil() {
    let w = World::matching(firebrand());
    let r = w.result("sigils.A");
    let n = firebrand().equipment.sigils.a1.len() + firebrand().equipment.sigils.a2.len();
    assert_eq!(r.marks.iter().filter(|m| matches!(m.key, SlotKey::Sigil(_, _))).count(), n);
    assert!(r.marks.iter().all(|m| m.status == Status::Pass));
}

#[test]
fn relic_and_infusions_are_marked() {
    let w = World::matching(firebrand());
    assert_eq!(mark_of(&w, "relic", SlotKey::Relic).map(|m| m.0), Some(Status::Pass));
    assert_eq!(mark_of(&w, "infusions", SlotKey::Infusions).map(|m| m.0), Some(Status::Pass));
}
```

  Also append to `consumables.rs` tests:

```rust
#[test]
fn food_and_utility_are_marked() {
    use crate::report::SlotKey;
    let w = World::matching(firebrand());
    assert!(w.result("food").marks.iter().any(|m| m.key == SlotKey::Food && m.status == Status::Pass));
    let mut w = World::matching(firebrand());
    w.live.active.clear();
    let r = w.result("utility");
    assert!(r.marks.iter().any(|m| m.key == SlotKey::Utility && m.status == Status::Fail && m.detail.as_deref() == Some("none")));
}
```

  Check the `World` helper names (`snap_mut`, `item_mut`, `api_slot`) in `testutil.rs`. The API slot name for the head is `"Helm"`. If `firebrand()` has no relic or infusions, set them in the test, or assert `None` and use `berserker()` instead.

- [ ] **Step 2: Run the tests and verify they fail.**

  Run: `cargo test -p axigear-core checks::`

- [ ] **Step 3: Implement.** Collect marks into `let mut marks: Vec<(SlotKey, Status, Option<String>)> = Vec::new();` in each function while it already iterates, then apply them to the result. Add a small helper at file scope in `gear.rs`:

```rust
fn with_marks(mut r: CheckResult, marks: Vec<(SlotKey, Status, Option<String>)>) -> CheckResult {
    for (k, s, d) in marks {
        r = r.mark(k, s, d);
    }
    r
}
```

  - `weapons`: in the `for (slot, want)` loop, push `(SlotKey::Gear(slot), …)` in each arm, using the table above. Then `out.push(with_marks(r, marks))`.
  - `stats`: in the per-slot loop:
    - `None` gives Fail `"empty"`.
    - Unresolved gives Unknown.
    - `Some(i)` gives Pass.
    - `None` from the pool position gives Fail `have.to_string()`.

    Then `vec![with_marks(finish(…), marks)]`.
  - `runes`: one push in each arm. Use `with_marks` on `r` before returning.
  - `sigils`: build `want: Vec<(GearSlot, u8, u32)>` from `slots.iter().flat_map(|sl| s.get(*sl).iter().enumerate().map(move |(i, id)| (*sl, i as u8, *id)))`. Keep a `Vec<u32>` of just the IDs for the existing `expected` summary. Push a mark in each arm of the matching `match`.
  - `relic`: in the `Some(have)` arm, `let st = …; vec![row(st).with_actual(have).mark(SlotKey::Relic, st, (st == Status::Fail).then(|| have.to_string()))]`. In the unresolved arm, add `.mark(SlotKey::Relic, Status::Unknown, None)`.
  - `infusions`: each return adds `.mark(SlotKey::Infusions, status, (status == Status::Fail).then(|| actual.clone()))`.
  - `consumables::check`:
    - Map `kind` to `SlotKey::Food` or `SlotKey::Utility`.
    - Add `.mark(key, Status::Pass, None)` to the Pass return.
    - Add `.mark(key, Status::Fail, Some(name.into()))` to the wrong-buff return.
    - Add `.mark(key, Status::Fail, Some("none".into()))` to the absent return.
    - Add `.mark(key, Status::Unknown, None)` to the final no-data return.
    - The grace-period return gets no mark.

- [ ] **Step 4: Run the tests and verify they pass.**

  Run: `cargo test -p axigear-core`

  Expected: all pass.

- [ ] **Step 5: Commit.**

```bash
git add crates/axigear-core/src/checks/gear.rs crates/axigear-core/src/checks/consumables.rs
git commit -m "feat(core): gear and consumable checks mark their slots

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 7: `Loadout` view model

**Files:**
- Create: `crates/axigear-core/src/loadout.rs`
- Modify: `crates/axigear-core/src/lib.rs` (`pub mod loadout;`)

**Interfaces:**
- Consumes:
  - `GameDb::item_icon` and `skill_icon` (Task 1)
  - `SpecDb::trait_info` and `SpecInfo.icon`, `background`, `minors` (Task 2)
  - `icons::{weight_of, gear_icon, weapon_icon, named, NamedKind}` (Task 3)
  - `SlotKey` (Task 4)
- Produces:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tile {
    pub key: SlotKey,
    /// Short slot label: "Head", "Acc 1", "Heal", "Rune".
    pub label: String,
    /// What the comp wants here: stat name, item/skill/trait name. Empty when `empty`.
    pub name: String,
    /// Second line: buff text for food/utility; None elsewhere.
    pub sub: Option<String>,
    pub icon: Option<String>,
    /// The comp leaves this slot unspecified (dashed, faded).
    pub empty: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GearRow { pub tile: Tile, pub upgrades: Vec<Tile> }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WeaponSet { pub label: &'static str, pub main: GearRow, pub off: Option<GearRow>, pub two_handed: bool }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraitTile { pub tile: Tile, pub selected: bool }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpecCard {
    pub key: SlotKey,
    pub name: String,
    pub icon: Option<String>,
    pub background: Option<String>,
    pub minors: Vec<Tile>,              // 3, keys Spec(line)
    pub majors: [Vec<TraitTile>; 3],    // per tier, 3 choices each, keys Trait{line,tier}
    pub any: [bool; 3],                 // tier left unchosen (TraitSel::None)
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Loadout {
    pub skills: Vec<Tile>,         // always 5: heal, 3 utilities, elite
    pub specs: Vec<SpecCard>,      // lines with id != 0
    pub armor: Vec<GearRow>,       // always 6, GearSlot::ARMOR order; upgrades = [rune] when set
    pub weapons: Vec<WeaponSet>,   // sets the build defines (A, then B)
    pub trinkets: Vec<Tile>,       // Back, Acc1, Acc2, Amulet, Ring1, Ring2
    pub relic: Tile,
    pub infusions: Vec<Tile>,      // one per wanted infusion, key Infusions
    pub food: Tile,
    pub utility: Tile,
}

impl Loadout {
    pub fn of(build: &Build, db: &GameDb, specs: &SpecDb) -> Loadout;
}
```

- [ ] **Step 1: Write failing tests** (at the bottom of the new `loadout.rs`):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::gamedb::{GameDb, ItemInfo, SkillInfo};
    use crate::model::{GearSlot, TraitSel};
    use crate::specs::SpecDb;
    use crate::testutil::firebrand;

    fn db_with_icons(b: &Build) -> GameDb {
        let mut db = GameDb::default();
        for id in [b.skills.heal, b.skills.elite].into_iter().chain(b.skills.utilities).filter(|i| *i != 0) {
            db.skills.insert(id, SkillInfo { name: format!("S{id}"), icon: Some(format!("https://render.guildwars2.com/s/{id}.png")), icon_checked: true, ..Default::default() });
        }
        for id in b.equipment.runes.values().copied().chain(b.equipment.infusions.iter().copied()) {
            db.items.insert(id, ItemInfo { name: format!("I{id}"), icon: Some(format!("https://render.guildwars2.com/i/{id}.png")), icon_checked: true, ..Default::default() });
        }
        db
    }

    #[test]
    fn skill_bar_has_five_tiles_with_icons() {
        let b = firebrand();
        let l = Loadout::of(&b, &db_with_icons(&b), SpecDb::bundled());
        assert_eq!(l.skills.len(), 5);
        assert_eq!(l.skills[0].key, SlotKey::Skill(0));
        assert_eq!(l.skills[0].icon.as_deref(), Some(format!("https://render.guildwars2.com/s/{}.png", b.skills.heal).as_str()));
        assert_eq!(l.skills[4].label, "Elite");
    }

    #[test]
    fn armor_rows_use_weight_icons_and_rune_chips() {
        let b = firebrand();
        let l = Loadout::of(&b, &db_with_icons(&b), SpecDb::bundled());
        assert_eq!(l.armor.len(), 6);
        let head = &l.armor[0];
        assert_eq!(head.tile.key, SlotKey::Gear(GearSlot::Head));
        assert_eq!(head.tile.icon.as_deref(), crate::icons::gear_icon(GearSlot::Head, Some(crate::icons::Weight::Heavy)));
        assert_eq!(head.tile.name, b.equipment.stats[&GearSlot::Head]);
        if b.equipment.runes.contains_key(&GearSlot::Head) {
            assert_eq!(head.upgrades[0].key, SlotKey::Rune(GearSlot::Head));
            assert!(head.upgrades[0].icon.is_some());
        }
    }

    #[test]
    fn spec_cards_carry_art_and_selection() {
        let b = firebrand();
        let l = Loadout::of(&b, &GameDb::default(), SpecDb::bundled());
        assert_eq!(l.specs.len(), 3);
        let fb = &l.specs[2];
        assert_eq!(fb.name, "Firebrand");
        assert!(fb.icon.is_some() && fb.background.is_some());
        assert_eq!(fb.minors.len(), 3);
        for t in 0..3 {
            assert_eq!(fb.majors[t].len(), 3);
            if !fb.any[t] {
                assert_eq!(fb.majors[t].iter().filter(|x| x.selected).count(), 1, "tier {t}");
            }
            assert!(fb.majors[t].iter().all(|x| x.tile.key == SlotKey::Trait { line: 2, tier: t as u8 }));
        }
    }

    #[test]
    fn unchosen_tier_is_any() {
        let mut b = firebrand();
        b.specs[0].majors[1] = TraitSel::None;
        let l = Loadout::of(&b, &GameDb::default(), SpecDb::bundled());
        assert!(l.specs[0].any[1]);
        assert!(l.specs[0].majors[1].iter().all(|x| !x.selected));
    }

    #[test]
    fn two_handed_set_has_no_offhand_tile() {
        let mut b = firebrand();
        b.equipment.weapons.a1 = Some("greatsword".into());
        b.equipment.weapons.a2 = Some("shield".into());
        let l = Loadout::of(&b, &GameDb::default(), SpecDb::bundled());
        let a = l.weapons.iter().find(|w| w.label == "A").unwrap();
        assert!(a.two_handed && a.off.is_none());
        assert_eq!(a.main.tile.icon.as_deref(), crate::icons::weapon_icon("greatsword"));
    }

    #[test]
    fn unspecified_slots_are_empty_tiles() {
        let mut b = firebrand();
        b.equipment.relic = None;
        b.equipment.food = None;
        b.equipment.stats.remove(&GearSlot::Back);
        let l = Loadout::of(&b, &GameDb::default(), SpecDb::bundled());
        assert!(l.relic.empty && l.food.empty);
        assert!(l.trinkets[0].empty, "Back first");
    }

    #[test]
    fn named_relic_and_food_get_icons() {
        let b = firebrand();
        let l = Loadout::of(&b, &GameDb::default(), SpecDb::bundled());
        if b.equipment.food.is_some() {
            assert!(l.food.icon.is_some(), "food {:?}", b.equipment.food);
            assert!(l.food.sub.is_some());
        }
        if b.equipment.relic.is_some() {
            assert!(l.relic.icon.is_some(), "relic {:?}", b.equipment.relic);
        }
    }
}
```

- [ ] **Step 2: Run the tests and verify they fail.**

  Run: `cargo test -p axigear-core loadout`

- [ ] **Step 3: Implement** `Loadout::of`:

```rust
//! The comp's target build as tiles: what each slot wants and which icon to
//! draw. Pure, so the UI only lays it out.

use crate::gamedb::GameDb;
use crate::icons::{gear_icon, named, weapon_icon, weight_of, NamedKind};
use crate::model::{is_two_handed, Build, GearSlot, TraitSel};
use crate::report::SlotKey;
use crate::specs::SpecDb;

// (structs from Interfaces above)

fn tile(key: SlotKey, label: &str, name: Option<String>, icon: Option<String>) -> Tile {
    let empty = name.is_none();
    Tile { key, label: label.into(), name: name.unwrap_or_default(), sub: None, icon, empty }
}

fn short_label(slot: GearSlot) -> &'static str {
    match slot {
        GearSlot::Accessory1 => "Acc 1",
        GearSlot::Accessory2 => "Acc 2",
        GearSlot::WeaponA1 | GearSlot::WeaponB1 => "Main",
        GearSlot::WeaponA2 | GearSlot::WeaponB2 => "Off",
        s => s.label(),
    }
}

impl Loadout {
    pub fn of(build: &Build, db: &GameDb, specs: &SpecDb) -> Loadout {
        let e = &build.equipment;
        let weight = weight_of(&build.profession);
        let skill = |k: u8, label: &str, id: u32| {
            let name = (id != 0).then(|| build.skill_names.get(&id).cloned().or_else(|| db.skill_name(id).map(String::from)).unwrap_or_else(|| format!("skill {id}")));
            tile(SlotKey::Skill(k), label, name, db.skill_icon(id).map(String::from))
        };
        let s = &build.skills;
        let skills = vec![
            skill(0, "Heal", s.heal),
            skill(1, "Utility", s.utilities[0]),
            skill(2, "Utility", s.utilities[1]),
            skill(3, "Utility", s.utilities[2]),
            skill(4, "Elite", s.elite),
        ];
        let item = |key: SlotKey, label: &str, id: u32| {
            tile(key, label, Some(db.item_name(id).map(String::from).unwrap_or_else(|| format!("item {id}"))), db.item_icon(id).map(String::from))
        };
        let gear = |slot: GearSlot| tile(SlotKey::Gear(slot), short_label(slot), e.stats.get(&slot).cloned(), gear_icon(slot, weight).map(String::from));
        let armor = GearSlot::ARMOR.iter().map(|&slot| GearRow {
            tile: gear(slot),
            upgrades: e.runes.get(&slot).map(|id| vec![item(SlotKey::Rune(slot), "Rune", *id)]).unwrap_or_default(),
        }).collect();
        let weapon_row = |slot: GearSlot, w: &str| {
            let mut t = tile(SlotKey::Gear(slot), short_label(slot), Some(w.to_string()), weapon_icon(w).map(String::from));
            if let Some(stat) = e.stats.get(&slot) {
                t.sub = Some(stat.clone());
            }
            let upgrades = e.sigils.get(slot).iter().enumerate().map(|(i, id)| item(SlotKey::Sigil(slot, i as u8), "Sigil", *id)).collect();
            GearRow { tile: t, upgrades }
        };
        let mut weapons = Vec::new();
        for (label, main_slot, off_slot) in [("A", GearSlot::WeaponA1, GearSlot::WeaponA2), ("B", GearSlot::WeaponB1, GearSlot::WeaponB2)] {
            let (main, off) = (e.weapons.get(main_slot), e.weapons.get(off_slot));
            if main.is_none() && off.is_none() {
                continue;
            }
            let two_handed = main.is_some_and(is_two_handed);
            let main_row = match main {
                Some(w) => weapon_row(main_slot, w),
                None => GearRow { tile: tile(SlotKey::Gear(main_slot), "Main", None, None), upgrades: vec![] },
            };
            let off_row = if two_handed { None } else { off.map(|w| weapon_row(off_slot, w)) };
            weapons.push(WeaponSet { label, main: main_row, off: off_row, two_handed });
        }
        let trinkets = [GearSlot::Back, GearSlot::Accessory1, GearSlot::Accessory2, GearSlot::Amulet, GearSlot::Ring1, GearSlot::Ring2].map(gear).to_vec();
        let named_tile = |key: SlotKey, label: &str, kind: NamedKind, want: &Option<String>| {
            let hit = want.as_deref().and_then(|n| named(kind, n));
            let mut t = tile(key, label, want.clone(), hit.map(|h| h.icon.clone()));
            t.sub = hit.map(|h| h.buff.clone()).filter(|b| !b.is_empty());
            t
        };
        let mut relic = named_tile(SlotKey::Relic, "Relic", NamedKind::Relic, &e.relic);
        relic.name = relic.name.trim_start_matches("Relic of the ").trim_start_matches("Relic of ").to_string();
        let infusions = e.infusions.iter().map(|id| item(SlotKey::Infusions, "Infusion", *id)).collect();
        let specs_cards = build.specs.iter().enumerate().filter(|(_, l)| l.id != 0).map(|(i, line)| {
            let info = specs.get(line.id);
            let tinfo = |t: u32| specs.trait_info(line.id, t);
            let minors = info.map(|s| s.minors.clone()).unwrap_or_default().into_iter().map(|t| {
                tile(SlotKey::Spec(i as u8), "Minor", tinfo(t).map(|x| x.name.clone()), tinfo(t).map(|x| x.icon.clone()))
            }).collect();
            let mut any = [false; 3];
            let majors: [Vec<TraitTile>; 3] = std::array::from_fn(|tier| {
                let choices = info.map(|s| s.majors[tier].clone()).unwrap_or_default();
                let chosen = match line.majors[tier] {
                    TraitSel::None => None,
                    TraitSel::Id(id) => Some(id),
                    TraitSel::Position(p) => specs.trait_at(line.id, tier + 1, p),
                };
                any[tier] = matches!(line.majors[tier], TraitSel::None);
                choices.into_iter().map(|t| TraitTile {
                    tile: tile(SlotKey::Trait { line: i as u8, tier: tier as u8 }, "Major", tinfo(t).map(|x| x.name.clone()), tinfo(t).map(|x| x.icon.clone())),
                    selected: chosen == Some(t),
                }).collect()
            });
            SpecCard {
                key: SlotKey::Spec(i as u8),
                name: specs.name(line.id).map(String::from).unwrap_or_else(|| format!("spec {}", line.id)),
                icon: info.map(|s| s.icon.clone()).filter(|u| !u.is_empty()),
                background: info.map(|s| s.background.clone()).filter(|u| !u.is_empty()),
                minors,
                majors,
                any,
            }
        }).collect();
        Loadout {
            skills,
            specs: specs_cards,
            armor,
            weapons,
            trinkets,
            relic,
            infusions,
            food: named_tile(SlotKey::Food, "Food", NamedKind::Food, &e.food),
            utility: named_tile(SlotKey::Utility, "Utility", NamedKind::Utility, &e.utility),
        }
    }
}
```

  `std::array::from_fn` borrows `any` mutably inside the closure. If the borrow checker objects, compute `any` in a separate loop first. If `firebrand()` has no `"Firebrand"` in line 2, adapt the test to the fixture's actual elite.

- [ ] **Step 4: Run the tests and verify they pass.**

  Run: `cargo test -p axigear-core`

  Expected: all pass.

- [ ] **Step 5: Commit.**

```bash
git add crates/axigear-core/src/loadout.rs crates/axigear-core/src/lib.rs
git commit -m "feat(core): loadout view model for the target build

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 8: Snapshot wiring and the `loadout_tab` setting

**Files:**
- Modify: `crates/axigear-core/src/session.rs` (add `Session::loadout`)
- Modify: `crates/axigear-core/src/driver.rs` (`UiSnapshot.loadout`, `SettingsPatch::LoadoutTab`)
- Modify: `crates/axigear-core/src/settings.rs` (`loadout_tab: Tab`)

**Interfaces:**
- Produces:
  - `UiSnapshot.loadout: Option<Loadout>`
  - `Settings.loadout_tab: report::Tab` (serde default `Build`)
  - `SettingsPatch::LoadoutTab(Tab)`
  - `Session::loadout(&self, specs: &SpecDb) -> Option<Loadout>`

- [ ] **Step 1: Write failing tests.** Add a driver test next to the existing `SettingsPatch` tests near `driver.rs:741`, copying their setup (`Driver::new` with a temp dir and `FakeHttp`). Mirror the closest existing test's construction exactly; read lines 700–760 first.

```rust
#[test]
fn loadout_tab_is_a_saved_setting() {
    let (mut d, t0) = /* same setup as the hotkey test at ~815 */;
    assert_eq!(d.snapshot(t0).settings.loadout_tab, crate::report::Tab::Build);
    d.handle(Command::Settings(SettingsPatch::LoadoutTab(crate::report::Tab::Equipment)), t0);
    assert_eq!(d.snapshot(t0).settings.loadout_tab, crate::report::Tab::Equipment);
}

#[test]
fn snapshot_carries_the_assigned_builds_loadout() {
    let (d, t0) = /* setup of an existing test that loads a comp and gets a report, e.g. one asserting snapshot().report.is_some() */;
    let snap = d.snapshot(t0);
    assert!(snap.report.is_some());
    let l = snap.loadout.expect("loadout with a report");
    assert_eq!(l.skills.len(), 5);
}
```

  Also add a settings test: `serde_json::from_str::<Settings>("{}")` has `loadout_tab == Tab::Build`.

- [ ] **Step 2: Run the tests and verify they fail.**

  Run: `cargo test -p axigear-core driver settings`

- [ ] **Step 3: Implement.**
  - `settings.rs`: add `pub loadout_tab: crate::report::Tab`, and `loadout_tab: Tab::Build` in `Default`.
  - `driver.rs`:
    - Add `LoadoutTab(Tab)` to `SettingsPatch`, and the match arm `SettingsPatch::LoadoutTab(t) => self.settings.loadout_tab = t,`.
    - Add the `loadout: Option<Loadout>` field.
    - In `snapshot()`, set `loadout: self.session.loadout(specs)`.
  - `session.rs`:

```rust
pub fn loadout(&self, specs: &SpecDb) -> Option<crate::loadout::Loadout> {
    let lc = self.comp.as_ref()?;
    let slot = self.assignment.slot()?;
    Some(crate::loadout::Loadout::of(&lc.comp.builds[slot.build], &self.db, specs))
}
```

  Check whether `UiSnapshot` is constructed anywhere else (`grep -rn "UiSnapshot {"`), and add `loadout: None` there.

- [ ] **Step 4: Run the tests and verify they pass.**

  Run: `cargo test -p axigear-core && cargo test -p arcdps_axigear`

  Expected: all pass.

- [ ] **Step 5: Commit.**

```bash
git add crates/axigear-core/src/session.rs crates/axigear-core/src/driver.rs crates/axigear-core/src/settings.rs
git commit -m "feat(core): expose the loadout and saved tab in the UI snapshot

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 9: Texture cache (ported from arcdps-axipulse)

**Files:**
- Modify: `crates/axigear/Cargo.toml`
- Create: `crates/axigear/src/ui/texture_rules.rs` (host-compiled), `crates/axigear/src/ui/textures.rs` (`#[cfg(windows)]`)
- Modify: `crates/axigear/src/ui/mod.rs`, `crates/axigear/src/plugin.rs`

**Interfaces:**
- Produces:
  - `texture_rules::allowed(url: &str) -> bool`
  - `texture_rules::cache_name(url: &str) -> String`, giving `"{:016x}.img"` of the FNV-1a 64-bit hash of the URL.
  - `textures::get(url: &str) -> Option<IconHandle>` with `IconHandle { tex: TextureId, aspect: f32 }`.
  - `textures::drain_pending()`.

- [ ] **Step 1: Write failing tests.** Create `texture_rules.rs` with its tests:

```rust
//! Which icon URLs we fetch and where they're cached. Host-testable.

pub const HOSTS: [&str; 2] = ["https://render.guildwars2.com/", "https://wiki.guildwars2.com/"];

pub fn allowed(url: &str) -> bool {
    HOSTS.iter().any(|h| url.starts_with(h) && url.len() > h.len())
}

/// FNV-1a 64: stable across runs and Rust versions (unlike DefaultHasher).
pub fn cache_name(url: &str) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in url.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{h:016x}.img")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_gw2_hosts_are_allowed() {
        assert!(allowed("https://render.guildwars2.com/file/ABC/1.png"));
        assert!(allowed("https://wiki.guildwars2.com/images/b/b5/Bandit_Cleaver.png"));
        assert!(!allowed("http://render.guildwars2.com/file/ABC/1.png"));
        assert!(!allowed("https://render.guildwars2.com/"));
        assert!(!allowed(""));
        assert!(!allowed("https://evil.example/render.guildwars2.com/x.png"));
        assert!(!allowed("https://render.guildwars2.com.evil.example/x.png"));
    }

    #[test]
    fn cache_names_are_stable_and_distinct() {
        assert_eq!(cache_name("a"), "af63dc4c8601ec8c.img");
        assert_ne!(cache_name("https://render.guildwars2.com/1"), cache_name("https://render.guildwars2.com/2"));
    }
}
```

  `https://render.guildwars2.com.evil.example/` does **not** start with `https://render.guildwars2.com/`, because of the trailing slash, so it is rejected. The test pins this. `"af63dc4c8601ec8c"` is the published FNV-1a 64 hash of `"a"`.

  In `ui/mod.rs`, add `pub mod texture_rules;` (not cfg-gated) and `#[cfg(windows)] pub mod textures;`.

- [ ] **Step 2: Run the tests and verify they pass.**

  Run: `cargo test -p arcdps_axigear texture_rules`

  Expected: PASS. These are pure functions written test-first in a single step. Confirm that both tests ran.

- [ ] **Step 3: Add the dependencies.** In `crates/axigear/Cargo.toml`:
  - Under `[target.'cfg(windows)'.dependencies]`, add `image = { version = "0.25", default-features = false, features = ["png", "jpeg"] }`.
  - Extend the windows features with `"Win32_Graphics_Direct3D"`, `"Win32_Graphics_Direct3D11"`, `"Win32_Graphics_Dxgi"` and `"Win32_Graphics_Dxgi_Common"`.

- [ ] **Step 4: Port `textures.rs`.** Start from `/var/home/mstephens/Documents/GitHub/arcdps-axipulse/src/ui/icons.rs` and adapt it:
  - Remove `IconKind`, `IconKey`, the `BUNDLED_*` items and `FightData`. Key everything by `String` URL.
  - Keep `State`, `Cache { by_key: HashMap<String, State>, _srvs }`, `Decoded`, `decode`, `Chan`, `MAX_UPLOADS_PER_FRAME = 4`, `create_srv`, and the single worker thread (named `"axigear-icon-worker"`).
  - Disk path: `crate::paths::data_dir().join("icons").join(texture_rules::cache_name(url))`.
  - Worker:
    1. If the file exists and decodes, send the result.
    2. If it exists but doesn't decode, delete it and fall through to the download.
    3. On download, write `<path>.tmp` then `std::fs::rename` to `<path>`, then decode and send.
    4. ureq timeout 20 s.
  - `pub fn get(url: &str) -> Option<IconHandle>`:
    1. If `!texture_rules::allowed(url)`, return `None` without caching anything, so a URL is never fetched.
    2. Look the URL up in the cache: `Ready` gives `Some`, `Pending`/`Failed` gives `None`.
    3. Otherwise insert `Pending`, send the request, and return `None`.
  - `pub fn drain_pending()`:
    - If `arcdps::d3d11_device()` is `None`, return immediately. The UI then stays on text tiles.
    - Otherwise upload at most 4 per frame. On error, `log::warn!("axigear icon: {url}: {e}")` and mark the URL `Failed`.
  - Module doc: one paragraph saying where it came from and the Wine upload cap.

- [ ] **Step 5: Drain each frame.** In `plugin.rs` `imgui()`, inside the `guard("imgui", …)` closure and before `badge::render`, call `crate::ui::textures::drain_pending();`.

- [ ] **Step 6: Check that the Windows build compiles.**

  Run: `cargo dll-check`

  Expected: finishes with no errors. Warnings for `get` being unused are OK until Task 10.

- [ ] **Step 7: Commit.**

```bash
git add crates/axigear/Cargo.toml Cargo.lock crates/axigear/src/ui/texture_rules.rs crates/axigear/src/ui/textures.rs crates/axigear/src/ui/mod.rs crates/axigear/src/plugin.rs
git commit -m "feat(ui): icon texture cache ported from axipulse

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 10: Tile primitive

**Files:**
- Create: `crates/axigear/src/ui/tile.rs` (`#[cfg(windows)]`)
- Modify: `crates/axigear/src/ui/theme.rs` (add `GOLD`, `TINT_DIM`), `crates/axigear/src/ui/mod.rs`

**Interfaces:**
- Consumes:
  - `textures::get` (Task 9)
  - `Tile` and `TraitTile` (Task 7)
  - `CheckReport::marks_for` and `worst` (Task 4)
  - `icons::draw` and `ink` (existing)
  - `axi::{card, outline_on, truncate_to_width, Rect}` (existing)
- Produces:

```rust
pub struct TileStyle { pub size: f32, pub faded: bool, pub underline: bool, pub pulse: bool, pub skip: &'static [Category] }

/// Draws an icon tile at the cursor (reserving `size`×`size`), its status chip,
/// and its tooltip. Returns true if hovered.
pub fn icon(ui: &Ui, t: &Tile, report: Option<&CheckReport>, style: TileStyle) -> bool;

/// Hover tooltip shared by tiles and rows.
pub fn tooltip(ui: &Ui, t: &Tile, report: Option<&CheckReport>);

/// Text clipped to `w` at the cursor, in `color`.
pub fn clipped_text(ui: &Ui, text: &str, w: f32, color: [f32; 4]);
```

  Also add to `theme.rs`:
  - `pub const GOLD: [f32; 4] = rgb(0xff, 0xc5, 0x3d); // --axi-gold`
  - `pub const TINT_DIM: [f32; 4] = [0.35, 0.35, 0.35, 1.0];`
  - `pub const TINT_FULL: [f32; 4] = [1.0, 1.0, 1.0, 1.0];`

- [ ] **Step 1: Implement `tile.rs`.**

```rust
//! One tile: icon (or text fallback), ink outline, status chip, tooltip.

use arcdps::imgui::Ui;
use axigear_core::loadout::Tile;
use axigear_core::report::{Category, CheckReport, Source, Tone};
use axigear_core::text;

use super::axi::{self, Rect};
use super::{icons, textures, theme};

pub struct TileStyle {
    pub size: f32,
    pub faded: bool,
    pub underline: bool,
    pub pulse: bool,
    pub skip: &'static [Category],
}

impl TileStyle {
    pub fn new(size: f32) -> Self {
        TileStyle { size, faded: false, underline: false, pulse: false, skip: &[] }
    }
}

const CHIP: f32 = 14.0;

pub fn icon(ui: &Ui, t: &Tile, report: Option<&CheckReport>, style: TileStyle) -> bool {
    let origin = ui.cursor_screen_pos();
    let r = Rect::at(origin, [style.size, style.size]);
    ui.invisible_button(format!("##tile-{:?}-{}-{}", t.key, t.name, origin[0] as i32), [style.size, style.size]);
    let hovered = ui.is_item_hovered();
    let tone = report.and_then(|rep| rep.worst(t.key, style.skip));
    let tex = t.icon.as_deref().and_then(textures::get);
    {
        let draw = ui.get_window_draw_list();
        draw.add_rect(r.min, r.max, theme::SURFACE_RAISED).filled(true).build();
        match tex {
            Some(h) => {
                let tint = if style.faded || t.empty { theme::TINT_DIM } else { theme::TINT_FULL };
                draw.add_image(h.tex, r.min, r.max).col(tint).build();
            }
            None => {
                let label = axi::truncate_to_width(if t.name.is_empty() { &t.label } else { &t.name }, style.size - 4.0, |s| ui.calc_text_size(s)[0]);
                draw.add_text([r.min[0] + 2.0, r.min[1] + 2.0], theme::TEXT_FAINT, label);
            }
        }
        let border = match tone {
            Some(Tone::Danger) => theme::DANGER,
            Some(Tone::Warn) => theme::WARN,
            _ if style.pulse => theme::GOLD,
            _ => theme::INK_LINE,
        };
        if t.empty {
            dashed(&draw, r, theme::RULE);
        } else {
            axi::outline_on(&draw, r, if tone.is_some_and(|x| x.rank() < 2) || style.pulse { theme::BORDER_CONTROL } else { theme::BORDER_HAIRLINE }, border);
        }
        if style.underline {
            draw.add_rect([r.min[0], r.max[1] + 2.0], [r.max[0], r.max[1] + 5.0], theme::GOLD).filled(true).build();
        }
    }
    if let Some(tone) = tone.filter(|t| *t != Tone::Ok) {
        let saved = ui.cursor_screen_pos();
        ui.set_cursor_screen_pos([r.max[0] - CHIP, r.min[1]]);
        ui.get_window_draw_list().add_rect([r.max[0] - CHIP, r.min[1]], [r.max[0], r.min[1] + CHIP], theme::INK_LINE).filled(true).build();
        icons::draw(ui, tone, CHIP);
        ui.set_cursor_screen_pos(saved);
    }
    if hovered {
        tooltip(ui, t, report);
    }
    hovered
}

fn dashed(draw: &arcdps::imgui::DrawListMut, r: Rect, ink: [f32; 4]) {
    let (dash, gap) = (4.0, 3.0);
    let mut x = r.min[0];
    while x < r.max[0] {
        let x2 = (x + dash).min(r.max[0]);
        draw.add_line([x, r.min[1]], [x2, r.min[1]], ink).thickness(1.0).build();
        draw.add_line([x, r.max[1]], [x2, r.max[1]], ink).thickness(1.0).build();
        x += dash + gap;
    }
    let mut y = r.min[1];
    while y < r.max[1] {
        let y2 = (y + dash).min(r.max[1]);
        draw.add_line([r.min[0], y], [r.min[0], y2], ink).thickness(1.0).build();
        draw.add_line([r.max[0], y], [r.max[0], y2], ink).thickness(1.0).build();
        y += dash + gap;
    }
}

pub fn tooltip(ui: &Ui, t: &Tile, report: Option<&CheckReport>) {
    ui.tooltip(|| {
        ui.text(if t.name.is_empty() { &t.label } else { &t.name });
        if t.empty {
            ui.text_colored(theme::TEXT_FAINT, "Not set in the comp");
        } else {
            ui.text_colored(theme::TEXT_DIM, format!("Comp: {}", t.name));
        }
        if let Some(sub) = &t.sub {
            ui.text_colored(theme::TEXT_FAINT, sub);
        }
        for (r, m) in report.map(|rep| rep.marks_for(t.key)).unwrap_or_default() {
            let source = match r.source {
                Source::Live => "live",
                Source::Api => "GW2 API",
            };
            let age = r.age_secs.map(|s| format!(" · {}", text::ago(s))).unwrap_or_default();
            let you = m.detail.as_deref().map(|d| format!(" · You: {d}")).unwrap_or_default();
            ui.text_colored(icons::ink(r.mark_tone(m)), format!("{}{you} ({source}{age})", r.label));
        }
    });
}

pub fn clipped_text(ui: &Ui, text: &str, w: f32, color: [f32; 4]) {
    let s = axi::truncate_to_width(text, w, |s| ui.calc_text_size(s)[0]);
    ui.text_colored(color, s);
}
```

  The vendored imgui-rs fork may differ in a few calls. Check `grep -n "fn add_image\|fn col\|DrawListMut" ~/.cargo/git/checkouts/imgui-rs-*/*/imgui/src/draw_list.rs`, and compare with axipulse's `map.rs:552`, which calls `add_image(...).col(...)`. `outline_on` takes `(draw, r, thickness, ink)`, per `axi.rs:254`.

- [ ] **Step 2: Check that the Windows build compiles.**

  Run: `cargo dll-check`

  Expected: OK.

- [ ] **Step 3: Commit.**

```bash
git add crates/axigear/src/ui/tile.rs crates/axigear/src/ui/theme.rs crates/axigear/src/ui/mod.rs
git commit -m "feat(ui): icon tile with status chip, fallback and tooltip

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 11: Window shell, problems panel and tabs

**Files:**
- Create: `crates/axigear/src/ui/problems.rs` (`#[cfg(windows)]`), `crates/axigear/src/ui/focus.rs` (host-compiled)
- Modify: `crates/axigear/src/ui/checklist.rs`, `crates/axigear/src/ui/state.rs`, `crates/axigear/src/ui/mod.rs`

**Interfaces:**
- Consumes:
  - `UiSnapshot.loadout` and `settings.loadout_tab` (Task 8)
  - `SettingsPatch::LoadoutTab` (Task 8)
  - `SlotKey::tab` (Task 4)
- Produces:
  - `focus::Focus { key: SlotKey, until: f64 }` with `focus::Focus::active(&self, now: f64) -> bool`.
  - `focus::target(r: &CheckResult) -> Option<SlotKey>`: the first non-pass mark, else the first mark.
  - `UiState.focus: Option<Focus>` and `UiState.tab: Option<Tab>`. The latter is a local echo of the setting until the snapshot catches up.
  - `problems::render(ui, report, state)`.
  - `build_tab::render` and `equipment_tab::render` are called by `checklist.rs`; they arrive in Tasks 12 and 13. Until then, add stub modules that each render `ui.text("…")`, so this task compiles on its own.

- [ ] **Step 1: Write failing tests** in a new `crates/axigear/src/ui/focus.rs`:

```rust
//! Problem click → which slot to pulse. Host-testable.

use axigear_core::report::{CheckResult, SlotKey, Status};

pub const PULSE_SECS: f64 = 1.5;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Focus {
    pub key: SlotKey,
    pub until: f64,
}

impl Focus {
    pub fn active(&self, now: f64) -> bool {
        now < self.until
    }
}

/// The slot a problem line jumps to: its first non-passing mark, else its first mark.
pub fn target(r: &CheckResult) -> Option<SlotKey> {
    r.marks.iter().find(|m| m.status != Status::Pass).or(r.marks.first()).map(|m| m.key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axigear_core::model::GearSlot;
    use axigear_core::report::{Category, Tab};

    #[test]
    fn target_prefers_the_failing_mark() {
        let r = CheckResult::new(Category::Runes, "runes", "Runes", Status::Fail, "x")
            .mark(SlotKey::Rune(GearSlot::Head), Status::Pass, None)
            .mark(SlotKey::Rune(GearSlot::Feet), Status::Fail, None);
        assert_eq!(target(&r), Some(SlotKey::Rune(GearSlot::Feet)));
        assert_eq!(target(&r).unwrap().tab(), Tab::Equipment);
        assert_eq!(target(&CheckResult::new(Category::Spec, "spec", "Spec", Status::Fail, "x")), None);
    }

    #[test]
    fn pulse_expires() {
        let f = Focus { key: SlotKey::Relic, until: 10.0 + PULSE_SECS };
        assert!(f.active(11.0) && !f.active(11.5));
    }
}
```

  In `mod.rs`, add `pub mod focus;`, ungated.

- [ ] **Step 2: Run the tests and verify they pass.**

  Run: `cargo test -p arcdps_axigear focus`

  Expected: PASS. These are pure helpers written with their tests; confirm both ran.

- [ ] **Step 3: Add the state fields.** In `state.rs`, add `pub focus: Option<super::focus::Focus>` and `pub tab: Option<axigear_core::report::Tab>`. In `sync`, clear `tab` once `snap.settings.loadout_tab == tab`, the same pattern as `badge_edit`.

- [ ] **Step 4: Write `problems.rs`.**

```rust
//! Failing and warning checks, one line each; click jumps to the slot.

use arcdps::imgui::Ui;
use axigear_core::driver::{Command, SettingsPatch};
use axigear_core::report::{CheckReport, Status, Tone};

use super::focus::{self, Focus, PULSE_SECS};
use super::state::UiState;
use super::{icons, theme};
use crate::plugin::send;

pub fn render(ui: &Ui, report: &CheckReport, state: &mut UiState) {
    let line = ui.text_line_height();
    let mut problems: Vec<_> = report.results.iter().filter(|r| matches!(r.tone(), Tone::Danger | Tone::Warn)).collect();
    problems.sort_by_key(|r| r.tone().rank());
    let unknown: Vec<_> = report.results.iter().filter(|r| r.status == Status::Unknown).collect();
    if problems.is_empty() {
        icons::draw(ui, Tone::Ok, line);
        ui.same_line();
        ui.text_colored(theme::TEXT_DIM, format!("All {} checks pass", report.summary().decided()));
    }
    for (i, r) in problems.iter().enumerate() {
        icons::draw(ui, r.tone(), line);
        ui.same_line();
        let clicked = ui.selectable(format!("{}  ##problem{i}", r.label));
        ui.same_line();
        ui.text_colored(theme::TEXT_DIM, r.detail());
        if clicked {
            if let Some(key) = focus::target(r) {
                let tab = key.tab();
                state.tab = Some(tab);
                send(Command::Settings(SettingsPatch::LoadoutTab(tab)));
                state.focus = Some(Focus { key, until: ui.time() + PULSE_SECS });
            }
        }
    }
    if !unknown.is_empty() {
        icons::draw(ui, Tone::Neutral, line);
        ui.same_line();
        ui.text_colored(theme::TEXT_FAINT, format!("{} waiting for data", unknown.len()));
        if ui.is_item_hovered() {
            ui.tooltip(|| {
                for r in &unknown {
                    ui.text_colored(theme::TEXT_DIM, format!("{} · {}", r.label, r.detail()));
                }
            });
        }
    }
}
```

  `ui.time()` returns `f64` seconds in imgui-rs 0.13. If the fork lacks it, use `ui.io().delta_time` to accumulate a time into `UiState`. Check with `grep -rn "fn time" ~/.cargo/git/checkouts/imgui-rs-*/*/imgui/src/lib.rs`.

- [ ] **Step 5: Rebuild `checklist.rs`.**
  - Keep `header()` as it is, but tighten it to two rows:
    - Row 1: title (comp name + source), offline marker, Refresh.
    - Row 2: slot label, picker button, then `api_line` + Refresh API on the same line.
  - Then call `render` as below. Delete `results()` and `row()`; the problems panel and tile tooltips replace them.

```rust
pub fn render(ui: &Ui, snap: &UiSnapshot, state: &mut UiState) {
    if !state.checklist_open {
        return;
    }
    let _form = theme::push_form(ui);
    let _bg = ui.push_style_color(StyleColor::WindowBg, theme::GROUND);
    let _text = ui.push_style_color(StyleColor::Text, theme::TEXT);
    let mut open = true;
    ui.window("axigear")
        .opened(&mut open)
        .size([540.0, 640.0], Condition::FirstUseEver)
        .size_constraints([520.0, 200.0], [f32::MAX, f32::MAX])
        .flags(WindowFlags::NO_COLLAPSE)
        .build(|| {
            header(ui, snap);
            ui.separator();
            let (Some(report), Some(loadout)) = (&snap.report, &snap.loadout) else {
                ui.text_colored(theme::TEXT_DIM, snap.header.note.as_deref().unwrap_or("Load a comp in arcdps options (Alt+Shift+T) > Extensions > axigear."));
                return;
            };
            super::problems::render(ui, report, state);
            ui.separator();
            let tab = state.tab.unwrap_or(snap.settings.loadout_tab);
            let origin = ui.cursor_screen_pos();
            let (build_clicked, w) = super::axi::chip(ui, origin, "tab-build", "BUILD", tab == Tab::Build, theme::GOLD, [10.0, 4.0]);
            let (equip_clicked, _) = super::axi::chip(ui, [origin[0] + w + 6.0, origin[1]], "tab-equip", "EQUIPMENT", tab == Tab::Equipment, theme::GOLD, [10.0, 4.0]);
            for (clicked, t) in [(build_clicked, Tab::Build), (equip_clicked, Tab::Equipment)] {
                if clicked && t != tab {
                    state.tab = Some(t);
                    send(Command::Settings(SettingsPatch::LoadoutTab(t)));
                }
            }
            ui.dummy([0.0, 8.0]);
            let focus = state.focus.filter(|f| f.active(ui.time())).map(|f| f.key);
            match state.tab.unwrap_or(snap.settings.loadout_tab) {
                Tab::Build => super::build_tab::render(ui, loadout, report, focus),
                Tab::Equipment => super::equipment_tab::render(ui, loadout, report, focus),
            }
        });
    state.checklist_open = open;
}
```

  Create the stubs `build_tab.rs` and `equipment_tab.rs`, each `#[cfg(windows)]`, with `pub fn render(ui: &Ui, _l: &Loadout, _r: &CheckReport, _focus: Option<SlotKey>) { ui.text("…"); }`. Register them in `mod.rs` under `#[cfg(windows)]`.

  `axi::chip` already uses the selected accent fill with ink text: a filled gold chip with an ink label. That is the "segmented control" look; the spec's "gold underline" is satisfied by the filled accent. Ruling: keep the existing chip primitive instead of building a new one.

- [ ] **Step 6: Check that the Windows build compiles, and run the host tests.**

  Run: `cargo dll-check && cargo test -p arcdps_axigear`

  Expected: OK.

- [ ] **Step 7: Commit.**

```bash
git add crates/axigear/src/ui/
git commit -m "feat(ui): problems panel and Build/Equipment tabs

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 12: Build tab

**Files:**
- Modify: `crates/axigear/src/ui/build_tab.rs`

**Interfaces:**
- Consumes:
  - `tile::{icon, TileStyle, clipped_text}` (Task 10)
  - `Loadout.skills` and `specs` (Task 7)
  - `CheckReport::worst` (Task 4)
  - `theme::{GOLD, DANGER, SURFACE, TINT_DIM}`
  - `textures::get`

**Layout:**

| Element | Size / spacing |
|---|---|
| Skill tiles | 48 px, in a row with 8 px gaps |
| Unseen-skill dot | 4 px, centred under the tile |
| Spec card width | content width (≥ 500) |
| Spec card height | 110 px |
| Emblem | 56 px |
| Minors | 26 px, vertically centred |
| Major column | 3 × 32 px icons stacked with 3 px gaps, plus 5 px for the underline |
| Gaps between card elements | 10 px |

The skill row is drawn as `EYEBROW "SKILLS"`, then the tiles. A tile has a Danger/Warn chip from `worst(key, &[Category::SkillsSeen])`. The "not cast yet" dot shows when `report.marks_for(key)` has a `SkillsSeen` mark with `Status::Unknown`.

Each spec card draws, in order:
1. A `SURFACE` panel (`axi::card`).
2. The background image (`textures::get(background)`) drawn into the card rect with `TINT_DIM` and alpha 0.35, i.e. `[0.35, 0.35, 0.35, 0.35]`.
3. The eyebrow name in `GOLD`, uppercase, at the top-left.
4. The emblem, then minor, major column, minor, major column, minor, major column.
5. Major tiles use `TileStyle { size: 32, faded: !selected && !any, underline: selected, pulse: focus == Some(key), skip: &[] }`.
6. Minor tiles use size 26 and are never faded.
7. If `report.worst(card.key, &[])` is `Danger`, a 3 px `DANGER` outline around the whole card.

- [ ] **Step 1: Implement `render(ui, l, report, focus)`.**

```rust
//! Build tab: skill bar, then a card per specialization line.

use arcdps::imgui::Ui;
use axigear_core::loadout::{Loadout, SpecCard};
use axigear_core::report::{Category, CheckReport, SlotKey, Status, Tone};

use super::axi::{self, Rect};
use super::tile::{self, TileStyle};
use super::{textures, theme};

const SKILL: f32 = 48.0;
const MAJOR: f32 = 32.0;
const MINOR: f32 = 26.0;
const EMBLEM: f32 = 56.0;
const CARD_H: f32 = 110.0;
const GAP: f32 = 10.0;

pub fn render(ui: &Ui, l: &Loadout, report: &CheckReport, focus: Option<SlotKey>) {
    eyebrow(ui, "Skills");
    let start = ui.cursor_screen_pos();
    for (i, t) in l.skills.iter().enumerate() {
        ui.set_cursor_screen_pos([start[0] + i as f32 * (SKILL + 8.0), start[1]]);
        tile::icon(ui, t, Some(report), TileStyle { pulse: focus == Some(t.key), skip: &[Category::SkillsSeen], ..TileStyle::new(SKILL) });
        let unseen = report.marks_for(t.key).iter().any(|(r, m)| r.category == Category::SkillsSeen && m.status == Status::Unknown);
        if unseen {
            let c = [start[0] + i as f32 * (SKILL + 8.0) + SKILL / 2.0, start[1] + SKILL + 5.0];
            ui.get_window_draw_list().add_circle(c, 2.0, theme::TEXT_FAINT).filled(true).build();
        }
    }
    ui.set_cursor_screen_pos([start[0], start[1] + SKILL + 14.0]);
    eyebrow(ui, "Specializations");
    for card in &l.specs {
        spec_card(ui, card, report, focus);
        ui.dummy([0.0, 8.0]);
    }
}

fn eyebrow(ui: &Ui, s: &str) {
    ui.text_colored(theme::GOLD, s.to_uppercase());
}

fn spec_card(ui: &Ui, c: &SpecCard, report: &CheckReport, focus: Option<SlotKey>) {
    let o = ui.cursor_screen_pos();
    let w = ui.content_region_avail()[0].max(500.0);
    let r = Rect::at(o, [w, CARD_H]);
    axi::card(ui, r, theme::SURFACE, false);
    if let Some(bg) = c.background.as_deref().and_then(textures::get) {
        ui.get_window_draw_list().add_image(bg.tex, r.min, r.max).col([0.35, 0.35, 0.35, 0.35]).build();
    }
    if report.worst(c.key, &[]) == Some(Tone::Danger) {
        axi::outline_on(&ui.get_window_draw_list(), r, theme::BORDER_CONTROL, theme::DANGER);
    }
    ui.get_window_draw_list().add_text([o[0] + 8.0, o[1] + 6.0], theme::GOLD, c.name.to_uppercase());
    let mid = o[1] + 22.0 + (CARD_H - 22.0) / 2.0;
    let mut x = o[0] + 8.0;
    // Emblem
    ui.set_cursor_screen_pos([x, mid - EMBLEM / 2.0]);
    let emblem = axigear_core::loadout::Tile { key: c.key, label: "Spec".into(), name: c.name.clone(), sub: None, icon: c.icon.clone(), empty: false };
    tile::icon(ui, &emblem, Some(report), TileStyle { pulse: focus == Some(c.key), ..TileStyle::new(EMBLEM) });
    x += EMBLEM + GAP;
    for tier in 0..3 {
        if let Some(minor) = c.minors.get(tier) {
            ui.set_cursor_screen_pos([x, mid - MINOR / 2.0]);
            tile::icon(ui, minor, None, TileStyle::new(MINOR));
            x += MINOR + GAP;
        }
        let col_h = 3.0 * MAJOR + 2.0 * 3.0;
        for (j, t) in c.majors[tier].iter().enumerate() {
            ui.set_cursor_screen_pos([x, mid - col_h / 2.0 + j as f32 * (MAJOR + 3.0)]);
            tile::icon(ui, &t.tile, Some(report), TileStyle {
                faded: !t.selected && !c.any[tier],
                underline: t.selected,
                pulse: focus == Some(t.tile.key) && t.selected,
                ..TileStyle::new(MAJOR)
            });
        }
        x += MAJOR + GAP;
    }
    ui.set_cursor_screen_pos([o[0], o[1] + CARD_H]);
    ui.dummy([w, 0.0]);
}
```

  Minor tiles share the line's `Spec(i)` key. They are passed `report: None`, so they never carry the line's chip; the emblem carries it. Every major tile in a tier shares the `Trait{line,tier}` key. To keep only the selected major showing the red outline, pass `Some(report)` only when `t.selected || c.any[tier]`, and `None` otherwise. Apply that change to the snippet: replace `Some(report)` in the major `tile::icon` call with `(t.selected || c.any[tier]).then_some(report)`.

- [ ] **Step 2: Check that the Windows build compiles.**

  Run: `cargo dll-check`

  Expected: OK.

- [ ] **Step 3: Commit.**

```bash
git add crates/axigear/src/ui/build_tab.rs
git commit -m "feat(ui): build tab with skill bar and spec cards

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 13: Equipment tab

**Files:**
- Modify: `crates/axigear/src/ui/equipment_tab.rs`

**Interfaces:**
- Consumes:
  - `tile::{icon, tooltip, TileStyle, clipped_text}` (Task 10)
  - `Loadout.armor`, `weapons`, `trinkets`, `relic`, `infusions`, `food`, `utility` (Task 7)

**Layout:**

Two columns: `col_w = (avail - 12) / 2`. The left column starts at the cursor x, the right at `x + col_w + 12`. Track each column's y separately, and finish with the cursor at the max y.

| Element | Spec |
|---|---|
| Row | 40 px icon tile, then 8 px gap, then a text block: line 1 the uppercase faint label, line 2 the name, clipped to the space left after the upgrade chips. Upgrade chips are 24 px each with 3 px gaps, right-aligned and vertically centred. Row height 44. |
| Armor | `eyebrow("Armor")`, then 6 rows. A row with no stat is drawn by its tile as empty; the name text becomes "—". |
| Weapons | `eyebrow("Weapons")`. For each set, a faint `"SET A"` / `"SET B"` label, then the main row. If `two_handed`, a faded "Two-Handed" line in `TEXT_FAINT`; else the off row when present. A weapon row's second text line is `sub` (the stat), when present. |
| Trinkets | `eyebrow("Trinkets")`. A 4-wide grid of 32 px tiles: `trinkets[0..3]` (Back, Acc 1, Acc 2) + `relic`, then a second row of `trinkets[3..6]` (Amulet, Ring 1, Ring 2). Under each tile: a 9-px-ish faint label (`ui.set_window_font_scale(0.8)` then reset) and the name clipped to the cell width. The cell is `col_w / 4`. |
| Infusions | `eyebrow("Infusions")` with the status chip of `worst(SlotKey::Infusions, &[])` drawn after it via `icons::draw`, then 24 px tiles in rows that wrap at `col_w`. Infusion tiles pass `report: None`, so per-tile chips don't repeat the build-wide status. When there are no infusions, show a faint "none set". |
| Consumables | `eyebrow("Consumables")`, then food and utility rows. Their second line is `sub` (the buff text), clipped. |

- [ ] **Step 1: Implement.**

```rust
//! Equipment tab: armor and weapons left; trinkets, infusions, consumables right.

use arcdps::imgui::Ui;
use axigear_core::loadout::{GearRow, Loadout, Tile};
use axigear_core::report::{CheckReport, SlotKey};

use super::tile::{self, TileStyle};
use super::{icons, theme};

const ROW_ICON: f32 = 40.0;
const CHIP: f32 = 24.0;
const TRINKET: f32 = 32.0;
const ROW_H: f32 = 44.0;

pub fn render(ui: &Ui, l: &Loadout, report: &CheckReport, focus: Option<SlotKey>) {
    let o = ui.cursor_screen_pos();
    let avail = ui.content_region_avail()[0];
    let col_w = ((avail - 12.0) / 2.0).max(240.0);
    let left = column(ui, [o[0], o[1]], col_w, |ui, w| {
        eyebrow(ui, "Armor");
        for row in &l.armor {
            gear_row(ui, row, w, report, focus);
        }
        eyebrow(ui, "Weapons");
        for set in &l.weapons {
            ui.text_colored(theme::TEXT_FAINT, format!("SET {}", set.label));
            gear_row(ui, &set.main, w, report, focus);
            if set.two_handed {
                ui.text_colored(theme::TEXT_FAINT, "     Two-Handed");
            } else if let Some(off) = &set.off {
                gear_row(ui, off, w, report, focus);
            }
        }
    });
    let right = column(ui, [o[0] + col_w + 12.0, o[1]], col_w, |ui, w| {
        eyebrow(ui, "Trinkets");
        let first: Vec<&Tile> = l.trinkets[..3].iter().chain(std::iter::once(&l.relic)).collect();
        let second: Vec<&Tile> = l.trinkets[3..].iter().collect();
        for row in [first, second] {
            trinket_row(ui, &row, w, report, focus);
        }
        eyebrow(ui, "Infusions");
        if let Some(t) = report.worst(SlotKey::Infusions, &[]) {
            ui.same_line();
            icons::draw(ui, t, ui.text_line_height());
        }
        if l.infusions.is_empty() {
            ui.text_colored(theme::TEXT_FAINT, "none set");
        } else {
            let start = ui.cursor_screen_pos();
            let per_row = ((w + 3.0) / (CHIP + 3.0)).floor().max(1.0) as usize;
            for (i, t) in l.infusions.iter().enumerate() {
                ui.set_cursor_screen_pos([start[0] + (i % per_row) as f32 * (CHIP + 3.0), start[1] + (i / per_row) as f32 * (CHIP + 3.0)]);
                tile::icon(ui, t, None, TileStyle::new(CHIP));
            }
        }
        eyebrow(ui, "Consumables");
        for t in [&l.food, &l.utility] {
            gear_row(ui, &GearRow { tile: t.clone(), upgrades: vec![] }, w, report, focus);
        }
    });
    ui.set_cursor_screen_pos([o[0], left.max(right)]);
    ui.dummy([avail, 0.0]);
}

/// Draws `f` with the cursor at `at`, inside a group so items lay out top-down; returns the bottom y.
fn column(ui: &Ui, at: [f32; 2], w: f32, f: impl FnOnce(&Ui, f32)) -> f32 {
    ui.set_cursor_screen_pos(at);
    let g = ui.begin_group();
    f(ui, w);
    g.end();
    ui.item_rect_max()[1]
}

fn eyebrow(ui: &Ui, s: &str) {
    ui.dummy([0.0, 4.0]);
    ui.text_colored(theme::GOLD, s.to_uppercase());
}

fn gear_row(ui: &Ui, row: &GearRow, w: f32, report: &CheckReport, focus: Option<SlotKey>) {
    let o = ui.cursor_screen_pos();
    tile::icon(ui, &row.tile, Some(report), TileStyle { pulse: focus == Some(row.tile.key), ..TileStyle::new(ROW_ICON) });
    let chips_w = row.upgrades.len() as f32 * (CHIP + 3.0);
    let text_x = o[0] + ROW_ICON + 8.0;
    let text_w = (w - ROW_ICON - 8.0 - chips_w - 4.0).max(20.0);
    ui.set_cursor_screen_pos([text_x, o[1] + 2.0]);
    ui.text_colored(theme::TEXT_FAINT, row.tile.label.to_uppercase());
    ui.set_cursor_screen_pos([text_x, o[1] + 2.0 + ui.text_line_height()]);
    let name = if row.tile.empty { "—" } else { row.tile.name.as_str() };
    tile::clipped_text(ui, name, text_w, if row.tile.empty { theme::TEXT_FAINT } else { theme::TEXT });
    if let Some(sub) = &row.tile.sub {
        ui.set_cursor_screen_pos([text_x, o[1] + 2.0 + 2.0 * ui.text_line_height()]);
        tile::clipped_text(ui, sub, text_w, theme::TEXT_FAINT);
    }
    for (i, u) in row.upgrades.iter().enumerate() {
        ui.set_cursor_screen_pos([o[0] + w - chips_w + i as f32 * (CHIP + 3.0), o[1] + (ROW_ICON - CHIP) / 2.0]);
        tile::icon(ui, u, Some(report), TileStyle { pulse: focus == Some(u.key), ..TileStyle::new(CHIP) });
    }
    ui.set_cursor_screen_pos([o[0], o[1] + ROW_H.max(if row.tile.sub.is_some() { 3.0 * ui.text_line_height() + 4.0 } else { 0.0 })]);
}

fn trinket_row(ui: &Ui, tiles: &[&Tile], w: f32, report: &CheckReport, focus: Option<SlotKey>) {
    let o = ui.cursor_screen_pos();
    let cell = w / 4.0;
    let line = ui.text_line_height();
    for (i, t) in tiles.iter().enumerate() {
        let x = o[0] + i as f32 * cell;
        ui.set_cursor_screen_pos([x + (cell - TRINKET) / 2.0, o[1]]);
        tile::icon(ui, t, Some(report), TileStyle { pulse: focus == Some(t.key), ..TileStyle::new(TRINKET) });
        ui.set_cursor_screen_pos([x, o[1] + TRINKET + 2.0]);
        tile::clipped_text(ui, &t.label.to_uppercase(), cell - 4.0, theme::TEXT_FAINT);
        ui.set_cursor_screen_pos([x, o[1] + TRINKET + 2.0 + line]);
        tile::clipped_text(ui, if t.empty { "—" } else { &t.name }, cell - 4.0, theme::TEXT_DIM);
    }
    ui.set_cursor_screen_pos([o[0], o[1] + TRINKET + 4.0 + 2.0 * line + 6.0]);
}
```

  If `begin_group` or `item_rect_max` differ in the fork, check `grep -n "fn begin_group\|fn item_rect_max" ~/.cargo/git/checkouts/imgui-rs-*/*/imgui/src/*.rs`. As an alternative, track y manually: read `ui.cursor_screen_pos()[1]` after the closure and return it.

- [ ] **Step 2: Check that the Windows build compiles, and run every test.**

  Run: `cargo dll-check && cargo test -p axigear-core && cargo test -p arcdps_axigear`

  Expected: all OK.

- [ ] **Step 3: Build the DLL and deploy it for an in-game look.**

  Run: `cargo dll && scripts/deploy.sh`

  Expected: the DLL is installed. Then **stop and hand over to the user** for the spec's manual in-game checklist:
  - icons stream in without stutter;
  - a problem click jumps to the right tab and slot;
  - offline gives a text fallback;
  - a two-handed set renders correctly;
  - a `TraitSel::None` tier renders correctly.

  Deploying only writes the user's local game folder, which they authorized for v0.1.3. Ask first if unsure.

- [ ] **Step 4: Commit.**

```bash
git add crates/axigear/src/ui/equipment_tab.rs
git commit -m "feat(ui): equipment tab with armor, weapons, trinkets and consumables

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

## Self-review notes

- **Spec coverage:**

  | Spec section | Task(s) |
  |---|---|
  | §1 icon URLs | 1, 2, 3 |
  | §1 per-slot status | 4, 5, 6 |
  | §1 texture loading | 9 |
  | §2 header and problems panel | 11 |
  | §2 tabs | 11 |
  | §2 Build tab | 12 |
  | §2 Equipment tab | 13 |
  | §2 tiles, chips and tooltips | 10 |
  | §3 errors and fallbacks | 9 (failed/off-host/no device), 10 (text fallback) |
  | §3 testing | 1–9, 11 |
  | §3 release | out of plan (user tags v0.2.0 after in-game checks) |

- **Interfaces used across tasks:**
  - `SlotKey`, `Tab`, `SlotMark` and `CheckResult::mark` are defined in Task 4 and used in Tasks 5–13.
  - `Tile`, `GearRow`, `WeaponSet`, `TraitTile`, `SpecCard` and `Loadout` are defined in Task 7 and used in Tasks 10–13.
  - `TileStyle::new(size)` is defined in Task 10.
