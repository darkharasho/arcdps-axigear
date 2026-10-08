# arcdps-axigear

arcdps plugin that checks your build/gear/consumables against your slot in an AxiForge comp.
Spec: docs/superpowers/specs/2026-10-07-axigear-design.md

## Build
- `cargo test --workspace` — all logic lives in `crates/axigear-core` and tests on Linux.
- `cargo dll-check` / `cargo dll` — cross-compile the plugin with cargo-xwin.
- NEVER build `x86_64-pc-windows-gnu`: it links but crashes on load inside GW2.

## Deploy
`scripts/deploy.sh` copies to `<dest>.new` then `mv`s — a plain `cp` over a loaded DLL under Wine corrupts GW2's mapped pages.

## Design
Same axi-design contract as arcdps-axipulse: colours only in `src/ui/theme.rs`, raised elements via `src/ui/axi.rs`,
one draw list at a time (a second `get_window_draw_list()` while one is live panics across FFI and kills the game).
`crates/axigear/tests/axi_guard_test.rs` enforces it. Status icons are drawn, not glyphs — the arcdps font is Latin-1 only.
