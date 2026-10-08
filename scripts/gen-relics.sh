#!/usr/bin/env bash
# Copies AxiCode's append-only relic wire table into crates/axigear-core/data/relics.json.
# Usage: AXIFORGE=../axiforge scripts/gen-relics.sh
set -euo pipefail
AXIFORGE="${AXIFORGE:-$(dirname "$0")/../../axiforge}"
node -e 'process.stdout.write(JSON.stringify(require(process.argv[1]), null, 1) + "\n")' \
  "$(realpath "$AXIFORGE/packages/axicode/src/relics.js")" > "$(dirname "$0")/../crates/axigear-core/data/relics.json"
echo "relics.json: $(grep -c '"' "$(dirname "$0")/../crates/axigear-core/data/relics.json") entries"
