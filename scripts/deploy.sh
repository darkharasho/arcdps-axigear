#!/usr/bin/env bash
# Install the release DLL into GW2's addons folder atomically (cp to .new then mv):
# a plain cp over a loaded DLL under Wine corrupts the pages GW2 has mapped.
set -euo pipefail
REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SRC="$REPO_ROOT/target/x86_64-pc-windows-msvc/release/arcdps_axigear.dll"
DEST="${AXIGEAR_DEPLOY_DEST:-/var/mnt/data/SteamLibrary/steamapps/common/Guild Wars 2/addons/arcdps_axigear.dll}"
[[ -f "$SRC" ]] || { echo "build artifact missing: $SRC - run 'cargo dll' first" >&2; exit 1; }
cp "$SRC" "${DEST}.new"
mv "${DEST}.new" "$DEST"
ls -lh "$DEST"
