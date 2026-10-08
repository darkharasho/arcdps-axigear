#!/usr/bin/env python3
"""Regenerates crates/axigear-core/data/specializations.json from the GW2 API.

Per spec: id, name, profession, elite, icon, background, minors (minor trait
IDs), traits (id -> name/icon for minors and majors), and major traits grouped
by tier in the API's major_traits order — the same grouping AxiForge uses to
turn a build code's trait positions (1..3) into trait IDs.
"""
import json, sys, urllib.error, urllib.request

API = "https://api.guildwars2.com/v2"

def get(path):
    with urllib.request.urlopen(f"{API}/{path}", timeout=30) as r:
        return json.load(r)

def paged(path):
    out, page = [], 0
    while True:
        try:
            batch = get(f"{path}?page_size=200&page={page}")
        except urllib.error.HTTPError as e:
            if e.code == 400 and page > 0:
                return out
            raise
        out += batch
        if len(batch) < 200:
            return out
        page += 1

out_path = sys.argv[1] if len(sys.argv) > 1 else "crates/axigear-core/data/specializations.json"
specs = get("specializations?ids=all")
traits = {t["id"]: t for t in paged("traits")}
rows = []
for s in sorted(specs, key=lambda s: s["id"]):
    majors = [[], [], []]
    for tid in s["major_traits"]:
        tier = traits.get(tid, {}).get("tier", 0)
        if 1 <= tier <= 3:
            majors[tier - 1].append(tid)
    used = list(s["minor_traits"]) + [t for tier in majors for t in tier]
    rows.append({"id": s["id"], "name": s["name"], "profession": s["profession"],
                 "elite": s["elite"], "majors": majors,
                 "icon": s["icon"], "background": s["background"],
                 "minors": s["minor_traits"],
                 "traits": {str(t): {"name": traits[t]["name"], "icon": traits[t]["icon"]}
                            for t in used if t in traits}})
with open(out_path, "w") as f:
    json.dump(rows, f, indent=1)
    f.write("\n")
print(f"{len(rows)} specializations -> {out_path}")
