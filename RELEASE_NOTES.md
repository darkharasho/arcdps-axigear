# Release Notes

Version v0.1.0 — October 7, 2026

## First release

axigear checks, live in game, whether your build, gear and consumables
match your slot in an AxiForge comp or build.

- Paste an AxiForge comp code, build code or published comp link in the
  arcdps options (Alt+Shift+T) → axigear. Both the v1 and v2 AxiForge
  formats load, and published comps refresh when their author updates
  them.
- Your slot is picked from your current spec. When several slots fit,
  you choose once and the pick is remembered per comp and character.
- The badge shows ✓, ⚠ or ✗. Click it, or press Ctrl+Shift+G, for the
  full checklist: spec, traits, skills, gear stats, runes, sigils,
  relic, food and utility. It hides in combat by default.
- A check axigear can't confirm shows as unknown with the reason,
  never as a false ✗.
- Gear, traits and skill-bar checks need a GW2 API key with the
  characters and builds permissions. Spec, food, utility and skills
  seen work without one.
- axigear updates itself from GitHub releases.

Commander reporting is planned for a later release.
