//! AxiCode wire tables (append-only in AxiForge). Index 0 is always "empty".

use once_cell::sync::Lazy;

use crate::model::GameMode;

const PROFESSIONS: [&str; 9] = [
    "Guardian", "Warrior", "Engineer", "Ranger", "Thief",
    "Elementalist", "Mesmer", "Necromancer", "Revenant",
];

const WEAPONS: [&str; 20] = [
    "", "axe", "dagger", "mace", "pistol", "sword", "scepter", "focus", "shield", "torch",
    "warhorn", "greatsword", "hammer", "longbow", "rifle", "shortbow", "staff", "harpoon",
    "spear", "trident",
];

const STATS: [&str; 41] = [
    "", "Berserker's", "Marauder's", "Assassin's", "Valkyrie", "Dragon's", "Viper's",
    "Grieving", "Sinister", "Dire", "Rabid", "Carrion", "Trailblazer's", "Knight's",
    "Soldier's", "Cleric's", "Minstrel's", "Harrier's", "Ritualist's", "Seraph", "Zealot's",
    "Celestial", "Sentinel's", "Wanderer's", "Diviner's", "Crusader", "Giver's",
    "Apothecary's", "Magi's", "Shaman's", "Rampager's", "Cavalier's", "Nomad's", "Settler's",
    "Captain's", "Vigilant", "Apostate's", "Plaguedoctor's", "Marshal's", "Demolisher",
    "Commander's",
];

pub const FOOD: [&str; 15] = [
    "",
    "Peppercorn-Crusted Sous-Vide Steak",
    "Cilantro Lime Sous-Vide Steak",
    "Bowl of Sweet and Spicy Butternut Squash Soup",
    "Plate of Truffle Steak Dinner",
    "Bowl of Fancy Potato and Leek Soup",
    "Plate of Beef Rendang",
    "Plate of Kimchi Pancakes",
    "Mint-Pear Cured Meat Flatbread",
    "Clove-Spiced Pear and Cured Meat Flatbread",
    "Mint and Veggie Flatbread",
    "Delicious Rice Ball",
    "Eggs Benedict with Mint-Parsley Sauce",
    "Bowl of Fruit Salad with Mint Garnish",
    "Bowl of Seaweed Salad",
];

pub const UTILITY: [&str; 6] = [
    "",
    "Superior Sharpening Stone",
    "Furious Sharpening Stone",
    "Bountiful Sharpening Stone",
    "Bountiful Maintenance Oil",
    "Furious Maintenance Oil",
];

static RELICS: Lazy<Vec<String>> = Lazy::new(|| {
    serde_json::from_str(include_str!("../../data/relics.json")).expect("bundled relics.json")
});

fn at<'a>(table: &[&'a str], idx: u32) -> &'a str {
    table.get(idx as usize).copied().unwrap_or("")
}

pub fn profession(idx: u32) -> &'static str {
    at(&PROFESSIONS, idx)
}

pub fn game_mode(idx: u32) -> GameMode {
    match idx {
        1 => GameMode::Pvp,
        2 => GameMode::Wvw,
        _ => GameMode::Pve,
    }
}

pub fn weapon(idx: u32) -> &'static str {
    at(&WEAPONS, idx)
}

/// Two-handed by wire index: greatsword..staff (11-16) and spear (18).
pub fn is_two_handed_index(idx: u32) -> bool {
    matches!(idx, 11..=16 | 18)
}

pub fn stat(idx: u32) -> &'static str {
    at(&STATS, idx)
}

pub fn relic(idx: u32) -> String {
    match idx {
        0 => String::new(),
        n => RELICS.get(n as usize - 1).cloned().unwrap_or_default(),
    }
}

pub fn food(idx: u32) -> &'static str {
    at(&FOOD, idx)
}

pub fn utility(idx: u32) -> &'static str {
    at(&UTILITY, idx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tables_index_from_the_wire() {
        assert_eq!(profession(0), "Guardian");
        assert_eq!(profession(8), "Revenant");
        assert_eq!(profession(15), "");
        assert_eq!(weapon(11), "greatsword");
        assert!(is_two_handed_index(18) && !is_two_handed_index(17));
        assert_eq!(stat(1), "Berserker's");
        assert_eq!(stat(40), "Commander's");
        assert_eq!(relic(0), "");
        assert_eq!(relic(1), "Relic of Agony");
        assert_eq!(relic(112), "Relic of Watch");
        assert_eq!(game_mode(3), GameMode::Pve);
    }
}
