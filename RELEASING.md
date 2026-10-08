# Releasing axigear

1. Bump `version` in the root `Cargo.toml` (`[workspace.package]`).
2. Write `RELEASE_NOTES.md` in the repo root (tracked, overwritten each release, as in axipulse).
   Start it with `# Release Notes`, a blank line, then `Version vX.Y.Z — <Month D, YYYY>`; the
   `Release` workflow strips those header lines before posting to Discord. Wrap prose at ~72
   columns (the workflow unwraps paragraphs for Discord).
3. `cargo dll-check` (refreshes Cargo.lock), then `cargo dll`.
4. `scripts/verify_release_build.sh`.
5. Commit the bump and the notes (`chore: release vX.Y.Z`), tag `vX.Y.Z`, push (ask first).
6. `gh release create vX.Y.Z target/x86_64-pc-windows-msvc/release/arcdps_axigear.dll --notes-file RELEASE_NOTES.md`
   — the asset must be named exactly `arcdps_axigear.dll` (the in-game updater looks for it).
7. The `Release` workflow posts the notes to Discord when `vars.DISCORD_WEBHOOK_URL` is set.
