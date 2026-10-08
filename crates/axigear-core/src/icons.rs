//! Icon URLs the build doesn't carry: armor/trinket/weapon art (copied from
//! AxiForge's constants.js / weapons.js) and relic/food/utility by name
//! (scripts/gen-named-icons.py).

use std::collections::HashMap;

use once_cell::sync::Lazy;
use serde::Deserialize;

use crate::model::GearSlot;
use crate::text::norm;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Weight { Light, Medium, Heavy }

pub fn weight_of(profession: &str) -> Option<Weight> {
    Some(match profession.to_ascii_lowercase().as_str() {
        "elementalist" | "mesmer" | "necromancer" => Weight::Light,
        "engineer" | "ranger" | "thief" => Weight::Medium,
        "guardian" | "warrior" | "revenant" => Weight::Heavy,
        _ => return None,
    })
}

// LEGENDARY_ARMOR_ICONS, in GearSlot::ARMOR order (head, shoulders, chest, hands, legs, feet).
const LIGHT: [&str; 6] = [
    "https://render.guildwars2.com/file/06146C9BD029041178F50B5D9ACD0A76E7051408/1634576.png",
    "https://render.guildwars2.com/file/A77403E5F0EB03E46E686B12297A04707AF50278/1634579.png",
    "https://render.guildwars2.com/file/C8FB494379CC98171EFB0F13923CACFD047743B3/1634574.png",
    "https://render.guildwars2.com/file/9703DBC0926F6BB4072032E6B55BE593F6B750CD/1634575.png",
    "https://render.guildwars2.com/file/65A4D3A41592D10EEABD0BC0D611F13A383B0261/1634577.png",
    "https://render.guildwars2.com/file/FD60D4E3986FA46F4FEBB8131B65159195260B19/1634578.png",
];
const MEDIUM: [&str; 6] = [
    "https://render.guildwars2.com/file/49092A1358E528DEC67EFA1C090546ED034642E2/1634588.png",
    "https://render.guildwars2.com/file/CF7609512FC6527D805F2B74F26AF4549FF4E808/1634591.png",
    "https://render.guildwars2.com/file/57360F35D1210D12010F6AE772382450A07D08F6/1634586.png",
    "https://render.guildwars2.com/file/C57E5E5FA69261A2503CBB50080A6C023A155C49/1634587.png",
    "https://render.guildwars2.com/file/EBD907C061747927AE062D1B41BC13D0EAF14AD5/1634589.png",
    "https://render.guildwars2.com/file/BF4C6A48BA02BD6D6AC32F1E9C3F32A50399E336/1634590.png",
];
const HEAVY: [&str; 6] = [
    "https://render.guildwars2.com/file/2695A8E44B7F07EF15A20857790EFCA91513F5F0/1634565.png",
    "https://render.guildwars2.com/file/0F0F4BE73C9316BAA4956A3AA622CB0AE84D9CEA/1634567.png",
    "https://render.guildwars2.com/file/DACF9B1ACBE8687B6B31ABC0CF295301120D7A67/1634563.png",
    "https://render.guildwars2.com/file/A5DD0D661970F02CC26D04B510C7C94259B99520/1634564.png",
    "https://render.guildwars2.com/file/EA9294557C175A43567906721E43962EC4B12D34/1634566.png",
    "https://render.guildwars2.com/file/E895D40AE0D1A500FFFDB955C27A98FF687AA4C1/1634562.png",
];
// EQUIP_TRINKET_SLOTS filledIcon.
const BACK: &str = "https://render.guildwars2.com/file/5EBEA1A467236237FCBACDC09969647956C4A371/1701118.png";
const AMULET: &str = "https://render.guildwars2.com/file/4944FD054FD80D805B0BFFB2DA60363A7DD31FDB/1614376.png";
const RING: &str = "https://render.guildwars2.com/file/EAA61AAF9BEF031104FD063C0A301A520EF5F5E6/1614682.png";
const ACCESSORY: &str = "https://render.guildwars2.com/file/741D3F520D1DFD7BB9A35AD50FC75152D2B3CA6B/1614709.png";

pub fn gear_icon(slot: GearSlot, weight: Option<Weight>) -> Option<&'static str> {
    let armor = |i: usize| weight.map(|w| match w { Weight::Light => LIGHT[i], Weight::Medium => MEDIUM[i], Weight::Heavy => HEAVY[i] });
    match slot {
        GearSlot::Back => Some(BACK),
        GearSlot::Amulet => Some(AMULET),
        GearSlot::Ring1 | GearSlot::Ring2 => Some(RING),
        GearSlot::Accessory1 | GearSlot::Accessory2 => Some(ACCESSORY),
        s if s.is_weapon() => None,
        s => armor(GearSlot::ARMOR.iter().position(|a| *a == s)?),
    }
}

// GW2_WEAPONS from forge-render/src/weapons.js (land weapons).
const WEAPONS: [(&str, &str); 17] = [
    ("axe", "https://wiki.guildwars2.com/images/b/b5/Bandit_Cleaver.png"),
    ("dagger", "https://wiki.guildwars2.com/images/a/ac/Bandit_Shiv.png"),
    ("mace", "https://wiki.guildwars2.com/images/b/b3/Bandit_Mallet.png"),
    ("pistol", "https://wiki.guildwars2.com/images/f/f3/Bandit_Revolver.png"),
    ("sword", "https://wiki.guildwars2.com/images/e/e1/Bandit_Slicer.png"),
    ("scepter", "https://wiki.guildwars2.com/images/9/95/Bandit_Baton.png"),
    ("focus", "https://wiki.guildwars2.com/images/d/da/Bandit_Focus.png"),
    ("shield", "https://wiki.guildwars2.com/images/7/7c/Bandit_Ward.png"),
    ("torch", "https://wiki.guildwars2.com/images/7/7e/Bandit_Torch.png"),
    ("warhorn", "https://wiki.guildwars2.com/images/3/31/Bandit_Bugle.png"),
    ("greatsword", "https://wiki.guildwars2.com/images/0/0b/Bandit_Sunderer.png"),
    ("hammer", "https://wiki.guildwars2.com/images/f/fb/Bandit_Demolisher.png"),
    ("longbow", "https://wiki.guildwars2.com/images/2/2d/Bandit_Longbow.png"),
    ("rifle", "https://wiki.guildwars2.com/images/3/37/Bandit_Musket.png"),
    ("shortbow", "https://wiki.guildwars2.com/images/2/2f/Bandit_Short_Bow.png"),
    ("staff", "https://wiki.guildwars2.com/images/9/98/Bandit_Spire.png"),
    ("spear", "https://wiki.guildwars2.com/images/c/c9/Bandit_Spear.png"),
];

pub fn weapon_icon(weapon: &str) -> Option<&'static str> {
    let key: String = weapon.to_ascii_lowercase().chars().filter(|c| !c.is_whitespace()).collect();
    WEAPONS.iter().find(|(id, _)| *id == key).map(|(_, url)| *url)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NamedKind { Relic, Food, Utility }

#[derive(Debug, Clone, Deserialize)]
pub struct Named {
    pub kind: NamedKind,
    pub name: String,
    pub icon: String,
    #[serde(default)]
    pub buff: String,
}

static NAMED: Lazy<HashMap<(NamedKind, String), Named>> = Lazy::new(|| {
    let rows: Vec<Named> = serde_json::from_str(include_str!("../data/named_icons.json")).expect("bundled named_icons.json");
    rows.into_iter().map(|r| ((r.kind, norm(&r.name)), r)).collect()
});

pub fn named(kind: NamedKind, name: &str) -> Option<&'static Named> {
    NAMED.get(&(kind, norm(name)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::GearSlot;

    #[test]
    fn weights_follow_profession() {
        assert_eq!(weight_of("Guardian"), Some(Weight::Heavy));
        assert_eq!(weight_of("thief"), Some(Weight::Medium));
        assert_eq!(weight_of("Necromancer"), Some(Weight::Light));
        assert_eq!(weight_of("Bogus"), None);
    }

    #[test]
    fn every_armor_and_trinket_slot_has_an_icon() {
        for w in [Weight::Light, Weight::Medium, Weight::Heavy] {
            for s in GearSlot::ARMOR {
                assert!(gear_icon(s, Some(w)).is_some_and(|u| u.starts_with("https://render.guildwars2.com/")), "{s:?} {w:?}");
            }
        }
        for s in [GearSlot::Back, GearSlot::Amulet, GearSlot::Ring1, GearSlot::Ring2, GearSlot::Accessory1, GearSlot::Accessory2] {
            assert!(gear_icon(s, None).is_some(), "{s:?}");
        }
        assert!(gear_icon(GearSlot::Head, None).is_none(), "armor needs a weight");
        assert!(gear_icon(GearSlot::WeaponA1, Some(Weight::Heavy)).is_none());
    }

    #[test]
    fn weapons_resolve_by_axiforge_id() {
        for w in ["axe", "dagger", "mace", "pistol", "sword", "scepter", "focus", "shield", "torch", "warhorn",
                  "greatsword", "hammer", "longbow", "rifle", "shortbow", "staff", "spear"] {
            assert!(weapon_icon(w).is_some_and(|u| u.starts_with("https://wiki.guildwars2.com/")), "{w}");
        }
        assert!(weapon_icon("Greatsword").is_some(), "case-insensitive");
        assert!(weapon_icon("short bow").is_some(), "the API spelling");
        assert!(weapon_icon("banana").is_none());
    }

    #[test]
    fn named_lookups_normalise_and_follow_aliases() {
        let relic = named(NamedKind::Relic, "relic of the thief").expect("relic");
        assert!(relic.icon.starts_with("https://render.guildwars2.com/"));
        assert!(named(NamedKind::Food, "Plate of Truffle Steak Dinner").is_some());
        assert!(named(NamedKind::Utility, "Superior Sharpening Stone").is_some());
        assert!(named(NamedKind::Food, "Superior Sharpening Stone").is_none(), "kinds are separate");
    }
}
