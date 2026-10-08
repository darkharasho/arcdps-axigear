//! Build code decoder, a line-by-line port of AxiForge's `decodeShareCode`
//! (packages/axicode/src/index.js). Underwater and profession-specific data
//! sit after everything axigear checks, so decoding stops at land infusions.

use std::collections::BTreeMap;

use super::bits::BitReader;
use super::{tables, z85, DecodeError};
use crate::model::{Build, Equipment, SkillBar, SpecLine, TraitSel};
use crate::raw::{OneOrMany, RawEquipment};

const ARMOR: [&str; 6] = ["head", "shoulders", "chest", "hands", "legs", "feet"];

/// Payload of `<AxiForge:Label:payload>`. Z85 contains `<`, `>` and `:`, so the
/// payload is everything after the first colon up to the final character.
pub(crate) fn wrapper_payload(code: &str) -> Option<&str> {
    let inner = code.strip_prefix("<AxiForge:")?.strip_suffix('>')?;
    let colon = inner.find(':')?;
    if colon < 1 {
        return None;
    }
    let payload = &inner[colon + 1..];
    (!payload.is_empty()).then_some(payload)
}

pub fn decode_build_code(code: &str) -> Result<Build, DecodeError> {
    let payload = wrapper_payload(code.trim()).ok_or(DecodeError::InvalidFormat)?;
    decode_payload(payload)
}

fn ids(r: &mut BitReader, n: usize) -> Result<Vec<String>, DecodeError> {
    (0..n).map(|_| r.read(17).map(|v| v.to_string())).collect()
}

pub fn decode_payload(payload: &str) -> Result<Build, DecodeError> {
    let bytes = z85::decode(payload)?;
    let mut r = BitReader::new(&bytes);
    let version = r.read(4)?;
    match version {
        0 => return Err(DecodeError::Corrupt),
        1 | 2 => {}
        _ => return Err(DecodeError::NewerVersion),
    }
    let stat_bits = if version >= 2 { 6 } else { 5 };

    let flags = r.read(8)?;
    let has_oh1 = flags & 2 != 0;
    let has_oh2 = flags & 4 != 0;
    let has_set2 = flags & 8 != 0;
    let per_slot_stats = flags & 32 != 0;
    let per_slot_runes = flags & 64 != 0;
    let per_slot_infusions = flags & 128 != 0;

    let profession = tables::profession(r.read(4)?).to_string();
    let game_mode = tables::game_mode(r.read(2)?);

    let mut specs: [SpecLine; 3] = Default::default();
    for line in specs.iter_mut() {
        line.id = r.read(7)? as u16;
        for major in line.majors.iter_mut() {
            let p = r.read(2)? as u8;
            *major = if p == 0 { TraitSel::None } else { TraitSel::Position(p) };
        }
    }

    let heal = r.read(17)?;
    let utilities = [r.read(17)?, r.read(17)?, r.read(17)?];
    let elite = r.read(17)?;

    let mut raw = RawEquipment::default();
    let mh1 = r.read(5)?;
    raw.weapons.insert("mainhand1".into(), tables::weapon(mh1).into());
    if has_oh1 {
        raw.weapons.insert("offhand1".into(), tables::weapon(r.read(5)?).into());
    }
    let mut mh2 = 0;
    if has_set2 {
        mh2 = r.read(5)?;
        raw.weapons.insert("mainhand2".into(), tables::weapon(mh2).into());
        if has_oh2 {
            raw.weapons.insert("offhand2".into(), tables::weapon(r.read(5)?).into());
        }
    }

    if !per_slot_stats {
        raw.stat_package = tables::stat(r.read(stat_bits)?).into();
    } else {
        for key in [
            "head", "shoulders", "chest", "hands", "legs", "feet",
            "back", "amulet", "ring1", "ring2", "accessory1", "accessory2", "mainhand1",
        ] {
            raw.slots.insert(key.into(), tables::stat(r.read(stat_bits)?).into());
        }
        if has_oh1 {
            raw.slots.insert("offhand1".into(), tables::stat(r.read(stat_bits)?).into());
        }
        if has_set2 {
            raw.slots.insert("mainhand2".into(), tables::stat(r.read(stat_bits)?).into());
            if has_oh2 {
                raw.slots.insert("offhand2".into(), tables::stat(r.read(stat_bits)?).into());
            }
        }
    }

    if !per_slot_runes {
        let id = r.read(17)?.to_string();
        for key in ARMOR {
            raw.runes.insert(key.into(), id.clone());
        }
    } else {
        for key in ARMOR {
            raw.runes.insert(key.into(), r.read(17)?.to_string());
        }
    }

    let n1 = if tables::is_two_handed_index(mh1) { 2 } else { 1 };
    raw.sigils.insert("mainhand1".into(), ids(&mut r, n1)?);
    if has_oh1 {
        raw.sigils.insert("offhand1".into(), ids(&mut r, 1)?);
    }
    if has_set2 {
        let n2 = if tables::is_two_handed_index(mh2) { 2 } else { 1 };
        raw.sigils.insert("mainhand2".into(), ids(&mut r, n2)?);
        if has_oh2 {
            raw.sigils.insert("offhand2".into(), ids(&mut r, 1)?);
        }
    }

    raw.relic = tables::relic(r.read(7)?);
    raw.food = tables::food(r.read(4)?).into();
    raw.utility = tables::utility(r.read(3)?).into();
    let _enrichment = r.read(17)?;

    let one = |s: String| OneOrMany::One(s);
    let many = |v: Vec<String>| OneOrMany::Many(v);
    if !per_slot_infusions {
        let id = r.read(17)?.to_string();
        for key in ARMOR.iter().chain(&["accessory1", "accessory2"]) {
            raw.infusions.insert((*key).into(), one(id.clone()));
        }
        raw.infusions.insert("back".into(), many(vec![id.clone(); 2]));
        raw.infusions.insert("ring1".into(), many(vec![id.clone(); 3]));
        raw.infusions.insert("ring2".into(), many(vec![id.clone(); 3]));
        raw.infusions.insert("mainhand1".into(), many(vec![id.clone(); 2]));
        if has_oh1 {
            raw.infusions.insert("offhand1".into(), many(vec![id.clone()]));
        }
        if has_set2 {
            raw.infusions.insert("mainhand2".into(), many(vec![id.clone(); 2]));
            if has_oh2 {
                raw.infusions.insert("offhand2".into(), many(vec![id.clone()]));
            }
        }
    } else {
        for key in ARMOR {
            raw.infusions.insert(key.into(), one(r.read(17)?.to_string()));
        }
        raw.infusions.insert("back".into(), many(ids(&mut r, 2)?));
        raw.infusions.insert("ring1".into(), many(ids(&mut r, 3)?));
        raw.infusions.insert("ring2".into(), many(ids(&mut r, 3)?));
        raw.infusions.insert("accessory1".into(), one(r.read(17)?.to_string()));
        raw.infusions.insert("accessory2".into(), one(r.read(17)?.to_string()));
        raw.infusions.insert("mainhand1".into(), many(ids(&mut r, 2)?));
        if has_oh1 {
            raw.infusions.insert("offhand1".into(), many(ids(&mut r, 1)?));
        }
        if has_set2 {
            raw.infusions.insert("mainhand2".into(), many(ids(&mut r, 2)?));
            if has_oh2 {
                raw.infusions.insert("offhand2".into(), many(ids(&mut r, 1)?));
            }
        }
    }

    Ok(Build {
        title: None,
        profession,
        game_mode,
        specs,
        skills: SkillBar { heal, utilities, elite },
        skill_names: BTreeMap::new(),
        equipment: Equipment::from_raw(&raw),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{GameMode, GearSlot};
    use crate::testutil::fixture;

    /// AxiForge's own decode of the same code, typed through the same
    /// `Equipment::from_raw`, so any bit-level slip shows up as a diff.
    fn js(name: &str) -> serde_json::Value {
        serde_json::from_str(&fixture(&format!("build-{name}.decoded.json"))).unwrap()
    }

    fn assert_parity(name: &str) {
        let build = decode_build_code(&fixture(&format!("build-{name}.txt"))).unwrap();
        let js = js(name);
        assert_eq!(build.profession, js["profession"].as_str().unwrap());
        assert_eq!(Some(build.game_mode), GameMode::parse(js["gameMode"].as_str().unwrap()));
        for (i, line) in build.specs.iter().enumerate() {
            let jl = &js["specializations"][i];
            assert_eq!(line.id as u64, jl["id"].as_u64().unwrap());
            for t in 0..3 {
                let p = jl["traitChoices"][t].as_u64().unwrap() as u8;
                let want = if p == 0 { TraitSel::None } else { TraitSel::Position(p) };
                assert_eq!(line.majors[t], want, "{name} spec {i} tier {t}");
            }
        }
        assert_eq!(build.skills.heal as u64, js["skills"]["healId"].as_u64().unwrap());
        assert_eq!(build.skills.elite as u64, js["skills"]["eliteId"].as_u64().unwrap());
        let raw: RawEquipment = serde_json::from_value(js["equipment"].clone()).unwrap();
        assert_eq!(build.equipment, Equipment::from_raw(&raw), "{name} equipment");
    }

    #[test]
    fn berserker_matches_axiforge() {
        assert_parity("berserker");
    }

    #[test]
    fn firebrand_matches_axiforge() {
        assert_parity("firebrand");
    }

    #[test]
    fn core_necro_matches_axiforge() {
        assert_parity("necro");
    }

    #[test]
    fn berserker_fields() {
        let b = decode_build_code(&fixture("build-berserker.txt")).unwrap();
        assert_eq!(b.profession, "Warrior");
        assert_eq!(b.specs.iter().map(|s| s.id).collect::<Vec<_>>(), [4, 36, 18]);
        assert_eq!(b.specs[0].majors, [TraitSel::Position(3), TraitSel::Position(3), TraitSel::Position(1)]);
        assert_eq!(b.skills, SkillBar { heal: 14402, utilities: [14404, 14410, 14405], elite: 14355 });
        assert_eq!(b.equipment.weapons.a1.as_deref(), Some("greatsword"));
        assert_eq!(b.equipment.weapons.b1.as_deref(), Some("axe"));
        assert_eq!(b.equipment.sigils.a1, vec![24615, 24868]);
        assert_eq!(b.equipment.sigils.b1, vec![24615]);
        assert_eq!(b.equipment.runes.len(), 6);
        assert_eq!(b.equipment.stats.len(), 14);
        // armor 6 + back 2 + rings 6 + accessories 2 + greatsword 2 + axe 1
        assert_eq!(b.equipment.infusions.len(), 19);
        assert_eq!(b.equipment.food.as_deref(), Some("Bowl of Sweet and Spicy Butternut Squash Soup"));
        assert_eq!(b.equipment.relic.as_deref(), Some("Relic of the Thief"));
    }

    #[test]
    fn firebrand_per_slot_fields() {
        let b = decode_build_code(&fixture("build-firebrand.txt")).unwrap();
        assert_eq!(b.equipment.stats[&GearSlot::WeaponB1], "Harrier's");
        assert_eq!(b.equipment.stats[&GearSlot::WeaponA2], "Minstrel's");
        assert_eq!(b.equipment.runes[&GearSlot::Legs], 24691);
        assert_eq!(b.equipment.sigils.a2, vec![24612]);
        assert_eq!(b.equipment.infusions.iter().filter(|i| **i == 86180).count(), 1);
        assert_eq!(b.equipment.infusions.len(), 20);
    }

    #[test]
    fn necro_has_an_empty_utility_slot_and_no_upgrades() {
        let b = decode_build_code(&fixture("build-necro.txt")).unwrap();
        assert_eq!(b.skills.utilities, [10545, 0, 10685]);
        assert!(b.equipment.runes.is_empty());
        assert!(b.equipment.infusions.is_empty());
        assert_eq!(b.equipment.food, None);
    }

    #[test]
    fn wrapper_keeps_angle_brackets_inside_the_payload() {
        assert_eq!(wrapper_payload("<AxiForge:X:ab>cd>"), Some("ab>cd"));
        assert_eq!(wrapper_payload("<AxiForge::abc>"), None);
        assert_eq!(wrapper_payload("<AxiForge:X:>"), None);
        assert_eq!(wrapper_payload("AxiForge:X:abc"), None);
    }

    #[test]
    fn bad_codes_fail_cleanly() {
        assert_eq!(decode_build_code("hello"), Err(DecodeError::InvalidFormat));
        assert_eq!(decode_build_code("<AxiForge:X:abcd>"), Err(DecodeError::Corrupt));
        assert_eq!(decode_build_code("<AxiForge:X:00000>"), Err(DecodeError::Corrupt)); // version 0
        let full = fixture("build-berserker.txt");
        let cut = format!("{}>", &full[..full.len() - 21]); // drop 4 z85 groups
        assert_eq!(decode_build_code(&cut), Err(DecodeError::Truncated));
    }

    #[test]
    fn newer_version_is_reported() {
        // z85("[bJB*") == [0xF0, 0, 0, 0] → version nibble 15.
        assert_eq!(decode_build_code("<AxiForge:X:[bJB*>"), Err(DecodeError::NewerVersion));
    }
}
