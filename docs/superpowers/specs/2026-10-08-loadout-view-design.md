# axigear loadout view: AxiForge-style tabs with real icons

Date: 2026-10-08
Status: approved design, pending implementation plan
Target release: v0.2.0

## Goal

The main axigear window should look like AxiForge's Build and Equipment tabs, with
real GW2 icons, instead of a text checklist. A problems summary stays pinned on top
so the player still sees what's wrong at a glance.

### Decisions made during brainstorming

1. **Layout.** The visual tabs are the main view, and a problems-only summary sits
   above them (option C).
2. **What the tabs draw.** They draw the comp's **target** build (option A). A slot
   that fails a check gets a ✗ or ⚠ chip, and its hover tooltip shows what the
   player is wearing. Nothing draws the worn loadout.
3. **Size.** Compact, at about 520 px wide: two tabs (Build and Equipment) and a
   two-column equipment layout (option A). There is no attributes panel.
4. **Where icons come from.** Icons are looked up by ID through data axigear already
   resolves, plus bundled data (option 1). Icon URLs in AxiForge's publish payload
   are not used.

## Section 1: Data

### Icon URLs (axigear-core)

- **Items.** `gamedb::ItemInfo` gets `icon: Option<String>`, filled from the
  `/v2/items` responses axigear already fetches. This covers runes, sigils,
  infusions, relic, food and utility.
- **Skills.** `gamedb::SkillInfo` gets `icon: Option<String>`, filled from the
  `/v2/skills` responses axigear already fetches (heal, utilities, elite). Neither
  change adds requests.
- **Cache migration.** Older `itemdb.json` entries have no `icon` field and load as
  `None`. `GameDb::wanted` treats an entry with `icon == None` that has never been
  icon-checked as wanted, so it is re-fetched once, in the normal batch.
  `ItemInfo`/`SkillInfo` get `icon_checked: bool` (serde default `false`), set to
  true once a fetch returns. An entry whose API response has no icon is then never
  re-fetched again.
- **Specializations.** `scripts/gen-specdb.py` extends `data/specializations.json`
  with the following for each spec:
  - `icon`: the emblem URL.
  - `background`: the banner URL.
  - `minors: [u32; 3]`.
  - `trait_icons: {trait_id: url}`, covering every minor and major.

  `specs::SpecInfo` exposes these fields. Traits need no API calls at runtime.
- **Stat-only slots.** Armor, trinkets, back and weapon types have no item ID, so
  their icons are bundled PNGs embedded with `include_bytes!` under
  `crates/axigear/assets/slots/`:
  - Armor: 6 slots × 3 weight classes.
  - Trinkets: amulet, ring, accessory, back.
  - Weapons: one PNG per weapon type.

  That is about 45 files at 64×64, around 1 MB in total. The armor weight class
  comes from the build's profession. A small table in the UI crate maps
  `(GearSlot, weight)` and weapon type to an asset.

### Per-slot status (axigear-core)

`report::CheckResult` gets `marks: Vec<SlotMark>`:

```rust
pub enum SlotKey {
    Gear(GearSlot),              // armor, trinket and weapon pieces: stats / weapon type
    Rune(GearSlot),
    Sigil(GearSlot, u8),         // weapon slot, sigil index
    Infusions,                   // build-wide; no per-slot breakdown
    Skill(u8),                   // 0 heal, 1-3 utilities, 4 elite
    Trait { line: u8, tier: u8 },
    Spec(u8),                    // line 0-2
    Relic,
    Food,
    Utility,
}
pub struct SlotMark { pub key: SlotKey, pub status: Status, pub detail: Option<String> }
```

- Checks that already loop over slots push one mark per slot. These are stats,
  runes, sigils, weapons, skill bar, skills seen, traits, specializations, relic,
  food, utility and infusions. `detail` holds that slot's actual-value text, for
  example "Scholar" or "empty".
- **Stats.** The multiset stat match stays as it is. A slot passes if it consumed a
  wanted stat from its group's pool and fails if it ended in `leftover` or was
  empty. An unresolved item gives `Unknown`.
- **Forced advisory.** Marks inherit the result's status after any
  severity/forced-advisory adjustment, so a mark's tone matches its row in the
  problems panel.
- **Unchanged.** The existing text output (`expected`, `actual`, `reason`,
  `detail()`, `text()`, `groups()`) stays the same, and its tests keep passing
  unmodified.
- **New lookup.** `CheckReport::mark(&SlotKey) -> Option<(&CheckResult, &SlotMark)>`
  returns the worst status for that key, ranked fail > warn > unknown > pass.

### Texture loading (axigear UI crate)

The texture loader is ported from arcdps-axipulse `src/ui/icons.rs` and
`src/ui/tile_cache.rs`, as a new `ui/textures.rs`. The existing `ui/icons.rs`
glyph drawer stays as it is.

- One worker thread receives URL requests over a channel. For each URL it checks
  the disk cache, downloads with ureq if needed, decodes with
  `image::load_from_memory` to RGBA8, and sends the result back.
- The imgui thread uploads at most `MAX_UPLOADS_PER_FRAME = 4` textures per frame
  through `CreateTexture2D` and a shader resource view (Wine stability). SRVs live
  for the life of the process.
- `textures::get(url) -> Option<IconHandle { tex: TextureId, aspect: f32 }>`.
  - If the icon isn't ready it returns `None` and queues the URL, deduplicated.
  - Bundled PNGs go through the same upload path, keyed `bundled:<name>`.
- Only URLs on `https://render.guildwars2.com/` are fetched. That host is a CDN,
  not the rate-limited API. Any other host is treated as failed.
- New dependencies: the `image` crate (`default-features = false`,
  `features = ["png", "jpeg"]`), and the windows features
  `Win32_Graphics_Direct3D11`, `Win32_Graphics_Direct3D` and
  `Win32_Graphics_Dxgi_Common`.

## Section 2: Window layout (min width 520 px, resizable, vertical scroll)

The window uses the AxiForge tokens already in `ui/theme.rs` and `ui/axi.rs`:
square corners, 3 px ink tile borders, gold eyebrow headers, and the ground, panel
and raised surfaces.

### 1. Header

The header keeps today's content in two tight rows:

- Build title and comp name, the offline marker, and Refresh.
- Slot label with the slot picker, the API status line, and Refresh API.

### 2. Problems panel

- Each failing or warning check gets one line: status mark, label, then the dim
  detail.
- Clicking a line switches to the tab that holds the check's first mark. That slot
  then pulses with a gold outline for about 1.5 s.
- Unknown checks fold into one dim line, "N waiting for data". Hovering it lists
  them.
- When nothing fails or warns, the panel collapses to one line,
  "✓ All N checks pass", where N is the number of decided checks.

### 3. Tabs

A segmented `BUILD | EQUIPMENT` control with a gold underline on the active tab.
The active tab is saved in settings.

#### Build tab

- **Skill bar.** Heal, utilities 1 to 3, and elite as 48 px tiles in one row.
  - Each tile has its check chip in the top-right corner.
  - A skill that hasn't been cast this map (an Unknown SkillsSeen mark) gets a
    faint dot under its tile.
- **Spec cards.** Three stacked cards, about 500×110 each.
  - The background is the spec banner, dimmed.
  - The emblem is 56 px, with the spec name as an eyebrow.
  - Then minor, major column, minor, major column, minor, major column. Minors are
    26 px.
  - A major column stacks its three choices, top, middle and bottom, at 32 px. The
    selected choice is in full colour with a 3 px gold underline. Unselected
    choices are drawn with a dark tint, because imgui cannot grayscale without a
    shader. A `TraitSel::Any` column draws all three in colour with no underline.
  - A mismatched trait gets a red outline. Its tooltip shows the expected and
    actual trait.
  - A wrong specialization line puts a red outline around the whole card.

#### Equipment tab: two columns of about 250 px

- **Left column**
  - **ARMOR.** Six rows: Helm, Shoulders, Coat, Gloves, Leggings, Boots. Each row
    has a 40 px icon, an 11 px uppercase faint slot label, the 12 px stat name,
    and a 24 px rune chip on the right.
  - **WEAPONS.** One row each for set A and set B. Each row shows the weapon icons,
    the weapon type, the stat and the sigil chips. When the set's main hand is
    two-handed, the off hand shows a dark-tinted "Two-Handed".
- **Right column**
  - **TRINKETS.** A 4-column grid of 32 px tiles: Back, Acc 1, Acc 2, Relic, then
    Amulet, Ring 1, Ring 2. Each tile shows a 9 px label and a truncated 10 px stat
    name. For the relic, "Relic of the" is stripped from its name.
  - **INFUSIONS.** A strip of 24 px chips. The check is build-wide, so the
    `Infusions` mark goes on the strip header.
  - **CONSUMABLES.** Food and Utility rows, each with a 40 px icon, the name, and
    the buff line in faint text.
- **Slots the comp leaves empty** are drawn as a dashed-border tile with a
  dark-tinted icon.

### Tiles

- **Loading or no icon.** A raised tile shows a short text label, truncated with
  `axi::truncate_to_width`.
- **Hover.** Every tile shows a tooltip with:
  - the name;
  - "Comp: <expected>" and, if the slot is marked, "You: <detail>";
  - the source and how old the data is, using today's live / GW2 API wording.
- **Status chip.** A 14 px chip in the tile's top-right corner shows the status
  glyph from `ui/icons.rs`, in fail, warn or unknown colours. A passing tile has no
  chip.

The badge is unchanged.

## Section 3: Errors, testing, release

### Errors and fallbacks

- **Download, decode or HTTP error.** The URL is marked failed for the session and
  never retried each frame. One `log::warn!` per URL. The tile keeps its text
  fallback.
- **Missing icon URL.** Use the bundled slot PNG if there is one, otherwise the text
  tile.
- **No D3D11 device.** The texture cache is disabled and `get` always returns
  `None`, so the UI renders entirely as text tiles.
- **Disk cache.** Files live at `<arcdps addon dir>/axigear/icons/<fnv64(url)>.png`.
  - Writes go to a temp file that is then renamed.
  - A file that fails to decode is deleted and fetched again once.
- **Memory.** About 80 icons at 64×64 RGBA is about 1.3 MB for each build seen.
  Textures are never evicted, which matches axipulse.

### Testing

- **axigear-core unit tests**
  - Each check emits the right `marks`: keys, statuses, and detail. This includes
    the stats multiset case, where a stat worn on the wrong slot fails that slot,
    and the unresolved-item case, which gives `Unknown`.
  - `CheckReport::mark` returns the worst status.
  - `gamedb` parses `icon`, and old cache entries without `icon_checked` are
    wanted again exactly once.
  - Bundled `specializations.json` has `icon`, `background`, three `minors` and a
    trait icon for every minor and major of every spec.
  - Existing report text tests stay unmodified and pass.
- **UI crate tests** cover pure helpers only:
  - the slot-asset table;
  - the problem → tab and key resolution;
  - the trait column layout maths;
  - URL host filtering and the cache file naming.

  The D3D11 path is not tested headless, which matches axipulse.
- **Manual in-game checklist (Wine).** This is done before release.
  - Icons stream in without stutter or crashes.
  - Clicking a problem jumps to the right tab and slot.
  - Offline gives a clean text fallback.
  - A two-handed weapon set renders correctly.
  - `TraitSel::Any` renders correctly.

### Release

v0.2.0 ships through the existing tag-triggered release flow, after the user
checks it in game.

## Out of scope

- Weapon skills, profession mechanics, pets and legends.
- The attributes panel.
- Drawing the worn loadout.
- Profession accent colours.
- Rarity colours, which AxiForge has none of.
- Using icon URLs from the AxiForge publish payload.
