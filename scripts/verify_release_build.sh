#!/usr/bin/env bash
# Refuse to ship a DLL whose baked-in CARGO_PKG_VERSION is stale (axipulse v0.4.0 lesson:
# the updater compared a stale version to the latest tag and re-downloaded forever).
set -euo pipefail
REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DLL="$REPO_ROOT/target/x86_64-pc-windows-msvc/release/arcdps_axigear.dll"
MANIFEST="$REPO_ROOT/Cargo.toml"
VERSION="${1:-$(sed -n '/^\[workspace.package\]/,/^\[/s/^version = "\(.*\)"/\1/p' "$MANIFEST" | head -1)}"
[[ -n "$VERSION" ]] || { echo "could not determine version" >&2; exit 1; }
[[ -f "$DLL" ]] || { echo "build artifact missing: $DLL - run 'cargo dll'" >&2; exit 1; }
if [[ "$MANIFEST" -nt "$DLL" ]]; then
    echo "STALE BUILD: Cargo.toml is newer than the DLL. Re-run 'cargo dll'." >&2
    exit 1
fi
grep -aqF "$VERSION" "$DLL" || { echo "STALE BUILD: '$VERSION' not found in the DLL." >&2; exit 1; }
echo "verified: $DLL carries version $VERSION"
