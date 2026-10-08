#!/usr/bin/env python3
"""Regenerates crates/axigear-core/data/named_icons.json.

Builds name the relic, food and utility (not an item ID), so the overlay looks
their icons up by name. IDs come from AxiForge's upgradeIds.json, the same
catalog its pickers use; names/icons/descriptions from /v2/items.
Usage: AXIFORGE=../axiforge scripts/gen-named-icons.py
"""
import json, os, re, sys, time, urllib.request

API = "https://api.guildwars2.com/v2"
here = os.path.dirname(os.path.abspath(__file__))
axiforge = os.environ.get("AXIFORGE", os.path.join(here, "..", "..", "axiforge"))
ids = json.load(open(os.path.join(axiforge, "src/main/gw2Data/upgradeIds.json")))
out_path = sys.argv[1] if len(sys.argv) > 1 else os.path.join(here, "..", "crates/axigear-core/data/named_icons.json")

def fetch(url):
    for attempt in range(4):
        try:
            with urllib.request.urlopen(url, timeout=30) as r:
                return json.load(r)
        except Exception:
            if attempt == 3:
                raise
            time.sleep(2 * (attempt + 1))

def items(id_list):
    out = []
    for i in range(0, len(id_list), 200):
        chunk = ",".join(str(x) for x in id_list[i:i + 200])
        out += fetch(f"{API}/items?ids={chunk}")
    return out

rows, seen = [], set()
for kind, key in (("relic", "RELIC_ITEM_IDS"), ("food", "FOOD_ITEM_IDS"), ("utility", "UTILITY_ITEM_IDS")):
    for it in items(ids[key]):
        name, icon = it.get("name", ""), it.get("icon", "")
        if not name or not icon or (kind, name) in seen:
            continue
        seen.add((kind, name))
        buff = (it.get("details") or {}).get("description", "") or it.get("description", "")
        buff = re.sub(r"<[^>]+>", "", buff)
        rows.append({"kind": kind, "name": name, "icon": icon, "buff": " ".join(buff.split())})
rows.sort(key=lambda r: (r["kind"], r["name"]))
with open(out_path, "w") as f:
    json.dump(rows, f, indent=1)
    f.write("\n")
print(f"{len(rows)} named icons -> {out_path}")
