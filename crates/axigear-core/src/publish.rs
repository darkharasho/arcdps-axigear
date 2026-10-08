//! Published AxiForge files: `site/comps/<id>.enc` and `site/builds/<id>.enc`.
//! File = base64(iv(12) | AES-256-GCM ciphertext | tag(16)), key = base64url(32 bytes),
//! plaintext = JSON (axiforge buildEncryption.js, compPublish.js).

use std::collections::{BTreeMap, HashMap};

use aes_gcm::aead::Aead;
use aes_gcm::{Aes256Gcm, KeyInit, Nonce};
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use base64::Engine;
use serde::Deserialize;

use crate::model::{Build, Comp, Equipment, GameMode, PartyLine, SkillBar, SlotEntry, SpecLine, TraitSel};
use crate::raw::RawEquipment;

pub const SUPPORTED_SCHEMA: u64 = 1;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PublishError {
    #[error("the key in that link is malformed")]
    BadKey,
    #[error("the published file is corrupted")]
    Corrupt,
    #[error("Couldn't decrypt - check the link.")]
    Decrypt,
    #[error("Comp made with a newer AxiForge (schema {0}) - update axigear.")]
    NewerSchema(u64),
    #[error("unexpected comp format: {0}")]
    Json(String),
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct PubComp {
    name: String,
    game_mode: Option<String>,
    party_lines: Vec<PubLine>,
    categories: Vec<PubCat>,
    builds: BTreeMap<String, PubBuild>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct PubLine {
    capacity: Option<f64>,
    slots: Vec<Option<String>>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct PubCat {
    id: String,
    name: String,
    build_ids: Vec<String>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct PubBuild {
    title: String,
    profession: String,
    game_mode: String,
    specializations: Vec<PubSpec>,
    skills: PubSkills,
    equipment: RawEquipment,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct PubSpec {
    id: u16,
    elite: bool,
    major_choices: BTreeMap<String, Option<u32>>,
    trait_choices: Option<Vec<u8>>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct PubSkills {
    heal: Option<PubSkill>,
    utility: Vec<Option<PubSkill>>,
    elite: Option<PubSkill>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct PubSkill {
    id: u32,
    name: String,
}

pub fn decrypt(file: &str, key: &str) -> Result<Vec<u8>, PublishError> {
    let key = URL_SAFE_NO_PAD
        .decode(key.trim().trim_end_matches('='))
        .map_err(|_| PublishError::BadKey)?;
    if key.len() != 32 {
        return Err(PublishError::BadKey);
    }
    let data = STANDARD.decode(file.trim()).map_err(|_| PublishError::Corrupt)?;
    if data.len() < 12 + 16 {
        return Err(PublishError::Corrupt);
    }
    let cipher = Aes256Gcm::new_from_slice(&key).map_err(|_| PublishError::BadKey)?;
    let (iv, sealed) = data.split_at(12);
    cipher.decrypt(Nonce::from_slice(iv), sealed).map_err(|_| PublishError::Decrypt)
}

fn json_with_schema(plain: &[u8]) -> Result<serde_json::Value, PublishError> {
    let value: serde_json::Value =
        serde_json::from_slice(plain).map_err(|e| PublishError::Json(e.to_string()))?;
    let version = value.get("schemaVersion").and_then(|v| v.as_u64()).unwrap_or(1);
    if version > SUPPORTED_SCHEMA {
        return Err(PublishError::NewerSchema(version));
    }
    Ok(value)
}

fn add_build(order: &mut Vec<String>, builds: &BTreeMap<String, PubBuild>, id: &str) {
    if builds.contains_key(id) && !order.iter().any(|o| o == id) {
        order.push(id.to_string());
    }
}

pub fn parse_comp(plain: &[u8]) -> Result<Comp, PublishError> {
    let value = json_with_schema(plain)?;
    let pc: PubComp = serde_json::from_value(value).map_err(|e| PublishError::Json(e.to_string()))?;

    // Builds in order of first reference: party lines, then categories (as AxiCode does).
    let mut order = Vec::new();
    for line in &pc.party_lines {
        for slot in line.slots.iter().flatten().filter(|s| !s.starts_with("tag:")) {
            add_build(&mut order, &pc.builds, slot);
        }
    }
    for cat in &pc.categories {
        for id in &cat.build_ids {
            add_build(&mut order, &pc.builds, id);
        }
    }
    let index: HashMap<&str, usize> = order.iter().enumerate().map(|(i, id)| (id.as_str(), i)).collect();
    let builds = order.iter().map(|id| pc.builds[id].to_build()).collect();

    let lines = pc
        .party_lines
        .iter()
        .map(|line| PartyLine {
            capacity: line.capacity.map(|c| c as i64).unwrap_or(5).clamp(1, 50) as u8,
            slots: line
                .slots
                .iter()
                .flatten()
                .filter_map(|slot| match slot.strip_prefix("tag:") {
                    Some(cat_id) => {
                        let cat = pc.categories.iter().find(|c| c.id == cat_id)?;
                        Some(SlotEntry::Tag {
                            name: cat.name.chars().take(60).collect(),
                            builds: cat.build_ids.iter().filter_map(|b| index.get(b.as_str()).copied()).collect(),
                        })
                    }
                    None => index.get(slot.as_str()).map(|i| SlotEntry::Build(*i)),
                })
                .collect(),
        })
        .collect();

    let name = Some(pc.name.trim().to_string()).filter(|n| !n.is_empty()).unwrap_or_else(|| "Untitled Comp".into());
    let game_mode = match pc.game_mode.as_deref() {
        Some("pve") => Some(GameMode::Pve),
        Some("wvw") => Some(GameMode::Wvw),
        _ => None,
    };
    Ok(Comp { name, game_mode, builds, lines })
}

pub fn parse_build(plain: &[u8]) -> Result<Build, PublishError> {
    let value = json_with_schema(plain)?;
    let pb: PubBuild = serde_json::from_value(value).map_err(|e| PublishError::Json(e.to_string()))?;
    Ok(pb.to_build())
}

impl PubBuild {
    fn to_build(&self) -> Build {
        let mut specs: [SpecLine; 3] = Default::default();
        for (line, ps) in specs.iter_mut().zip(&self.specializations) {
            line.id = ps.id;
            line.elite = Some(ps.elite);
            for t in 0..3 {
                let id = ps.major_choices.get(&(t + 1).to_string()).copied().flatten().unwrap_or(0);
                let pos = ps.trait_choices.as_ref().and_then(|v| v.get(t)).copied().unwrap_or(0);
                line.majors[t] = match (id, pos) {
                    (0, 0) => TraitSel::None,
                    (0, p) => TraitSel::Position(p),
                    (id, _) => TraitSel::Id(id),
                };
            }
        }
        let skill = |s: &Option<PubSkill>| s.as_ref().map_or(0, |s| s.id);
        let utility = |i: usize| self.skills.utility.get(i).map_or(0, skill);
        let mut skill_names = BTreeMap::new();
        for s in [&self.skills.heal, &self.skills.elite].into_iter().chain(self.skills.utility.iter()).flatten() {
            if s.id != 0 && !s.name.is_empty() {
                skill_names.insert(s.id, s.name.clone());
            }
        }
        Build {
            title: Some(self.title.trim().to_string()).filter(|t| !t.is_empty()),
            profession: self.profession.clone(),
            game_mode: GameMode::parse(&self.game_mode).unwrap_or(GameMode::Pve),
            specs,
            skills: SkillBar {
                heal: skill(&self.skills.heal),
                utilities: [utility(0), utility(1), utility(2)],
                elite: skill(&self.skills.elite),
            },
            skill_names,
            equipment: Equipment::from_raw(&self.equipment),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::axicode::decode_comp_code;
    use crate::testutil::fixture;

    fn published_comp() -> Comp {
        let plain = decrypt(&fixture("comp-tuesday.enc"), &fixture("fixture.key")).unwrap();
        parse_comp(&plain).unwrap()
    }

    #[test]
    fn decrypts_and_parses_the_tuesday_comp() {
        let comp = published_comp();
        assert_eq!(comp.name, "Tuesday Zerg");
        assert_eq!(comp.game_mode, Some(GameMode::Wvw));
        let profs: Vec<&str> = comp.builds.iter().map(|b| b.profession.as_str()).collect();
        assert_eq!(profs, ["Guardian", "Warrior", "Necromancer"]);
        assert_eq!(comp.builds[0].title.as_deref(), Some("Quickbrand"));
    }

    #[test]
    fn same_comp_as_the_axicode_version() {
        let published = published_comp();
        let code = decode_comp_code(&fixture("comp-tuesday.txt")).unwrap();
        assert_eq!(published.lines, code.lines);
        for (p, c) in published.builds.iter().zip(&code.builds) {
            assert_eq!(p.profession, c.profession);
            assert_eq!(p.skills, c.skills);
            assert_eq!(p.equipment, c.equipment, "{}", p.profession);
            assert_eq!(p.specs.clone().map(|s| s.id), c.specs.clone().map(|s| s.id));
        }
    }

    #[test]
    fn published_traits_are_ids_and_skills_have_names() {
        let fb = &published_comp().builds[0];
        // Firebrand (62) positions [3, 1, 1] → trait IDs from the bundled tiers.
        assert_eq!(fb.specs[2].majors, [TraitSel::Id(2086), TraitSel::Id(2063), TraitSel::Id(2105)]);
        assert_eq!(fb.specs[2].elite, Some(true));
        assert_eq!(fb.skill_names.get(&41714).map(String::as_str), Some("skill-41714"));
    }

    #[test]
    fn zero_major_choice_falls_back_to_trait_position() {
        let json = br#"{"profession":"Guardian","specializations":[{"id":42,"majorChoices":{"1":0,"2":null,"3":625},"traitChoices":[2,0,1]}]}"#;
        let b = parse_build(json).unwrap();
        assert_eq!(b.specs[0].majors, [TraitSel::Position(2), TraitSel::None, TraitSel::Id(625)]);
    }

    #[test]
    fn build_link_payload() {
        let plain = decrypt(&fixture("build-firebrand.enc"), &fixture("fixture.key")).unwrap();
        let b = parse_build(&plain).unwrap();
        assert_eq!(b.title.as_deref(), Some("Quickbrand"));
        assert_eq!(b.equipment.weapons.a1.as_deref(), Some("mace"));
    }

    #[test]
    fn wrong_key_truncated_and_malformed() {
        let other = URL_SAFE_NO_PAD.encode([7u8; 32]);
        assert_eq!(decrypt(&fixture("comp-tuesday.enc"), &other), Err(PublishError::Decrypt));
        assert_eq!(decrypt(&fixture("comp-tuesday.enc"), "abc"), Err(PublishError::BadKey));
        assert_eq!(decrypt("AAAA", &fixture("fixture.key")), Err(PublishError::Corrupt));
        assert_eq!(decrypt("not base64!", &fixture("fixture.key")), Err(PublishError::Corrupt));
    }

    #[test]
    fn schema_versions() {
        assert_eq!(parse_comp(br#"{"schemaVersion":2,"name":"x"}"#), Err(PublishError::NewerSchema(2)));
        let legacy = parse_comp(br#"{"name":"Old","partyLines":[{"capacity":5,"slots":["a","tag:missing",null]}],"builds":{"a":{"profession":"Thief"}}}"#).unwrap();
        assert_eq!(legacy.lines[0].slots, vec![SlotEntry::Build(0)]);
        assert!(matches!(parse_comp(b"not json"), Err(PublishError::Json(_))));
    }
}
