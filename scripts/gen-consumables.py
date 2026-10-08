#!/usr/bin/env python3
"""Regenerates crates/axigear-core/data/consumables.json from Elite Insights.

EI's FoodBuffs.cs / UtilityBuffs.cs name every nourishment and enhancement
buff, and SkillIDs.cs maps the constants to the buff IDs arcdps reports.
"""
import json, re, sys, urllib.request

EI = ("https://raw.githubusercontent.com/baaron4/GW2-Elite-Insights-Parser/master/"
      "GW2EI.Library/GW2EI.Services/GW2EIEvtcParser")
NOT_CONSUMABLES = {"Malnourished", "Diminished"}

def fetch(path):
    with urllib.request.urlopen(f"{EI}/{path}", timeout=30) as r:
        return r.read().decode("utf-8")

out_path = sys.argv[1] if len(sys.argv) > 1 else "crates/axigear-core/data/consumables.json"
ids = {name: int(v) for name, v in re.findall(r"public const long (\w+) = (\d+);",
                                              fetch("ParserHelpers/IDs/SkillIDs.cs"))}
rows, seen = [], set()
for path, cls, kind in (("EIData/Buffs/FoodBuffs.cs", "Nourishment", "food"),
                        ("EIData/Buffs/UtilityBuffs.cs", "Enhancement", "utility")):
    pattern = r'new Buff\("([^"]+)",\s*(\w+),\s*Source\.Item,\s*BuffClassification\.' + cls
    for name, const in re.findall(pattern, fetch(path)):
        if name in NOT_CONSUMABLES or const not in ids or ids[const] in seen:
            continue
        seen.add(ids[const])
        rows.append({"id": ids[const], "kind": kind, "name": name})
rows.sort(key=lambda r: r["id"])
with open(out_path, "w") as f:
    json.dump(rows, f, indent=1)
    f.write("\n")
print(f"{len(rows)} consumables -> {out_path}")
