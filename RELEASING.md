# Releasing axigear

1. Bump `version` in the root `Cargo.toml` (`[workspace.package]`).
2. `cargo dll-check` (refreshes Cargo.lock), then `cargo dll`.
3. `scripts/verify_release_build.sh`.
4. Commit, tag `vX.Y.Z`, push (ask first).
5. `gh release create vX.Y.Z target/x86_64-pc-windows-msvc/release/arcdps_axigear.dll --notes-file RELEASE_NOTES.md`
   — the asset must be named exactly `arcdps_axigear.dll` (the in-game updater looks for it).
6. The `Release` workflow posts the notes to Discord when `vars.DISCORD_WEBHOOK_URL` is set.
