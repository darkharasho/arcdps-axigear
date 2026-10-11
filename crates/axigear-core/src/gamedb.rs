//! Item, itemstat and skill names from the GW2 API, cached on disk forever
//! (`itemdb.json`). Checks compare IDs where they can and names where they must.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::gw2api::{ApiItem, ApiSnapshot, API_ROOT};
use crate::http::Http;
use crate::model::Build;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemInfo {
    pub name: String,
    #[serde(default)]
    pub weapon_type: Option<String>,
    /// `details.infix_upgrade.id` for fixed-stat items.
    #[serde(default)]
    pub default_stats: Option<u32>,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub icon_checked: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillInfo {
    pub name: String,
    #[serde(default)]
    pub weapon_type: Option<String>,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub icon_checked: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Wanted {
    pub items: BTreeSet<u32>,
    pub itemstats: BTreeSet<u32>,
    pub skills: BTreeSet<u32>,
}

impl Wanted {
    pub fn is_empty(&self) -> bool {
        self.items.is_empty() && self.itemstats.is_empty() && self.skills.is_empty()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameDb {
    #[serde(default)]
    pub items: BTreeMap<u32, ItemInfo>,
    #[serde(default)]
    pub itemstats: BTreeMap<u32, String>,
    #[serde(default)]
    pub skills: BTreeMap<u32, SkillInfo>,
}

impl GameDb {
    pub fn load(path: &Path) -> GameDb {
        std::fs::read_to_string(path).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let json = serde_json::to_vec(self).map_err(std::io::Error::other)?;
        crate::fsutil::write_atomic(path, &json)
    }

    pub fn item_name(&self, id: u32) -> Option<&str> {
        self.items.get(&id).map(|i| i.name.as_str())
    }

    /// Weapon type in AxiForge's names. The API still calls spears "Harpoon"
    /// and harpoon guns "Speargun", from before spears became land weapons.
    pub fn weapon_type(&self, item_id: u32) -> Option<&str> {
        match self.items.get(&item_id)?.weapon_type.as_deref()? {
            "Harpoon" => Some("Spear"),
            "Speargun" => Some("Harpoon"),
            other => Some(other),
        }
    }

    pub fn item_icon(&self, id: u32) -> Option<&str> {
        self.items.get(&id)?.icon.as_deref()
    }

    pub fn skill_icon(&self, id: u32) -> Option<&str> {
        self.skills.get(&id)?.icon.as_deref()
    }

    pub fn skill_name(&self, id: u32) -> Option<&str> {
        self.skills.get(&id).map(|s| s.name.as_str())
    }

    /// Itemstat name for an equipped item: its selected stats, else the item's own.
    pub fn stat_name(&self, item: &ApiItem) -> Option<&str> {
        let id = item.stats_id.or_else(|| self.items.get(&item.id).and_then(|i| i.default_stats))?;
        self.itemstats.get(&id).map(String::as_str)
    }

    /// IDs the checks will need names for that aren't cached yet.
    pub fn wanted(&self, build: Option<&Build>, snap: Option<&ApiSnapshot>, cast: &BTreeSet<u32>) -> Wanted {
        let mut w = Wanted::default();
        if let Some(b) = build {
            let e = &b.equipment;
            w.items.extend(e.runes.values().copied());
            for s in [&e.sigils.a1, &e.sigils.a2, &e.sigils.b1, &e.sigils.b2] {
                w.items.extend(s.iter().copied());
            }
            w.items.extend(e.infusions.iter().copied());
            w.items.extend([&e.food, &e.utility].into_iter().flatten().filter_map(|l| crate::model::consumable_item_id(l)));
            let skills = [b.skills.heal, b.skills.elite].into_iter().chain(b.skills.utilities);
            w.skills.extend(skills.filter(|id| *id != 0));
        }
        if let Some(s) = snap {
            for i in &s.equipment {
                w.items.insert(i.id);
                w.items.extend(i.upgrades.iter().copied());
                w.items.extend(i.infusions.iter().copied());
                if let Some(id) = i.stats_id.or_else(|| self.items.get(&i.id).and_then(|x| x.default_stats)) {
                    w.itemstats.insert(id);
                }
            }
            let sk = &s.build.skills;
            w.skills.extend([sk.heal, sk.elite].into_iter().chain(sk.utilities).filter(|id| *id != 0));
        }
        w.skills.extend(cast.iter().copied());
        w.items.retain(|id| self.items.get(id).map_or(true, |i| !i.icon_checked));
        w.itemstats.retain(|id| !self.itemstats.contains_key(id));
        w.skills.retain(|id| self.skills.get(id).map_or(true, |s| !s.icon_checked));
        w
    }

    /// Fetch everything in `wanted` (200 IDs per request), then the itemstats
    /// the new items point at. Unknown IDs are simply left out.
    pub fn resolve(&mut self, http: &dyn Http, wanted: &Wanted) -> Result<(), String> {
        for v in fetch_all(http, "items", &wanted.items)? {
            let Some(id) = v["id"].as_u64() else { continue };
            let weapon_type = (v["type"] == "Weapon").then(|| v["details"]["type"].as_str().map(String::from)).flatten();
            self.items.insert(
                id as u32,
                ItemInfo {
                    name: v["name"].as_str().unwrap_or("").to_string(),
                    weapon_type,
                    default_stats: v["details"]["infix_upgrade"]["id"].as_u64().map(|x| x as u32),
                    icon: icon_of(&v),
                    icon_checked: true,
                },
            );
        }
        let mut stats = wanted.itemstats.clone();
        stats.extend(wanted.items.iter().filter_map(|id| self.items.get(id)?.default_stats));
        stats.retain(|id| !self.itemstats.contains_key(id));
        for v in fetch_all(http, "itemstats", &stats)? {
            if let (Some(id), Some(name)) = (v["id"].as_u64(), v["name"].as_str()) {
                self.itemstats.insert(id as u32, name.to_string());
            }
        }
        for v in fetch_all(http, "skills", &wanted.skills)? {
            let Some(id) = v["id"].as_u64() else { continue };
            let weapon_type = v["weapon_type"].as_str().filter(|w| *w != "None").map(String::from);
            self.skills.insert(id as u32, SkillInfo {
                name: v["name"].as_str().unwrap_or("").to_string(),
                weapon_type,
                icon: icon_of(&v),
                icon_checked: true,
            });
        }
        Ok(())
    }
}

fn icon_of(v: &Value) -> Option<String> {
    v["icon"].as_str().filter(|s| !s.is_empty()).map(String::from)
}

fn fetch_all(http: &dyn Http, endpoint: &str, ids: &BTreeSet<u32>) -> Result<Vec<Value>, String> {
    let ids: Vec<u32> = ids.iter().copied().collect();
    let mut out = Vec::new();
    for chunk in ids.chunks(200) {
        let list = chunk.iter().map(u32::to_string).collect::<Vec<_>>().join(",");
        let resp = http.get(&format!("{API_ROOT}/v2/{endpoint}?ids={list}"), &[])?;
        match resp.status {
            200 | 206 => out.extend(serde_json::from_slice::<Vec<Value>>(&resp.body).map_err(|e| e.to_string())?),
            404 => {} // "all ids provided are invalid"
            s => return Err(format!("HTTP {s} from /v2/{endpoint}")),
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gw2api::{ApiBuild, ApiSkills};
    use crate::http::fake::FakeHttp;
    use crate::model::GearSlot;
    use crate::testutil::firebrand;
    use std::time::Instant;

    #[test]
    fn numeric_food_and_utility_are_wanted() {
        let mut b = crate::testutil::firebrand();
        b.equipment.food = Some("91835".into());
        b.equipment.utility = Some("Superior Sharpening Stone".into());
        let w = GameDb::default().wanted(Some(&b), None, &BTreeSet::new());
        assert!(w.items.contains(&91835));
    }

    fn snap(items: Vec<ApiItem>) -> ApiSnapshot {
        ApiSnapshot {
            fetched_at: Instant::now(),
            character: "A".into(),
            build: ApiBuild { profession: "Guardian".into(), specs: vec![], skills: ApiSkills { heal: 41714, ..Default::default() } },
            equipment: items,
        }
    }

    fn item(id: u32, slot: &str, stats_id: Option<u32>) -> ApiItem {
        ApiItem { id, slot: slot.into(), upgrades: vec![24842], infusions: vec![37133], stats_id }
    }

    #[test]
    fn wants_what_the_build_and_snapshot_reference() {
        let db = GameDb::default();
        let w = db.wanted(Some(&firebrand()), Some(&snap(vec![item(100, "Helm", Some(7))])), &[555].into());
        assert!(w.items.contains(&24842) && w.items.contains(&24865) && w.items.contains(&100));
        assert!(w.items.contains(&86180), "infusions");
        assert_eq!(w.itemstats, [7].into());
        assert!(w.skills.contains(&41714) && w.skills.contains(&555));
    }

    #[test]
    fn api_spear_names_map_to_axiforge_names() {
        let mut db = GameDb::default();
        for (id, ty) in [(1, "Harpoon"), (2, "Speargun"), (3, "LongBow")] {
            db.items.insert(id, ItemInfo { weapon_type: Some(ty.into()), ..Default::default() });
        }
        assert_eq!(db.weapon_type(1), Some("Spear"));
        assert_eq!(db.weapon_type(2), Some("Harpoon"));
        assert_eq!(db.weapon_type(3), Some("LongBow"));
    }

    #[test]
    fn known_ids_are_not_wanted_again() {
        let mut db = GameDb::default();
        db.items.insert(100, ItemInfo { name: "Helm".into(), weapon_type: None, default_stats: Some(5), icon_checked: true, ..Default::default() });
        db.skills.insert(41714, SkillInfo { name: "Heal".into(), weapon_type: None, icon_checked: true, ..Default::default() });
        let w = db.wanted(None, Some(&snap(vec![item(100, "Helm", None)])), &BTreeSet::new());
        assert!(!w.items.contains(&100));
        assert_eq!(w.itemstats, [5].into(), "default stats of a known item");
        assert!(!w.skills.contains(&41714));
    }

    #[test]
    fn resolves_items_then_their_stats() {
        let http = FakeHttp::new();
        http.on(
            "https://api.guildwars2.com/v2/items?ids=100,200",
            200,
            r#"[{"id":100,"name":"Helm","type":"Armor","details":{"infix_upgrade":{"id":5}}},
                {"id":200,"name":"Sword","type":"Weapon","details":{"type":"Sword"}}]"#,
        )
        .on("https://api.guildwars2.com/v2/itemstats?ids=5,7", 200, r#"[{"id":5,"name":"Berserker's"},{"id":7,"name":"Minstrel's"}]"#)
        .on("https://api.guildwars2.com/v2/skills?ids=9153", 206, r#"[{"id":9153,"name":"Mantra of Potence","weapon_type":"None"}]"#);
        let mut db = GameDb::default();
        let wanted = Wanted { items: [100, 200].into(), itemstats: [7].into(), skills: [9153].into() };
        db.resolve(&http, &wanted).unwrap();
        assert_eq!(db.stat_name(&item(100, "Helm", None)), Some("Berserker's"));
        assert_eq!(db.stat_name(&item(200, "WeaponA1", Some(7))), Some("Minstrel's"));
        assert_eq!(db.weapon_type(200), Some("Sword"));
        assert_eq!(db.skills[&9153].weapon_type, None, "\"None\" means no weapon");
    }

    #[test]
    fn all_unknown_ids_is_not_an_error_but_a_500_is() {
        let http = FakeHttp::new();
        let mut db = GameDb::default();
        db.resolve(&http, &Wanted { items: [1].into(), ..Default::default() }).unwrap();
        assert!(db.items.is_empty());
        http.on("https://api.guildwars2.com/v2/items?ids=2", 500, "");
        assert!(db.resolve(&http, &Wanted { items: [2].into(), ..Default::default() }).is_err());
    }

    #[test]
    fn batches_of_two_hundred() {
        let http = FakeHttp::new();
        let mut db = GameDb::default();
        db.resolve(&http, &Wanted { items: (1..=201).collect(), ..Default::default() }).unwrap();
        assert_eq!(http.calls().len(), 2);
        assert!(http.calls()[1].ends_with("ids=201"));
    }

    #[test]
    fn save_and_load_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("axigear/itemdb.json");
        let mut db = GameDb::default();
        db.itemstats.insert(5, "Berserker's".into());
        db.save(&path).unwrap();
        assert_eq!(GameDb::load(&path), db);
        std::fs::write(&path, "{broken").unwrap();
        assert_eq!(GameDb::load(&path), GameDb::default());
    }

    #[test]
    fn icons_are_parsed_and_marked_checked() {
        let http = FakeHttp::new();
        http.on("https://api.guildwars2.com/v2/items?ids=24842", 200,
            r#"[{"id":24842,"name":"Superior Rune of the Scholar","type":"UpgradeComponent","icon":"https://render.guildwars2.com/file/A/1.png","details":{}}]"#)
        .on("https://api.guildwars2.com/v2/skills?ids=9153", 200,
            r#"[{"id":9153,"name":"Mantra of Potence","icon":"https://render.guildwars2.com/file/B/2.png"}]"#);
        let mut db = GameDb::default();
        let w = Wanted { items: [24842].into(), skills: [9153].into(), ..Default::default() };
        db.resolve(&http, &w).unwrap();
        assert_eq!(db.item_icon(24842), Some("https://render.guildwars2.com/file/A/1.png"));
        assert_eq!(db.skill_icon(9153), Some("https://render.guildwars2.com/file/B/2.png"));
        assert!(db.items[&24842].icon_checked && db.skills[&9153].icon_checked);
    }

    #[test]
    fn old_entries_are_rewanted_once_for_icons() {
        // An itemdb.json written by v0.1.x: no icon / icon_checked fields.
        let mut db: GameDb = serde_json::from_str(
            r#"{"items":{"24842":{"name":"Rune"}},"skills":{"9153":{"name":"Mantra"}}}"#).unwrap();
        let mut b = firebrand();
        b.equipment.runes.clear();
        b.equipment.runes.insert(GearSlot::Head, 24842);
        let w = db.wanted(Some(&b), None, &BTreeSet::new());
        assert!(w.items.contains(&24842), "old item without icon_checked is wanted");
        // The API answers with no icon at all; it must not be wanted again.
        let http = FakeHttp::new();
        http.on(&format!("https://api.guildwars2.com/v2/items?ids={}", join(&w.items)), 200, r#"[{"id":24842,"name":"Rune"}]"#)
            .on(&format!("https://api.guildwars2.com/v2/skills?ids={}", join(&w.skills)), 200, r#"[{"id":9153,"name":"Mantra"}]"#);
        db.resolve(&http, &Wanted { itemstats: Default::default(), ..w.clone() }).unwrap();
        let again = db.wanted(Some(&b), None, &BTreeSet::new());
        assert!(!again.items.contains(&24842));
        assert!(!again.skills.contains(&9153));
    }

    #[test]
    fn named_build_skills_are_still_wanted_for_icons() {
        let mut b = firebrand();
        b.skill_names.insert(b.skills.heal, "Named Heal".into());
        let w = GameDb::default().wanted(Some(&b), None, &BTreeSet::new());
        assert!(w.skills.contains(&b.skills.heal));
    }

    fn join(ids: &BTreeSet<u32>) -> String {
        ids.iter().map(u32::to_string).collect::<Vec<_>>().join(",")
    }
}
