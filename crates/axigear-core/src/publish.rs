//! Published AxiForge files: `site/comps/<id>.enc` and `site/builds/<id>.enc`.
//! v1 file = base64(iv(12) | AES-256-GCM ciphertext | tag(16)) of the JSON; v2 file =
//! `\0AX\x02` | iv | AES-256-GCM(gzip(JSON)) | tag as raw bytes. Key = base64url(32 bytes)
//! (axiforge buildEncryption.js, compPublish.js).

use std::collections::{BTreeMap, HashMap};
use std::io::Read;

use aes_gcm::aead::Aead;
use aes_gcm::{Aes256Gcm, KeyInit, Nonce};
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use base64::Engine;
use serde::Deserialize;

use crate::model::{Build, Comp, Equipment, GameMode, PartyLine, SkillBar, SlotEntry, SpecLine, TraitSel};
use crate::raw::RawEquipment;

pub const SUPPORTED_SCHEMA: u64 = 1;
/// Highest comp `"v"` read: 2 = `members` links instead of embedded `builds`.
const SUPPORTED_COMP_FORMAT: u64 = 2;

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
    members: BTreeMap<String, PubMember>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct PubMember {
    file_id: String,
    key: String,
    owner: String,
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

/// Bytes of JSON a gunzipped v2 payload may expand to (zip-bomb guard).
const MAX_PLAIN: usize = 16 * 1024 * 1024;
/// v2 envelope: `\0 A X <version>` then iv(12) | ciphertext | tag(16).
const MAGIC: [u8; 3] = [0x00, 0x41, 0x58];
const ENVELOPE_VERSION: u8 = 2;

/// Decrypt a published file (v1 base64 text, or v2 binary envelope) to its JSON bytes.
pub fn decrypt(file: &[u8], key: &str) -> Result<Vec<u8>, PublishError> {
    let key = URL_SAFE_NO_PAD
        .decode(key.trim().trim_end_matches('='))
        .map_err(|_| PublishError::BadKey)?;
    if key.len() != 32 {
        return Err(PublishError::BadKey);
    }
    let cipher = Aes256Gcm::new_from_slice(&key).map_err(|_| PublishError::BadKey)?;
    if file.len() >= 4 && file[..3] == MAGIC {
        return match file[3] {
            ENVELOPE_VERSION => decrypt_v2(&cipher, &file[4..]),
            0 | 1 => Err(PublishError::Corrupt),
            newer => Err(PublishError::NewerSchema(newer as u64)),
        };
    }
    let text = std::str::from_utf8(file).map_err(|_| PublishError::Corrupt)?;
    let data = STANDARD.decode(text.trim()).map_err(|_| PublishError::Corrupt)?;
    open(&cipher, &data)
}

fn open(cipher: &Aes256Gcm, iv_and_sealed: &[u8]) -> Result<Vec<u8>, PublishError> {
    if iv_and_sealed.len() < 12 + 16 {
        return Err(PublishError::Corrupt);
    }
    let (iv, sealed) = iv_and_sealed.split_at(12);
    cipher.decrypt(Nonce::from_slice(iv), sealed).map_err(|_| PublishError::Decrypt)
}

fn decrypt_v2(cipher: &Aes256Gcm, body: &[u8]) -> Result<Vec<u8>, PublishError> {
    let packed = open(cipher, body)?;
    let mut plain = Vec::new();
    flate2::read::GzDecoder::new(packed.as_slice())
        .take(MAX_PLAIN as u64 + 1)
        .read_to_end(&mut plain)
        .map_err(|_| PublishError::Corrupt)?;
    if plain.len() > MAX_PLAIN {
        return Err(PublishError::Corrupt);
    }
    Ok(plain)
}

fn json_with_schema(plain: &[u8]) -> Result<serde_json::Value, PublishError> {
    let value: serde_json::Value =
        serde_json::from_slice(plain).map_err(|e| PublishError::Json(e.to_string()))?;
    let version = value.get("schemaVersion").and_then(|v| v.as_u64()).unwrap_or(1);
    if version > SUPPORTED_SCHEMA {
        return Err(PublishError::NewerSchema(version));
    }
    // v2 comps (members instead of embedded builds) carry `"v": 2`.
    let comp_version = value.get("v").and_then(|v| v.as_u64()).unwrap_or(1);
    if comp_version > SUPPORTED_COMP_FORMAT {
        return Err(PublishError::NewerSchema(comp_version));
    }
    Ok(value)
}

/// First-reference order: party lines (tag slots skipped), then categories (as AxiCode does).
fn reference_order(pc: &PubComp, known: impl Fn(&str) -> bool) -> Vec<String> {
    let mut order: Vec<String> = Vec::new();
    let mut add = |id: &str| {
        if known(id) && !order.iter().any(|o| o == id) {
            order.push(id.to_string());
        }
    };
    for line in &pc.party_lines {
        for slot in line.slots.iter().flatten().filter(|s| !s.starts_with("tag:")) {
            add(slot);
        }
    }
    for cat in &pc.categories {
        for id in &cat.build_ids {
            add(id);
        }
    }
    order
}

fn read_comp(plain: &[u8]) -> Result<PubComp, PublishError> {
    let value = json_with_schema(plain)?;
    serde_json::from_value(value).map_err(|e| PublishError::Json(e.to_string()))
}

/// A v2 comp's link to a teammate's published build.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Member {
    pub build_id: String,
    pub file_id: String,
    pub key: String,
    pub owner: String,
}

/// The builds a v2 comp links to, in slot order then the rest by id. Empty for v1 comps.
pub fn comp_members(plain: &[u8]) -> Result<Vec<Member>, PublishError> {
    let pc = read_comp(plain)?;
    let mut ids = reference_order(&pc, |id| pc.members.contains_key(id));
    ids.extend(pc.members.keys().filter(|id| !ids.contains(id)).cloned().collect::<Vec<_>>());
    Ok(ids
        .into_iter()
        .map(|build_id| {
            let m = &pc.members[&build_id];
            Member { build_id, file_id: m.file_id.clone(), key: m.key.clone(), owner: m.owner.clone() }
        })
        .collect())
}

/// Parse a v1 comp (builds embedded).
pub fn parse_comp(plain: &[u8]) -> Result<Comp, PublishError> {
    parse_comp_with(plain, &BTreeMap::new())
}

/// Parse a comp, resolving build ids against `fetched` (a v2 comp's members, already
/// parsed) and the comp's own embedded `builds` (v1). Ids found in neither are dropped.
pub fn parse_comp_with(plain: &[u8], fetched: &BTreeMap<String, Build>) -> Result<Comp, PublishError> {
    let pc = read_comp(plain)?;
    let mut all: BTreeMap<&str, Build> = pc.builds.iter().map(|(id, b)| (id.as_str(), b.to_build())).collect();
    all.extend(fetched.iter().map(|(id, b)| (id.as_str(), b.clone())));

    let order = reference_order(&pc, |id| all.contains_key(id));
    let index: HashMap<&str, usize> = order.iter().enumerate().map(|(i, id)| (id.as_str(), i)).collect();
    let builds = order.iter().map(|id| all[id.as_str()].clone()).collect();

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
    use crate::testutil::{fixture, fixture_bytes, seal_v2};

    fn published_comp() -> Comp {
        let plain = decrypt(fixture("comp-tuesday.enc").as_bytes(), &fixture("fixture.key")).unwrap();
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
        let plain = decrypt(fixture("build-firebrand.enc").as_bytes(), &fixture("fixture.key")).unwrap();
        let b = parse_build(&plain).unwrap();
        assert_eq!(b.title.as_deref(), Some("Quickbrand"));
        assert_eq!(b.equipment.weapons.a1.as_deref(), Some("mace"));
    }

    #[test]
    fn wrong_key_truncated_and_malformed() {
        let other = URL_SAFE_NO_PAD.encode([7u8; 32]);
        assert_eq!(decrypt(fixture("comp-tuesday.enc").as_bytes(), &other), Err(PublishError::Decrypt));
        assert_eq!(decrypt(fixture("comp-tuesday.enc").as_bytes(), "abc"), Err(PublishError::BadKey));
        assert_eq!(decrypt(b"AAAA", &fixture("fixture.key")), Err(PublishError::Corrupt));
        assert_eq!(decrypt(b"not base64!", &fixture("fixture.key")), Err(PublishError::Corrupt));
    }

    #[test]
    fn schema_versions() {
        assert_eq!(parse_comp(br#"{"schemaVersion":2,"name":"x"}"#), Err(PublishError::NewerSchema(2)));
        let legacy = parse_comp(br#"{"name":"Old","partyLines":[{"capacity":5,"slots":["a","tag:missing",null]}],"builds":{"a":{"profession":"Thief"}}}"#).unwrap();
        assert_eq!(legacy.lines[0].slots, vec![SlotEntry::Build(0)]);
        assert!(matches!(parse_comp(b"not json"), Err(PublishError::Json(_))));
    }

    fn json(bytes: &[u8]) -> serde_json::Value {
        serde_json::from_slice(bytes).unwrap()
    }

    fn v2_member_builds() -> BTreeMap<String, Build> {
        let members: BTreeMap<String, serde_json::Value> =
            serde_json::from_slice(&fixture_bytes("comp-tuesday.v2.members.json")).unwrap();
        let files = [("firebrand", "member-firebrand.enc"), ("berserker", "member-berserker.enc.v2"), ("necro", "member-necro.enc.v2")];
        files
            .iter()
            .map(|(id, file)| {
                let plain = decrypt(&fixture_bytes(file), members[*id]["key"].as_str().unwrap()).unwrap();
                (id.to_string(), parse_build(&plain).unwrap())
            })
            .collect()
    }

    #[test]
    fn v2_build_envelope_decrypts_to_the_v1_plaintext() {
        let key = fixture("fixture.key");
        let v1 = decrypt(fixture("build-firebrand.enc").as_bytes(), &key).unwrap();
        let v2 = decrypt(&fixture_bytes("build-firebrand.enc.v2"), &key).unwrap();
        assert_eq!(json(&v1), json(&v2));
        assert_eq!(parse_build(&v2).unwrap().title.as_deref(), Some("Quickbrand"));
    }

    #[test]
    fn envelope_version_handling() {
        let key = fixture("fixture.key");
        let good = fixture_bytes("build-firebrand.enc.v2");
        for (version, want) in [(3u8, PublishError::NewerSchema(3)), (255, PublishError::NewerSchema(255)), (1, PublishError::Corrupt), (0, PublishError::Corrupt)] {
            let mut f = good.clone();
            f[3] = version;
            assert_eq!(decrypt(&f, &key), Err(want), "version {version}");
        }
        assert_eq!(decrypt(&good[..4], &key), Err(PublishError::Corrupt));
        assert_eq!(decrypt(&good[..20], &key), Err(PublishError::Corrupt));
        assert_eq!(decrypt(&good[..good.len() - 1], &key), Err(PublishError::Decrypt));
        assert_eq!(decrypt(b"\0AX", &key), Err(PublishError::Corrupt));
        assert_eq!(decrypt(b"\0A", &key), Err(PublishError::Corrupt));
        let mut flipped = good.clone();
        flipped[20] ^= 1;
        assert_eq!(decrypt(&flipped, &key), Err(PublishError::Decrypt));
    }

    #[test]
    fn gunzip_output_is_bounded() {
        let key_text = fixture("fixture.key");
        let seal = |plain: &[u8]| seal_v2(plain, &key_text);
        let ok = vec![b' '; MAX_PLAIN];
        assert_eq!(decrypt(&seal(&ok), &key_text).unwrap().len(), MAX_PLAIN);
        let bomb = vec![b' '; MAX_PLAIN + 1];
        assert_eq!(decrypt(&seal(&bomb), &key_text), Err(PublishError::Corrupt));
    }

    #[test]
    fn v2_comp_lists_members_in_slot_order() {
        let plain = decrypt(&fixture_bytes("comp-tuesday.v2.enc.v2"), &fixture("fixture.key")).unwrap();
        let members = comp_members(&plain).unwrap();
        let ids: Vec<&str> = members.iter().map(|m| m.build_id.as_str()).collect();
        assert_eq!(ids, ["firebrand", "berserker", "necro"]);
        assert_eq!(members[0].file_id, "aaaa0001");
        assert_eq!(members[0].owner, "teammate");
        assert!(!members[0].key.is_empty());
    }

    #[test]
    fn unreferenced_members_follow_sorted_and_v1_has_none() {
        let json = br#"{"v":2,"partyLines":[{"slots":["z",null,"tag:t"]}],"categories":[{"id":"t","name":"T","buildIds":["m","z"]}],
            "members":{"a":{"fileId":"f1","key":"k","owner":"o"},"m":{"fileId":"f2","key":"k","owner":"o"},"z":{"fileId":"f3","key":"k","owner":"o"},"b":{"fileId":"f4","key":"k","owner":"o"}}}"#;
        let ids: Vec<String> = comp_members(json).unwrap().into_iter().map(|m| m.build_id).collect();
        assert_eq!(ids, ["z", "m", "a", "b"]);
        let v1 = decrypt(fixture("comp-tuesday.enc").as_bytes(), &fixture("fixture.key")).unwrap();
        assert!(comp_members(&v1).unwrap().is_empty());
    }

    #[test]
    fn v2_comp_with_member_builds_equals_the_v1_comp() {
        let plain = decrypt(&fixture_bytes("comp-tuesday.v2.enc.v2"), &fixture("fixture.key")).unwrap();
        assert_eq!(parse_comp_with(&plain, &v2_member_builds()).unwrap(), published_comp());
        // Without member builds the slots have nothing to point at.
        let bare = parse_comp_with(&plain, &BTreeMap::new()).unwrap();
        assert_eq!(bare.name, "Tuesday Zerg");
        assert!(bare.builds.is_empty());
    }

    #[test]
    fn v_field_newer_than_two_is_rejected() {
        assert_eq!(parse_comp(br#"{"v":3,"name":"x"}"#), Err(PublishError::NewerSchema(3)));
        assert_eq!(comp_members(br#"{"v":3}"#), Err(PublishError::NewerSchema(3)));
        assert!(parse_comp(br#"{"v":2,"name":"x"}"#).is_ok());
    }
}
