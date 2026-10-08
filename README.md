# axigear

An arcdps plugin that checks — live, in game — whether your build, gear and
consumables match your slot in an [AxiForge](https://github.com/darkharasho/axiforge) comp.

## Install
Drop `arcdps_axigear.dll` next to your other arcdps addons and start GW2.

## Use
1. arcdps options (Alt+Shift+T) → axigear → paste an AxiForge comp code, build code,
   or published comp link → **Load**.
2. axigear matches your slot from your current spec (pick manually when several fit;
   the pick is remembered per comp and character).
3. The badge shows ✓ / ⚠ / ✗; click it (or Ctrl+Shift+G) for the checklist.

## GW2 API key
Gear, traits and skill-bar checks need a GW2 API key with the **characters** and
**builds** permissions. The key is stored **in plain text** in
`addons/axigear/config.json` (as arcdps and axiam do). Live checks (spec, food,
utility, skills seen) work without a key.

## Files
`addons/axigear/config.json`, `itemdb.json` (item name cache), `comp_cache.json` (last comp, for offline use).

## Building
See `CLAUDE.md` (`cargo test --workspace`, `cargo dll`) and `RELEASING.md`.
