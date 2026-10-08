# axigear comp library, saved API key, and Equipment tab tidy-up

Date: 2026-10-08
Status: approved design, pending implementation plan
Target release: v0.2.0 (same branch as the loadout view, `feat/loadout-view`)

## Goal

1. Keep several comps (pasted codes or AxiForge links). The user switches between
   them without pasting again, refreshes each one, and unsubscribes from each one
   on its own line.
2. The API key stays saved without a separate Save click.
3. The Equipment tab is easier to read: rune and sigil names are shown, icons line
   up with their text, infusions appear on their own slot, and food and utility
   show real names instead of item IDs.

### Decisions made during brainstorming

1. **Where comps are managed.** The list lives in the arcdps options tab, with one
   line per comp and Use / Refresh / Unsubscribe buttons. The main window's
   "Comp:" line becomes a dropdown of saved comps (option A).
2. **Cache layout.** One `comp_cache.json` holds every saved comp, instead of one
   file per comp.
3. **No Attributes panel** on the Equipment tab.

## Part 1: Comp library and API key

### Settings (axigear-core `settings.rs`)

- `Settings.comp_input: String` is replaced by:
  - `comps: Vec<SavedComp>`, where `SavedComp { input: String, name: String }`.
    `input` is the exact text the user loaded (a code or a link). `name` is the
    comp's display name from its last successful load, so the list can be drawn
    before the cache is read.
  - `active_comp: Option<String>`, the `input` of the comp in use.
  - Both have serde defaults.
- **Migration.** `comp_input` stays as a read-only serde field
  (`#[serde(default, skip_serializing)]`). On load, if it is non-empty and `comps`
  is empty, it becomes the single entry in `comps` and is set as `active_comp`.
  Its `name` is filled from the cache, or left as `""` until the next load. The
  next save drops `comp_input` from the file.
- Order: newest first. Loading a new comp inserts it at the top.

### Cache (axigear-core `driver.rs`)

- `comp_cache.json` becomes `{ "comps": [LoadedComp, ...] }`, one entry per saved
  comp, keyed by `LoadedComp.input`.
- **Old format.** If the file holds a single `LoadedComp` (the v0.1.x format), it
  is read as a list of one. It is written back in the new format on the next save.
- On start-up, the active comp is taken from the cache when its `input` matches
  `active_comp`. If it has no cache entry, it is loaded the same way as today: a
  code is decoded, a link is fetched.
- Entries whose `input` is not in `settings.comps` are dropped on save.

### Commands

- `LoadInput(text)`:
  - **New input:** loads it as today. On success the comp is added to the top of
    `comps`, becomes active, and is written to the cache.
  - **Input already saved** (same trimmed text): it is refreshed instead. It becomes
    active and its cache entry is replaced. No duplicate is added.
  - **Load fails:** nothing is added, and the error shows as today.
- `UseComp(input)` (new): switches to the saved copy in the cache, with no network
  call. If the comp has no cache entry, it is loaded again with `LoadInput` logic.
  Picks stay per comp and character, because the pick key is already derived from
  the comp.
- `RefreshComp(input)`: now takes which comp to refresh.
  - **Link:** fetched again, using the same ETag logic as today.
  - **Code:** decoded again (offline, so it effectively just re-reads the code).
  - Refreshing a comp that is not active updates its cache entry but does not
    switch to it.
- `Unsubscribe(input)`: now takes which comp to drop. It removes the comp from
  `comps` and the cache, and drops that comp's picks.
  - **Active comp:** the next comp in the list becomes active. If none is left,
    the state is "no comp loaded", as `Unsubscribe` gives today.
- Automatic link refresh: the existing schedule applies to the **active** comp only.

### Snapshot

- `UiSnapshot.comps: Vec<CompRow>`, where `CompRow { input, name, source, active }`.
  - `source` uses today's `Header.source` wording ("code" or "link · fetched 2m ago").
  - `name` falls back to a shortened input when it is empty.

### UI

- **Options tab, COMP section:** the multiline input and **Load** button stay at
  the top. Below them is one line per saved comp:
  - the comp name and source, with the active comp marked;
  - **Use** (disabled on the active comp), **Refresh**, and **Unsubscribe**.

  The old `Current: …` line and the standalone Unsubscribe button are removed.
- **Main window header:** `Comp:` becomes an imgui combo showing the active comp's
  name. Choosing an entry sends `UseComp`. The existing Refresh small button and the
  "offline" tag stay next to it. With no saved comps it reads "Comp: none", as today.

### API key

- The input saves the key itself. When the field is deactivated after an edit
  (`is_item_deactivated_after_edit`), it sends `SetApiKey`. **Test** sends
  `SetApiKey` first if the field differs from the saved key, then `TestKey`.
- The **Save** button is removed.
- **Status line** under the field:
  - "Saved", until the next test;
  - after a test, the test result (today's `api_line` text);
  - when no key is saved, nothing.
- The key is never logged or shown in plain text; the field stays a password field.

## Part 2: Equipment tab

### Model (axigear-core)

- **Infusions per slot.** `Equipment` gains
  `infusions_by_slot: BTreeMap<GearSlot, Vec<u32>>`. It is filled in the same loop
  that builds `infusions`, with the same capacity limit per slot.
  - The flat, sorted `infusions` list stays as it is, and the infusion check still
    uses it, so check results do not change.
  - No cache migration is needed: `Build` is parsed from raw data each time.
- **Food and utility given as item IDs.** AxiForge links can store food and utility
  as item ID strings (e.g. `"91835"`). Text that is all digits is treated as an
  item ID:
  - `GameDb::wanted` adds those IDs to `w.items`, so their name and icon are fetched
    in the normal batch.
  - **Check.** `checks::consumables::check` resolves the wanted label through
    `ctx.db.item_name(id)` before matching. While the name is still unknown, the
    row is `Unknown` with the reason "looking up item name".
  - **Tile.** `Loadout` uses the resolved name and the item icon. The bundled named
    icon and buff text are used when the resolved name matches a bundled entry.
  - Non-numeric labels behave exactly as today.

### Loadout (axigear-core `loadout.rs`)

- `GearRow` gains `infusions: Vec<Tile>`, the per-slot infusion tiles, keyed
  `SlotKey::Infusion(item_id)` (new variant).
- The trinket rows are built the same way: as `GearRow`s with `upgrades` empty and
  their own `infusions`.
- `Loadout.infusions` (the flat list) is removed once nothing draws it.
- Weapon Set B rows are left out when every Set B slot is empty, which the tab
  already uses to decide whether to draw the heading.

### Layout (axigear `equipment_tab.rs`)

- **Columns.** Two fixed-width columns of `COL_W = 340` px, laid out from the left
  instead of stretched. If the window is narrower than `2 * COL_W + gap`, the right
  column moves below the left one.
  - **Left column:** Armor, then Weapons · Set A and Set B.
  - **Right column:** Trinkets, Relic, Consumables.
- **Section headings.** A small eyebrow label in `theme::TEXT_FAINT` with a hairline
  rule under it (a new `theme::RULE` colour), for **ARMOR**, **WEAPONS · SET A**,
  **WEAPONS · SET B**, **TRINKETS** and **CONSUMABLES**.
- **Armor and weapon row**, about 44 px tall:
  - a 40 px item icon with its status chip;
  - the item name on the first line;
  - one sub-line per upgrade: a 16 px rune or sigil icon and its full name;
  - "two-handed" shown inline after the weapon name;
  - per-slot infusion chips (16 px) right-aligned at the end of the row.
  - Two sigils on one line when they fit the column width, otherwise one per line.
    Text is clipped with "…" rather than wrapped.
- **Trinket row.** A 32 px icon centred vertically on its two text lines: the item
  name, and below it the slot name ("Back", "Ring 1", …) in `TEXT_DIM`. The
  per-slot infusion chips are right-aligned. The relic uses the same row.
- **Consumables.** Food and Utility rows in the trinket-row style: the resolved
  name, with the buff text (or "Food" / "Utility") as the sub-line.
- **Infusion problems.** The infusion check compares counts only, so it cannot
  say which slot is wrong. It keeps its `SlotKey::Infusions` row mark. On a fail it
  also adds a `SlotKey::Infusion(id)` Fail mark for each wanted infusion ID that the
  worn gear has fewer of than the comp wants. The tab draws the pulse/warn ring
  around every chip whose ID carries such a mark, instead of around a whole grid.
  For example, if the comp wants 18 of one infusion and the player wears 17, every
  chip of that infusion rings.
- All colours come from `ui/theme.rs`, raised elements go through `ui/axi.rs`, and
  only one draw list is used at a time. `tests/axi_guard_test.rs` must still pass.

## Error handling

- A comp that fails to refresh keeps its cached copy and shows the error on its
  row, as the header note does today.
- A broken `comp_cache.json` (fails to parse) is treated as empty. Saved comps are
  then loaded again on use, and the settings list is never lost.
- An unknown food or utility item ID (the API never returns it) stays `Unknown`
  and the tile shows "Item {id}".

## Testing

- **Settings:**
  - an old `config.json` with `comp_input` migrates to `comps` and `active_comp`;
  - after saving, the file no longer contains `comp_input`.
- **Cache:**
  - an old single-comp `comp_cache.json` loads;
  - the new format round-trips;
  - entries that are no longer saved are dropped.
- **Driver:**
  - loading two comps keeps both, with the newest active;
  - loading a duplicate input refreshes it and does not add a second entry;
  - `UseComp` switches without an HTTP call (mock `Http` records no requests);
  - unsubscribing the active comp activates the next one, and unsubscribing the
    last one leaves no comp;
  - picks for comp A survive switching to B and back;
  - `RefreshComp` on a comp that is not active does not change the active comp.
- **API key:**
  - `SetApiKey` trims and persists;
  - Test with an unsaved edit sends `SetApiKey` before `TestKey` (UI helper unit
    test, in the same style as the existing UI-logic tests).
- **Model:**
  - `infusions_by_slot` keeps each slot's infusions and respects slot capacity;
  - the flat `infusions` list is unchanged.
- **Consumables:**
  - `"91835"` with a known item name passes when that buff is active;
  - with the name still unknown, the row is `Unknown`;
  - non-numeric labels are unchanged;
  - `GameDb::wanted` includes the numeric food and utility IDs.
- **Loadout:**
  - armor and weapon rows carry their infusion tiles;
  - an empty Set B produces no rows;
  - the food tile shows the resolved name.
- **Infusion marks:** wanting 3× A and wearing 2× A plus 1× B gives a
  `SlotKey::Infusion(A)` Fail mark and no mark for B.
- **Build:** `cargo test --workspace`, `cargo dll-check`, then `cargo dll` and
  `scripts/deploy.sh` for in-game verification.
