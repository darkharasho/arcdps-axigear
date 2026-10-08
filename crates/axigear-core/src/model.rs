//! What a comp asks for. Built by `axicode` (codes) and `publish` (links).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::raw::RawEquipment;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum GameMode {
    Pve,
    Pvp,
    Wvw,
}

impl GameMode {
    pub fn parse(s: &str) -> Option<GameMode> {
        match s.trim().to_ascii_lowercase().as_str() {
            "pve" => Some(GameMode::Pve),
            "pvp" => Some(GameMode::Pvp),
            "wvw" => Some(GameMode::Wvw),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum GearSlot {
    Head,
    Shoulders,
    Chest,
    Hands,
    Legs,
    Feet,
    Back,
    Amulet,
    Ring1,
    Ring2,
    Accessory1,
    Accessory2,
    WeaponA1,
    WeaponA2,
    WeaponB1,
    WeaponB2,
}

impl GearSlot {
    pub const ARMOR: [GearSlot; 6] = [
        GearSlot::Head, GearSlot::Shoulders, GearSlot::Chest,
        GearSlot::Hands, GearSlot::Legs, GearSlot::Feet,
    ];
    pub const ALL: [GearSlot; 16] = [
        GearSlot::Head, GearSlot::Shoulders, GearSlot::Chest, GearSlot::Hands,
        GearSlot::Legs, GearSlot::Feet, GearSlot::Back, GearSlot::Amulet,
        GearSlot::Ring1, GearSlot::Ring2, GearSlot::Accessory1, GearSlot::Accessory2,
        GearSlot::WeaponA1, GearSlot::WeaponA2, GearSlot::WeaponB1, GearSlot::WeaponB2,
    ];

    /// Key AxiForge uses in `slots`, `runes`, `sigils`, `infusions`, `weapons`.
    pub fn axiforge_key(self) -> &'static str {
        match self {
            GearSlot::Head => "head",
            GearSlot::Shoulders => "shoulders",
            GearSlot::Chest => "chest",
            GearSlot::Hands => "hands",
            GearSlot::Legs => "legs",
            GearSlot::Feet => "feet",
            GearSlot::Back => "back",
            GearSlot::Amulet => "amulet",
            GearSlot::Ring1 => "ring1",
            GearSlot::Ring2 => "ring2",
            GearSlot::Accessory1 => "accessory1",
            GearSlot::Accessory2 => "accessory2",
            GearSlot::WeaponA1 => "mainhand1",
            GearSlot::WeaponA2 => "offhand1",
            GearSlot::WeaponB1 => "mainhand2",
            GearSlot::WeaponB2 => "offhand2",
        }
    }

    /// `slot` value in GW2 API equipment entries.
    pub fn from_api_slot(s: &str) -> Option<GearSlot> {
        Some(match s {
            "Helm" => GearSlot::Head,
            "Shoulders" => GearSlot::Shoulders,
            "Coat" => GearSlot::Chest,
            "Gloves" => GearSlot::Hands,
            "Leggings" => GearSlot::Legs,
            "Boots" => GearSlot::Feet,
            "Backpack" => GearSlot::Back,
            "Amulet" => GearSlot::Amulet,
            "Ring1" => GearSlot::Ring1,
            "Ring2" => GearSlot::Ring2,
            "Accessory1" => GearSlot::Accessory1,
            "Accessory2" => GearSlot::Accessory2,
            "WeaponA1" => GearSlot::WeaponA1,
            "WeaponA2" => GearSlot::WeaponA2,
            "WeaponB1" => GearSlot::WeaponB1,
            "WeaponB2" => GearSlot::WeaponB2,
            _ => return None,
        })
    }

    pub fn label(self) -> &'static str {
        match self {
            GearSlot::Head => "Head",
            GearSlot::Shoulders => "Shoulders",
            GearSlot::Chest => "Chest",
            GearSlot::Hands => "Hands",
            GearSlot::Legs => "Legs",
            GearSlot::Feet => "Feet",
            GearSlot::Back => "Back",
            GearSlot::Amulet => "Amulet",
            GearSlot::Ring1 => "Ring 1",
            GearSlot::Ring2 => "Ring 2",
            GearSlot::Accessory1 => "Accessory 1",
            GearSlot::Accessory2 => "Accessory 2",
            GearSlot::WeaponA1 => "Weapon A1",
            GearSlot::WeaponA2 => "Weapon A2",
            GearSlot::WeaponB1 => "Weapon B1",
            GearSlot::WeaponB2 => "Weapon B2",
        }
    }

    pub fn is_weapon(self) -> bool {
        matches!(self, GearSlot::WeaponA1 | GearSlot::WeaponA2 | GearSlot::WeaponB1 | GearSlot::WeaponB2)
    }
}

/// AxiForge weapon types that occupy both hands.
pub fn is_two_handed(weapon: &str) -> bool {
    matches!(
        weapon.to_ascii_lowercase().as_str(),
        "greatsword" | "hammer" | "longbow" | "rifle" | "shortbow" | "staff" | "spear"
    )
}

/// A major trait choice. Codes store a position (1 top, 2 middle, 3 bottom);
/// published records usually store the trait ID.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TraitSel {
    #[default]
    None,
    Position(u8),
    Id(u32),
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpecLine {
    /// 0 = empty line.
    pub id: u16,
    /// Known for published records; `None` for codes (look it up in `SpecDb`).
    pub elite: Option<bool>,
    pub majors: [TraitSel; 3],
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillBar {
    pub heal: u32,
    pub utilities: [u32; 3],
    pub elite: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Weapons {
    pub a1: Option<String>,
    pub a2: Option<String>,
    pub b1: Option<String>,
    pub b2: Option<String>,
}

impl Weapons {
    pub fn get(&self, slot: GearSlot) -> Option<&str> {
        match slot {
            GearSlot::WeaponA1 => self.a1.as_deref(),
            GearSlot::WeaponA2 => self.a2.as_deref(),
            GearSlot::WeaponB1 => self.b1.as_deref(),
            GearSlot::WeaponB2 => self.b2.as_deref(),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sigils {
    pub a1: Vec<u32>,
    pub a2: Vec<u32>,
    pub b1: Vec<u32>,
    pub b2: Vec<u32>,
}

impl Sigils {
    pub fn get(&self, slot: GearSlot) -> &[u32] {
        match slot {
            GearSlot::WeaponA1 => &self.a1,
            GearSlot::WeaponA2 => &self.a2,
            GearSlot::WeaponB1 => &self.b1,
            GearSlot::WeaponB2 => &self.b2,
            _ => &[],
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Equipment {
    /// Itemstat name per slot. Weapon slots only when that weapon exists.
    pub stats: BTreeMap<GearSlot, String>,
    pub weapons: Weapons,
    /// Rune item ID per armor slot; slots without a rune are absent.
    pub runes: BTreeMap<GearSlot, u32>,
    pub sigils: Sigils,
    pub relic: Option<String>,
    pub food: Option<String>,
    pub utility: Option<String>,
    /// Every land infusion item ID, sorted. Weapons count 2 slots two-handed, 1 otherwise.
    pub infusions: Vec<u32>,
}

impl Equipment {
    pub fn from_raw(raw: &RawEquipment) -> Equipment {
        fn text(s: &str) -> Option<String> {
            let t = s.trim();
            (!t.is_empty()).then(|| t.to_string())
        }
        fn id(s: &str) -> Option<u32> {
            s.trim().parse::<u32>().ok().filter(|v| *v != 0)
        }
        let weapon = |key: &str| raw.weapons.get(key).and_then(|s| text(s)).map(|s| s.to_lowercase());
        let weapons = Weapons {
            a1: weapon("mainhand1"),
            a2: weapon("offhand1"),
            b1: weapon("mainhand2"),
            b2: weapon("offhand2"),
        };

        let mut stats = BTreeMap::new();
        for slot in GearSlot::ALL {
            if slot.is_weapon() && weapons.get(slot).is_none() {
                continue;
            }
            let stat = raw
                .slots
                .get(slot.axiforge_key())
                .and_then(|s| text(s))
                .or_else(|| text(&raw.stat_package));
            if let Some(stat) = stat {
                stats.insert(slot, stat);
            }
        }

        let runes = GearSlot::ARMOR
            .iter()
            .filter_map(|s| raw.runes.get(s.axiforge_key()).and_then(|v| id(v)).map(|v| (*s, v)))
            .collect();

        let sigil = |key: &str| -> Vec<u32> {
            raw.sigils.get(key).map(|v| v.iter().filter_map(|s| id(s)).collect()).unwrap_or_default()
        };
        let sigils = Sigils { a1: sigil("mainhand1"), a2: sigil("offhand1"), b1: sigil("mainhand2"), b2: sigil("offhand2") };

        let mut infusions = Vec::new();
        for slot in GearSlot::ALL {
            let capacity = match slot {
                s if s.is_weapon() => match weapons.get(s) {
                    None => 0,
                    Some(w) if is_two_handed(w) => 2,
                    Some(_) => 1,
                },
                GearSlot::Back => 2,
                GearSlot::Ring1 | GearSlot::Ring2 => 3,
                GearSlot::Amulet => 0,
                _ => 1,
            };
            if let Some(v) = raw.infusions.get(slot.axiforge_key()) {
                infusions.extend(v.as_slice().iter().take(capacity).filter_map(|s| id(s)));
            }
        }
        infusions.sort_unstable();

        Equipment {
            stats,
            weapons,
            runes,
            sigils,
            relic: text(&raw.relic),
            food: text(&raw.food),
            utility: text(&raw.utility),
            infusions,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Build {
    pub title: Option<String>,
    pub profession: String,
    pub game_mode: GameMode,
    pub specs: [SpecLine; 3],
    pub skills: SkillBar,
    /// Skill names when the source carried them (published records).
    pub skill_names: BTreeMap<u32, String>,
    pub equipment: Equipment,
}

impl Build {
    pub fn display_name(&self) -> String {
        self.title.clone().unwrap_or_else(|| self.profession.clone())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SlotRef {
    pub line: usize,
    pub slot: usize,
    /// Index into `Comp::builds` (for tag slots, the build picked from the tag).
    pub build: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SlotEntry {
    Build(usize),
    /// A tag slot: any of these builds fills it.
    Tag { name: String, builds: Vec<usize> },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PartyLine {
    pub capacity: u8,
    pub slots: Vec<SlotEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Comp {
    pub name: String,
    pub game_mode: Option<GameMode>,
    pub builds: Vec<Build>,
    pub lines: Vec<PartyLine>,
}

impl Comp {
    /// A lone build code or build link, as a one-slot comp.
    pub fn single(build: Build) -> Comp {
        Comp {
            name: build.display_name(),
            game_mode: Some(build.game_mode),
            builds: vec![build],
            lines: vec![PartyLine { capacity: 1, slots: vec![SlotEntry::Build(0)] }],
        }
    }

    /// Every (line, slot, build) a player could be assigned, tag slots expanded.
    pub fn candidates(&self) -> Vec<SlotRef> {
        let mut out = Vec::new();
        for (line, pl) in self.lines.iter().enumerate() {
            for (slot, entry) in pl.slots.iter().enumerate() {
                match entry {
                    SlotEntry::Build(b) => out.push(SlotRef { line, slot, build: *b }),
                    SlotEntry::Tag { builds, .. } => {
                        out.extend(builds.iter().map(|b| SlotRef { line, slot, build: *b }))
                    }
                }
            }
        }
        out
    }

    pub fn is_valid(&self, r: SlotRef) -> bool {
        r.build < self.builds.len()
            && match self.lines.get(r.line).and_then(|l| l.slots.get(r.slot)) {
                Some(SlotEntry::Build(b)) => *b == r.build,
                Some(SlotEntry::Tag { builds, .. }) => builds.contains(&r.build),
                None => false,
            }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::raw::OneOrMany;

    fn map(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    fn greatsword_axe() -> RawEquipment {
        RawEquipment {
            stat_package: "Berserker's".into(),
            weapons: map(&[("mainhand1", "Greatsword"), ("offhand1", ""), ("mainhand2", "axe")]),
            runes: map(&[("head", "24836"), ("shoulders", "0"), ("chest", "24836")]),
            sigils: [
                ("mainhand1".to_string(), vec!["24615".to_string(), "24868".to_string()]),
                ("mainhand2".to_string(), vec!["24615".to_string(), "0".to_string()]),
            ]
            .into(),
            infusions: [
                ("head".to_string(), OneOrMany::One("49432".into())),
                ("back".to_string(), OneOrMany::Many(vec!["49432".into(), "".into()])),
                ("mainhand1".to_string(), OneOrMany::Many(vec!["49432".into(), "49432".into()])),
                ("mainhand2".to_string(), OneOrMany::Many(vec!["49432".into(), "49432".into()])),
                ("offhand2".to_string(), OneOrMany::Many(vec!["49432".into()])),
            ]
            .into(),
            food: " ".into(),
            relic: "Relic of the Thief".into(),
            ..Default::default()
        }
    }

    #[test]
    fn uniform_stat_package_fills_every_slot_that_exists() {
        let eq = Equipment::from_raw(&greatsword_axe());
        // 12 armor/trinket slots + A1 + B1; no A2 (two-hander) and no B2.
        assert_eq!(eq.stats.len(), 14);
        assert!(eq.stats.values().all(|s| s == "Berserker's"));
        assert!(!eq.stats.contains_key(&GearSlot::WeaponA2));
        assert!(!eq.stats.contains_key(&GearSlot::WeaponB2));
    }

    #[test]
    fn per_slot_stats_override_the_package() {
        let mut raw = greatsword_axe();
        raw.slots = map(&[("ring1", "Assassin's")]);
        let eq = Equipment::from_raw(&raw);
        assert_eq!(eq.stats[&GearSlot::Ring1], "Assassin's");
        assert_eq!(eq.stats[&GearSlot::Ring2], "Berserker's");
    }

    #[test]
    fn weapons_are_lowercased_and_blank_is_none() {
        let eq = Equipment::from_raw(&greatsword_axe());
        assert_eq!(eq.weapons.a1.as_deref(), Some("greatsword"));
        assert_eq!(eq.weapons.a2, None);
        assert_eq!(eq.weapons.b1.as_deref(), Some("axe"));
    }

    #[test]
    fn zero_and_blank_ids_are_dropped() {
        let eq = Equipment::from_raw(&greatsword_axe());
        assert_eq!(eq.runes.len(), 2);
        assert_eq!(eq.sigils.a1, vec![24615, 24868]);
        assert_eq!(eq.sigils.b1, vec![24615]);
        assert_eq!(eq.food, None);
        assert_eq!(eq.relic.as_deref(), Some("Relic of the Thief"));
    }

    #[test]
    fn infusions_respect_real_slot_counts() {
        let eq = Equipment::from_raw(&greatsword_axe());
        // head 1 + back 1 (blank dropped) + greatsword 2 + axe capped to 1; offhand2 has no weapon.
        assert_eq!(eq.infusions, vec![49432; 5]);
    }

    fn build(profession: &str) -> Build {
        Build {
            title: None,
            profession: profession.into(),
            game_mode: GameMode::Wvw,
            specs: Default::default(),
            skills: SkillBar::default(),
            skill_names: BTreeMap::new(),
            equipment: Equipment::default(),
        }
    }

    #[test]
    fn candidates_expand_tag_slots() {
        let comp = Comp {
            name: "c".into(),
            game_mode: None,
            builds: vec![build("Guardian"), build("Warrior"), build("Necromancer")],
            lines: vec![PartyLine {
                capacity: 5,
                slots: vec![
                    SlotEntry::Build(0),
                    SlotEntry::Tag { name: "DPS".into(), builds: vec![1, 2] },
                ],
            }],
        };
        assert_eq!(
            comp.candidates(),
            vec![
                SlotRef { line: 0, slot: 0, build: 0 },
                SlotRef { line: 0, slot: 1, build: 1 },
                SlotRef { line: 0, slot: 1, build: 2 },
            ]
        );
        assert!(comp.is_valid(SlotRef { line: 0, slot: 1, build: 2 }));
        assert!(!comp.is_valid(SlotRef { line: 0, slot: 1, build: 0 }));
        assert!(!comp.is_valid(SlotRef { line: 3, slot: 0, build: 0 }));
    }

    #[test]
    fn single_wraps_one_build() {
        let comp = Comp::single(build("Guardian"));
        assert_eq!(comp.candidates(), vec![SlotRef { line: 0, slot: 0, build: 0 }]);
        assert_eq!(comp.game_mode, Some(GameMode::Wvw));
    }
}
