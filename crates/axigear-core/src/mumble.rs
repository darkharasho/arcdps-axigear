//! MumbleLink: GW2's shared-memory identity JSON and context struct.
//! The plugin copies the bytes out; parsing lives here so it is testable.
//! Context layout (wiki API:MumbleLink): mapId @28, mapType @32, uiState @48.

use serde::Deserialize;

use crate::model::GameMode;

#[derive(Deserialize)]
struct JsonIdentity {
    name: String,
    profession: u8,
    #[serde(default)]
    spec: u16,
    #[serde(default)]
    map_id: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MumbleSample {
    pub ui_tick: u32,
    pub identity: String,
    pub context: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    pub name: String,
    /// MumbleLink profession: 1 Guardian … 9 Revenant.
    pub profession: u8,
    /// Third specialization line (the elite slot), 0 if none.
    pub spec: u16,
    pub map_id: u32,
    pub map_type: u32,
    pub in_combat: bool,
}

const PROFESSIONS: [&str; 9] = [
    "Guardian", "Warrior", "Engineer", "Ranger", "Thief",
    "Elementalist", "Mesmer", "Necromancer", "Revenant",
];
const UI_IN_COMBAT: u32 = 0x40;

pub fn profession_name(id: u8) -> Option<&'static str> {
    PROFESSIONS.get((id as usize).checked_sub(1)?).copied()
}

/// Shown when MumbleLink reports a profession this build doesn't know (a new one).
pub fn unknown_profession(id: u8) -> String {
    format!("unknown profession id {id} - update axigear")
}

pub fn profession_id(name: &str) -> u8 {
    PROFESSIONS.iter().position(|p| p.eq_ignore_ascii_case(name)).map_or(0, |i| i as u8 + 1)
}

pub fn game_mode_for_map_type(map_type: u32) -> GameMode {
    match map_type {
        2 | 3 | 6 | 8 => GameMode::Pvp,
        9..=15 | 18 => GameMode::Wvw,
        _ => GameMode::Pve,
    }
}

/// Shared-memory name. Multi-client setups (axiam) start each client with
/// `-mumble <name>` so they don't share one link; default "MumbleLink".
pub fn link_name(args: impl IntoIterator<Item = String>) -> String {
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        if arg.eq_ignore_ascii_case("-mumble") {
            if let Some(name) = args.next().filter(|n| !n.is_empty() && !n.starts_with('-')) {
                return name;
            }
        }
    }
    "MumbleLink".into()
}

pub fn parse(sample: &MumbleSample) -> Option<Identity> {
    if sample.ui_tick == 0 {
        return None;
    }
    let json: JsonIdentity = serde_json::from_str(sample.identity.trim_end_matches('\0')).ok()?;
    let field = |offset: usize| {
        sample.context.get(offset..offset + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    };
    Some(Identity {
        name: json.name,
        profession: json.profession,
        spec: json.spec,
        map_id: field(28).filter(|v| *v != 0).unwrap_or(json.map_id),
        map_type: field(32).unwrap_or(0),
        in_combat: field(48).is_some_and(|ui| ui & UI_IN_COMBAT != 0),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn link_name_follows_the_mumble_argument() {
        let args = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(link_name(args(&["Gw2-64.exe", "-autologin", "-mumble", "GW2Alt2"])), "GW2Alt2");
        assert_eq!(link_name(args(&["Gw2-64.exe", "-autologin"])), "MumbleLink");
        assert_eq!(link_name(args(&["Gw2-64.exe", "-mumble"])), "MumbleLink");
        assert_eq!(link_name(args(&["Gw2-64.exe", "-mumble", "-shareArchive"])), "MumbleLink");
    }

    fn context(map_id: u32, map_type: u32, ui_state: u32) -> Vec<u8> {
        let mut c = vec![0u8; 88];
        c[28..32].copy_from_slice(&map_id.to_le_bytes());
        c[32..36].copy_from_slice(&map_type.to_le_bytes());
        c[48..52].copy_from_slice(&ui_state.to_le_bytes());
        c
    }

    fn sample(json: &str, ctx: Vec<u8>) -> MumbleSample {
        MumbleSample { ui_tick: 7, identity: json.into(), context: ctx }
    }

    #[test]
    fn parses_identity_and_context() {
        let s = sample(
            r#"{"name":"Zoë Ström","profession":1,"spec":62,"race":2,"map_id":38,"world_id":1,"team_color_id":9,"commander":false,"fov":1.2,"uisz":1}"#,
            context(1099, 9, UI_IN_COMBAT | 8),
        );
        let id = parse(&s).unwrap();
        assert_eq!(id.name, "Zoë Ström");
        assert_eq!((id.profession, id.spec, id.map_id, id.map_type), (1, 62, 1099, 9));
        assert!(id.in_combat);
    }

    #[test]
    fn falls_back_to_identity_map_and_tolerates_short_context() {
        let id = parse(&sample(r#"{"name":"A","profession":8,"map_id":38}"#, vec![])).unwrap();
        assert_eq!((id.spec, id.map_id, id.map_type, id.in_combat), (0, 38, 0, false));
    }

    #[test]
    fn nothing_before_the_game_writes_or_on_bad_json() {
        assert_eq!(parse(&MumbleSample { ui_tick: 0, ..sample(r#"{"name":"A","profession":1}"#, vec![]) }), None);
        assert_eq!(parse(&sample("{not json", vec![])), None);
        assert_eq!(parse(&sample("", vec![])), None);
    }

    #[test]
    fn professions_and_map_types() {
        assert_eq!(profession_name(1), Some("Guardian"));
        assert_eq!(profession_name(9), Some("Revenant"));
        assert_eq!(profession_name(0), None);
        assert_eq!(profession_id("necromancer"), 8);
        assert_eq!(profession_id("Bard"), 0);
        assert_eq!(game_mode_for_map_type(9), GameMode::Wvw);
        assert_eq!(game_mode_for_map_type(18), GameMode::Wvw);
        assert_eq!(game_mode_for_map_type(2), GameMode::Pvp);
        assert_eq!(game_mode_for_map_type(5), GameMode::Pve);
    }
}
