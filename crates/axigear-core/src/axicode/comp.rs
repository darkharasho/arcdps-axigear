//! Comp code decoder: `<AxiForge:Comp:base64url(zlib(json))>`, a port of
//! AxiForge's `decodeCompCode` (packages/axicode/src/compCodec.js).
//! Slot ints: >= 0 build index, -1 empty, <= -2 tag slot for category (-idx - 2).

use std::io::Read;

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use serde::Deserialize;

use super::{build::decode_payload, DecodeError};
use crate::model::{Build, Comp, GameMode, PartyLine, SlotEntry};

const PREFIX: &str = "<AxiForge:Comp:";
const MAX_JSON: u64 = 1 << 20;

#[derive(Deserialize)]
struct Schema {
    v: u32,
    #[serde(default)]
    n: Option<String>,
    #[serde(default)]
    g: Option<String>,
    #[serde(default)]
    b: Vec<String>,
    #[serde(default)]
    p: Vec<LineSchema>,
    #[serde(default)]
    cat: Vec<CatSchema>,
}

#[derive(Deserialize)]
struct LineSchema {
    #[serde(default)]
    c: Option<f64>,
    #[serde(default)]
    s: Vec<i64>,
}

#[derive(Deserialize)]
struct CatSchema {
    #[serde(default)]
    i: Option<String>,
    #[serde(default)]
    n: Option<String>,
    #[serde(default)]
    b: Vec<i64>,
}

pub fn is_comp_code(s: &str) -> bool {
    s.starts_with(PREFIX)
}

pub fn decode_comp_code(code: &str) -> Result<Comp, DecodeError> {
    let b64 = code
        .trim()
        .strip_prefix(PREFIX)
        .and_then(|s| s.strip_suffix('>'))
        .filter(|s| !s.is_empty())
        .ok_or(DecodeError::InvalidFormat)?;
    let compressed = URL_SAFE_NO_PAD
        .decode(b64.trim_end_matches('='))
        .map_err(|_| DecodeError::Corrupt)?;
    let mut json = Vec::new();
    flate2::read::ZlibDecoder::new(&compressed[..])
        .take(MAX_JSON + 1)
        .read_to_end(&mut json)
        .map_err(|_| DecodeError::Corrupt)?;
    if json.len() as u64 > MAX_JSON {
        return Err(DecodeError::Corrupt);
    }
    let schema: Schema = serde_json::from_slice(&json).map_err(|_| DecodeError::Corrupt)?;
    if schema.v != 1 {
        return Err(DecodeError::NewerVersion);
    }

    // Decode every build, then compact: old index -> new index (None if it failed).
    let decoded: Vec<Option<Build>> = schema.b.iter().map(|p| decode_payload(p).ok()).collect();
    let mut remap = Vec::with_capacity(decoded.len());
    let mut builds = Vec::new();
    for b in decoded {
        match b {
            Some(b) => {
                remap.push(Some(builds.len()));
                builds.push(b);
            }
            None => remap.push(None),
        }
    }
    let mapped = |idx: i64| -> Option<usize> {
        usize::try_from(idx).ok().and_then(|i| remap.get(i).copied().flatten())
    };

    let lines = schema
        .p
        .iter()
        .map(|line| {
            let capacity = line.c.map(|c| c as i64).unwrap_or(5).clamp(1, 50) as u8;
            let slots = line
                .s
                .iter()
                .filter_map(|&idx| {
                    if idx <= -2 {
                        let cat = schema.cat.get((-idx - 2) as usize)?;
                        cat.i.as_ref()?;
                        Some(SlotEntry::Tag {
                            name: cat.n.clone().unwrap_or_default().chars().take(60).collect(),
                            builds: cat.b.iter().filter_map(|&b| mapped(b)).collect(),
                        })
                    } else {
                        mapped(idx).map(SlotEntry::Build)
                    }
                })
                .collect();
            PartyLine { capacity, slots }
        })
        .collect();

    let name = schema
        .n
        .map(|n| n.chars().take(140).collect::<String>())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| "Untitled Comp".into());
    let game_mode = match schema.g.as_deref() {
        Some("pve") => Some(GameMode::Pve),
        Some("wvw") => Some(GameMode::Wvw),
        _ => None,
    };
    Ok(Comp { name, game_mode, builds, lines })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::fixture;
    use flate2::{write::ZlibEncoder, Compression};
    use std::io::Write;

    fn encode(json: &str) -> String {
        let mut z = ZlibEncoder::new(Vec::new(), Compression::default());
        z.write_all(json.as_bytes()).unwrap();
        format!("{PREFIX}{}>", URL_SAFE_NO_PAD.encode(z.finish().unwrap()))
    }

    #[test]
    fn decodes_the_tuesday_comp() {
        let comp = decode_comp_code(&fixture("comp-tuesday.txt")).unwrap();
        assert_eq!(comp.name, "Tuesday Zerg");
        assert_eq!(comp.game_mode, Some(GameMode::Wvw));
        let profs: Vec<&str> = comp.builds.iter().map(|b| b.profession.as_str()).collect();
        assert_eq!(profs, ["Guardian", "Warrior", "Necromancer"]);
        assert_eq!(comp.lines.len(), 2);
        assert_eq!(comp.lines[0].capacity, 5);
        assert_eq!(
            comp.lines[0].slots,
            vec![
                SlotEntry::Build(0),
                SlotEntry::Build(1),
                SlotEntry::Tag { name: "DPS".into(), builds: vec![1, 2] },
            ]
        );
        assert_eq!(comp.lines[1].slots, vec![SlotEntry::Build(0), SlotEntry::Build(2)]);
    }

    #[test]
    fn matches_axiforge_decode_of_the_same_code() {
        let comp = decode_comp_code(&fixture("comp-tuesday.txt")).unwrap();
        let js: serde_json::Value = serde_json::from_str(&fixture("comp-tuesday.decoded.json")).unwrap();
        assert_eq!(comp.builds.len(), js["builds"].as_array().unwrap().len());
        assert_eq!(js["failedBuildCount"], 0);
    }

    #[test]
    fn bad_builds_are_dropped_and_their_slots_skipped() {
        let good = fixture("build-necro.txt");
        let payload = super::super::build::wrapper_payload(&good).unwrap().to_string();
        let json = format!(r#"{{"v":1,"n":"x","g":null,"b":["!!!!!","{payload}"],"p":[{{"c":5,"s":[0,1,-1,7]}}]}}"#);
        let comp = decode_comp_code(&encode(&json)).unwrap();
        assert_eq!(comp.builds.len(), 1);
        assert_eq!(comp.lines[0].slots, vec![SlotEntry::Build(0)]);
        assert_eq!(comp.game_mode, None);
    }

    #[test]
    fn rejects_newer_schema_and_garbage() {
        assert_eq!(decode_comp_code(&encode(r#"{"v":2}"#)), Err(DecodeError::NewerVersion));
        assert_eq!(decode_comp_code("<AxiForge:Comp:@@@@>"), Err(DecodeError::Corrupt));
        assert_eq!(decode_comp_code("<AxiForge:Comp:>"), Err(DecodeError::InvalidFormat));
        assert_eq!(decode_comp_code("<AxiForge:Build:abc>"), Err(DecodeError::InvalidFormat));
    }

    #[test]
    fn capacity_is_clamped_and_name_defaults() {
        let comp = decode_comp_code(&encode(r#"{"v":1,"b":[],"p":[{"c":99,"s":[]},{"s":[]}]}"#)).unwrap();
        assert_eq!(comp.name, "Untitled Comp");
        assert_eq!(comp.lines[0].capacity, 50);
        assert_eq!(comp.lines[1].capacity, 5);
    }
}
