//! The comp's target build as tiles: what each slot wants and which icon to
//! draw. Pure, so the UI only lays it out.

use crate::gamedb::GameDb;
use crate::icons::{gear_icon, named, weapon_icon, weight_of, NamedKind};
use crate::model::{is_two_handed, Build, GearSlot, TraitSel};
use crate::report::SlotKey;
use crate::specs::SpecDb;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tile {
    pub key: SlotKey,
    /// Short slot label: "Head", "Acc 1", "Heal", "Rune".
    pub label: String,
    /// What the comp wants here: stat name, item/skill/trait name. Empty when `empty`.
    pub name: String,
    /// Second line: buff text for food/utility; None elsewhere.
    pub sub: Option<String>,
    pub icon: Option<String>,
    /// The comp leaves this slot unspecified (dashed, faded).
    pub empty: bool,
}

impl Default for Tile {
    fn default() -> Self {
        Tile { key: SlotKey::Relic, label: String::new(), name: String::new(), sub: None, icon: None, empty: true }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GearRow { pub tile: Tile, pub upgrades: Vec<Tile> }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WeaponSet { pub label: &'static str, pub main: GearRow, pub off: Option<GearRow>, pub two_handed: bool }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraitTile { pub tile: Tile, pub selected: bool }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpecCard {
    pub key: SlotKey,
    pub name: String,
    pub icon: Option<String>,
    pub background: Option<String>,
    pub minors: Vec<Tile>,              // 3, keys Spec(line)
    pub majors: [Vec<TraitTile>; 3],    // per tier, 3 choices each, keys Trait{line,tier}
    pub any: [bool; 3],                 // tier left unchosen (TraitSel::None)
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Loadout {
    pub skills: Vec<Tile>,         // always 5: heal, 3 utilities, elite
    pub specs: Vec<SpecCard>,      // lines with id != 0
    pub armor: Vec<GearRow>,       // always 6, GearSlot::ARMOR order; upgrades = [rune] when set
    pub weapons: Vec<WeaponSet>,   // sets the build defines (A, then B)
    pub trinkets: Vec<Tile>,       // Back, Acc1, Acc2, Amulet, Ring1, Ring2
    pub relic: Tile,
    pub infusions: Vec<Tile>,      // one per wanted infusion, key Infusions
    pub food: Tile,
    pub utility: Tile,
}

fn tile(key: SlotKey, label: &str, name: Option<String>, icon: Option<String>) -> Tile {
    let empty = name.is_none();
    Tile { key, label: label.into(), name: name.unwrap_or_default(), sub: None, icon, empty }
}

fn short_label(slot: GearSlot) -> &'static str {
    match slot {
        GearSlot::Accessory1 => "Acc 1",
        GearSlot::Accessory2 => "Acc 2",
        GearSlot::WeaponA1 | GearSlot::WeaponB1 => "Main",
        GearSlot::WeaponA2 | GearSlot::WeaponB2 => "Off",
        s => s.label(),
    }
}

impl Loadout {
    pub fn of(build: &Build, db: &GameDb, specs: &SpecDb) -> Loadout {
        let e = &build.equipment;
        let weight = weight_of(&build.profession);
        let skill = |k: u8, label: &str, id: u32| {
            let name = (id != 0).then(|| build.skill_names.get(&id).cloned().or_else(|| db.skill_name(id).map(String::from)).unwrap_or_else(|| format!("skill {id}")));
            tile(SlotKey::Skill(k), label, name, db.skill_icon(id).map(String::from))
        };
        let s = &build.skills;
        let skills = vec![
            skill(0, "Heal", s.heal),
            skill(1, "Utility", s.utilities[0]),
            skill(2, "Utility", s.utilities[1]),
            skill(3, "Utility", s.utilities[2]),
            skill(4, "Elite", s.elite),
        ];
        let item = |key: SlotKey, label: &str, id: u32| {
            tile(key, label, Some(db.item_name(id).map(String::from).unwrap_or_else(|| format!("item {id}"))), db.item_icon(id).map(String::from))
        };
        let gear = |slot: GearSlot| tile(SlotKey::Gear(slot), short_label(slot), e.stats.get(&slot).cloned(), gear_icon(slot, weight).map(String::from));
        let armor = GearSlot::ARMOR.iter().map(|&slot| GearRow {
            tile: gear(slot),
            upgrades: e.runes.get(&slot).map(|id| vec![item(SlotKey::Rune(slot), "Rune", *id)]).unwrap_or_default(),
        }).collect();
        let weapon_row = |slot: GearSlot, w: &str| {
            let mut t = tile(SlotKey::Gear(slot), short_label(slot), Some(w.to_string()), weapon_icon(w).map(String::from));
            if let Some(stat) = e.stats.get(&slot) {
                t.sub = Some(stat.clone());
            }
            let upgrades = e.sigils.get(slot).iter().enumerate().map(|(i, id)| item(SlotKey::Sigil(slot, i as u8), "Sigil", *id)).collect();
            GearRow { tile: t, upgrades }
        };
        let mut weapons = Vec::new();
        for (label, main_slot, off_slot) in [("A", GearSlot::WeaponA1, GearSlot::WeaponA2), ("B", GearSlot::WeaponB1, GearSlot::WeaponB2)] {
            let (main, off) = (e.weapons.get(main_slot), e.weapons.get(off_slot));
            if main.is_none() && off.is_none() {
                continue;
            }
            let two_handed = main.is_some_and(is_two_handed);
            let main_row = match main {
                Some(w) => weapon_row(main_slot, w),
                None => GearRow { tile: tile(SlotKey::Gear(main_slot), "Main", None, None), upgrades: vec![] },
            };
            let off_row = if two_handed { None } else { off.map(|w| weapon_row(off_slot, w)) };
            weapons.push(WeaponSet { label, main: main_row, off: off_row, two_handed });
        }
        let trinkets = [GearSlot::Back, GearSlot::Accessory1, GearSlot::Accessory2, GearSlot::Amulet, GearSlot::Ring1, GearSlot::Ring2].map(gear).to_vec();
        let named_tile = |key: SlotKey, label: &str, kind: NamedKind, want: &Option<String>| {
            let hit = want.as_deref().and_then(|n| named(kind, n));
            let mut t = tile(key, label, want.clone(), hit.map(|h| h.icon.clone()));
            t.sub = hit.map(|h| h.buff.clone()).filter(|b| !b.is_empty());
            t
        };
        let mut relic = named_tile(SlotKey::Relic, "Relic", NamedKind::Relic, &e.relic);
        relic.name = relic.name.trim_start_matches("Relic of the ").trim_start_matches("Relic of ").to_string();
        let infusions = e.infusions.iter().map(|id| item(SlotKey::Infusions, "Infusion", *id)).collect();
        let specs_cards = build.specs.iter().enumerate().filter(|(_, l)| l.id != 0).map(|(i, line)| {
            let info = specs.get(line.id);
            let tinfo = |t: u32| specs.trait_info(line.id, t);
            let minors = info.map(|s| s.minors.clone()).unwrap_or_default().into_iter().map(|t| {
                tile(SlotKey::Spec(i as u8), "Minor", tinfo(t).map(|x| x.name.clone()), tinfo(t).map(|x| x.icon.clone()))
            }).collect();
            let any: [bool; 3] = std::array::from_fn(|t| matches!(line.majors[t], TraitSel::None));
            let majors: [Vec<TraitTile>; 3] = std::array::from_fn(|tier| {
                let choices = info.map(|s| s.majors[tier].clone()).unwrap_or_default();
                let chosen = match line.majors[tier] {
                    TraitSel::None => None,
                    TraitSel::Id(id) => Some(id),
                    TraitSel::Position(p) => specs.trait_at(line.id, tier + 1, p),
                };
                choices.into_iter().map(|t| TraitTile {
                    tile: tile(SlotKey::Trait { line: i as u8, tier: tier as u8 }, "Major", tinfo(t).map(|x| x.name.clone()), tinfo(t).map(|x| x.icon.clone())),
                    selected: chosen == Some(t),
                }).collect()
            });
            SpecCard {
                key: SlotKey::Spec(i as u8),
                name: specs.name(line.id).map(String::from).unwrap_or_else(|| format!("spec {}", line.id)),
                icon: info.map(|s| s.icon.clone()).filter(|u| !u.is_empty()),
                background: info.map(|s| s.background.clone()).filter(|u| !u.is_empty()),
                minors,
                majors,
                any,
            }
        }).collect();
        Loadout {
            skills,
            specs: specs_cards,
            armor,
            weapons,
            trinkets,
            relic,
            infusions,
            food: named_tile(SlotKey::Food, "Food", NamedKind::Food, &e.food),
            utility: named_tile(SlotKey::Utility, "Utility", NamedKind::Utility, &e.utility),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gamedb::{GameDb, ItemInfo, SkillInfo};
    use crate::model::{GearSlot, TraitSel};
    use crate::specs::SpecDb;
    use crate::testutil::firebrand;

    fn db_with_icons(b: &Build) -> GameDb {
        let mut db = GameDb::default();
        for id in [b.skills.heal, b.skills.elite].into_iter().chain(b.skills.utilities).filter(|i| *i != 0) {
            db.skills.insert(id, SkillInfo { name: format!("S{id}"), icon: Some(format!("https://render.guildwars2.com/s/{id}.png")), icon_checked: true, ..Default::default() });
        }
        for id in b.equipment.runes.values().copied().chain(b.equipment.infusions.iter().copied()) {
            db.items.insert(id, ItemInfo { name: format!("I{id}"), icon: Some(format!("https://render.guildwars2.com/i/{id}.png")), icon_checked: true, ..Default::default() });
        }
        db
    }

    #[test]
    fn skill_bar_has_five_tiles_with_icons() {
        let b = firebrand();
        let l = Loadout::of(&b, &db_with_icons(&b), SpecDb::bundled());
        assert_eq!(l.skills.len(), 5);
        assert_eq!(l.skills[0].key, SlotKey::Skill(0));
        assert_eq!(l.skills[0].icon.as_deref(), Some(format!("https://render.guildwars2.com/s/{}.png", b.skills.heal).as_str()));
        assert_eq!(l.skills[4].label, "Elite");
    }

    #[test]
    fn armor_rows_use_weight_icons_and_rune_chips() {
        let b = firebrand();
        let l = Loadout::of(&b, &db_with_icons(&b), SpecDb::bundled());
        assert_eq!(l.armor.len(), 6);
        let head = &l.armor[0];
        assert_eq!(head.tile.key, SlotKey::Gear(GearSlot::Head));
        assert_eq!(head.tile.icon.as_deref(), crate::icons::gear_icon(GearSlot::Head, Some(crate::icons::Weight::Heavy)));
        assert_eq!(head.tile.name, b.equipment.stats[&GearSlot::Head]);
        if b.equipment.runes.contains_key(&GearSlot::Head) {
            assert_eq!(head.upgrades[0].key, SlotKey::Rune(GearSlot::Head));
            assert!(head.upgrades[0].icon.is_some());
        }
    }

    #[test]
    fn spec_cards_carry_art_and_selection() {
        let b = firebrand();
        let l = Loadout::of(&b, &GameDb::default(), SpecDb::bundled());
        assert_eq!(l.specs.len(), 3);
        let fb = &l.specs[2];
        assert_eq!(fb.name, "Firebrand");
        assert!(fb.icon.is_some() && fb.background.is_some());
        assert_eq!(fb.minors.len(), 3);
        for t in 0..3 {
            assert_eq!(fb.majors[t].len(), 3);
            if !fb.any[t] {
                assert_eq!(fb.majors[t].iter().filter(|x| x.selected).count(), 1, "tier {t}");
            }
            assert!(fb.majors[t].iter().all(|x| x.tile.key == SlotKey::Trait { line: 2, tier: t as u8 }));
        }
    }

    #[test]
    fn unchosen_tier_is_any() {
        let mut b = firebrand();
        b.specs[0].majors[1] = TraitSel::None;
        let l = Loadout::of(&b, &GameDb::default(), SpecDb::bundled());
        assert!(l.specs[0].any[1]);
        assert!(l.specs[0].majors[1].iter().all(|x| !x.selected));
    }

    #[test]
    fn two_handed_set_has_no_offhand_tile() {
        let mut b = firebrand();
        b.equipment.weapons.a1 = Some("greatsword".into());
        b.equipment.weapons.a2 = Some("shield".into());
        let l = Loadout::of(&b, &GameDb::default(), SpecDb::bundled());
        let a = l.weapons.iter().find(|w| w.label == "A").unwrap();
        assert!(a.two_handed && a.off.is_none());
        assert_eq!(a.main.tile.icon.as_deref(), crate::icons::weapon_icon("greatsword"));
    }

    #[test]
    fn unspecified_slots_are_empty_tiles() {
        let mut b = firebrand();
        b.equipment.relic = None;
        b.equipment.food = None;
        b.equipment.stats.remove(&GearSlot::Back);
        let l = Loadout::of(&b, &GameDb::default(), SpecDb::bundled());
        assert!(l.relic.empty && l.food.empty);
        assert!(l.trinkets[0].empty, "Back first");
    }

    #[test]
    fn named_relic_and_food_get_icons() {
        let b = firebrand();
        let l = Loadout::of(&b, &GameDb::default(), SpecDb::bundled());
        if b.equipment.food.is_some() {
            assert!(l.food.icon.is_some(), "food {:?}", b.equipment.food);
            assert!(l.food.sub.is_some());
        }
        if b.equipment.relic.is_some() {
            assert!(l.relic.icon.is_some(), "relic {:?}", b.equipment.relic);
        }
    }
}
