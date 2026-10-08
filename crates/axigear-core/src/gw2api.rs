//! GW2 API: the active build and equipment tabs, and token info.

use std::time::Instant;

use serde_json::Value;
use url::Url;

use crate::http::Http;
use crate::model::GearSlot;

pub const API_ROOT: &str = "https://api.guildwars2.com";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiItem {
    pub id: u32,
    /// API slot name: "Helm", "WeaponA1", "Relic", …
    pub slot: String,
    pub upgrades: Vec<u32>,
    pub infusions: Vec<u32>,
    /// Selected stats for selectable-stat items; otherwise the item's own (see GameDb).
    pub stats_id: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiSpec {
    pub id: u16,
    pub traits: [u32; 3],
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ApiSkills {
    pub heal: u32,
    pub utilities: [u32; 3],
    pub elite: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiBuild {
    pub profession: String,
    pub specs: Vec<ApiSpec>,
    pub skills: ApiSkills,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ApiSnapshot {
    pub fetched_at: Instant,
    pub character: String,
    pub build: ApiBuild,
    pub equipment: Vec<ApiItem>,
}

impl ApiSnapshot {
    pub fn item(&self, slot: GearSlot) -> Option<&ApiItem> {
        self.equipment.iter().find(|i| GearSlot::from_api_slot(&i.slot) == Some(slot))
    }

    pub fn relic(&self) -> Option<&ApiItem> {
        self.equipment.iter().find(|i| i.slot == "Relic")
    }

    pub fn spec3(&self) -> u16 {
        self.build.specs.get(2).map_or(0, |s| s.id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenInfo {
    pub name: String,
    pub permissions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ApiError {
    #[error("needs API key (characters, builds)")]
    NoKey,
    #[error("API key rejected: {0}")]
    Unauthorized(String),
    #[error("character not on this API key")]
    CharacterNotFound,
    #[error("API rate limited")]
    RateLimited,
    #[error("API error (HTTP {0})")]
    Server(u16),
    #[error("API unreachable: {0}")]
    Network(String),
    #[error("unexpected API response: {0}")]
    Parse(String),
}

impl ApiError {
    /// Transient: retry with backoff rather than at the normal interval.
    pub fn backoff(&self) -> bool {
        matches!(self, ApiError::RateLimited | ApiError::Server(_) | ApiError::Network(_))
    }
}

/// `/v2/characters/<name>/<tail…>` with the name percent-encoded as a path segment.
pub fn character_url(name: &str, tail: &[&str]) -> String {
    let mut url = Url::parse(API_ROOT).expect("API_ROOT is a valid URL");
    url.path_segments_mut()
        .expect("https URL has path segments")
        .pop_if_empty()
        .extend(["v2", "characters", name])
        .extend(tail);
    url.to_string()
}

fn error_text(body: &str) -> String {
    serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| v.get("text").and_then(Value::as_str).map(String::from))
        .unwrap_or_else(|| "invalid key".into())
}

fn get_json(http: &dyn Http, url: &str, key: &str) -> Result<Value, ApiError> {
    let auth = format!("Bearer {}", key.trim());
    let resp = http.get(url, &[("Authorization", &auth)]).map_err(ApiError::Network)?;
    match resp.status {
        200 | 206 => serde_json::from_str(&resp.body).map_err(|e| ApiError::Parse(e.to_string())),
        401 | 403 => Err(ApiError::Unauthorized(error_text(&resp.body))),
        404 => Err(ApiError::CharacterNotFound),
        429 => Err(ApiError::RateLimited),
        s => Err(ApiError::Server(s)),
    }
}

fn num(v: &Value) -> u32 {
    v.as_u64().unwrap_or(0) as u32
}

fn ids(v: &Value) -> Vec<u32> {
    v.as_array().map(|a| a.iter().filter_map(Value::as_u64).map(|x| x as u32).collect()).unwrap_or_default()
}

fn parse_build(tab: &Value) -> Result<ApiBuild, ApiError> {
    let b = tab.get("build").ok_or_else(|| ApiError::Parse("buildtab has no build".into()))?;
    let specs = b["specializations"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|s| ApiSpec { id: num(&s["id"]) as u16, traits: [num(&s["traits"][0]), num(&s["traits"][1]), num(&s["traits"][2])] })
                .collect()
        })
        .unwrap_or_default();
    let sk = &b["skills"];
    Ok(ApiBuild {
        profession: b["profession"].as_str().unwrap_or("").to_string(),
        specs,
        skills: ApiSkills {
            heal: num(&sk["heal"]),
            utilities: [num(&sk["utilities"][0]), num(&sk["utilities"][1]), num(&sk["utilities"][2])],
            elite: num(&sk["elite"]),
        },
    })
}

fn parse_equipment(tab: &Value) -> Result<Vec<ApiItem>, ApiError> {
    let items = tab
        .get("equipment")
        .and_then(Value::as_array)
        .ok_or_else(|| ApiError::Parse("equipment tab has no equipment".into()))?;
    Ok(items
        .iter()
        .filter_map(|i| {
            Some(ApiItem {
                id: i["id"].as_u64()? as u32,
                slot: i["slot"].as_str().unwrap_or("").to_string(),
                upgrades: ids(&i["upgrades"]),
                infusions: ids(&i["infusions"]),
                stats_id: i["stats"]["id"].as_u64().map(|v| v as u32),
            })
        })
        .collect())
}

pub fn fetch_snapshot(http: &dyn Http, key: &str, character: &str, now: Instant) -> Result<ApiSnapshot, ApiError> {
    if key.trim().is_empty() {
        return Err(ApiError::NoKey);
    }
    let build = get_json(http, &character_url(character, &["buildtabs", "active"]), key)?;
    let equipment = get_json(http, &character_url(character, &["equipmenttabs", "active"]), key)?;
    Ok(ApiSnapshot {
        fetched_at: now,
        character: character.to_string(),
        build: parse_build(&build)?,
        equipment: parse_equipment(&equipment)?,
    })
}

pub fn tokeninfo(http: &dyn Http, key: &str) -> Result<TokenInfo, ApiError> {
    if key.trim().is_empty() {
        return Err(ApiError::NoKey);
    }
    let v = get_json(http, &format!("{API_ROOT}/v2/tokeninfo"), key)?;
    Ok(TokenInfo {
        name: v["name"].as_str().unwrap_or("").to_string(),
        permissions: v["permissions"]
            .as_array()
            .map(|a| a.iter().filter_map(Value::as_str).map(String::from).collect())
            .unwrap_or_default(),
    })
}

pub fn missing_permissions(info: &TokenInfo) -> Vec<&'static str> {
    ["characters", "builds"].into_iter().filter(|p| !info.permissions.iter().any(|x| x == p)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::fake::FakeHttp;

    const BUILDTAB: &str = r#"{"tab":1,"is_active":true,"build":{"name":"","profession":"Guardian",
        "specializations":[{"id":42,"traits":[634,653,2017]},{"id":46,"traits":[624,610,622]},{"id":62,"traits":[2086,2063,2105]}],
        "skills":{"heal":41714,"utilities":[40915,9153,9246],"elite":43357},
        "aquatic_skills":{"heal":null,"utilities":[null,null,null],"elite":null}}}"#;
    const EQUIPTAB: &str = r#"{"tab":1,"name":"","is_active":true,"equipment":[
        {"id":48073,"slot":"Helm","upgrades":[24842],"infusions":[37133],"stats":{"id":1123,"attributes":{}}},
        {"id":30699,"slot":"WeaponA1","upgrades":[24865]},
        {"id":101580,"slot":"Relic"}],"equipment_pvp":{}}"#;

    fn urls(name: &str) -> (String, String) {
        (character_url(name, &["buildtabs", "active"]), character_url(name, &["equipmenttabs", "active"]))
    }

    #[test]
    fn character_names_are_percent_encoded() {
        assert_eq!(
            character_url("Zoë Ström", &["buildtabs", "active"]),
            "https://api.guildwars2.com/v2/characters/Zo%C3%AB%20Str%C3%B6m/buildtabs/active"
        );
    }

    #[test]
    fn fetches_and_parses_the_active_tabs() {
        let http = FakeHttp::new();
        let (b, e) = urls("Zoë Ström");
        http.on(&b, 200, BUILDTAB).on(&e, 200, EQUIPTAB);
        let now = Instant::now();
        let snap = fetch_snapshot(&http, "KEY", "Zoë Ström", now).unwrap();
        assert_eq!(snap.character, "Zoë Ström");
        assert_eq!(snap.fetched_at, now);
        assert_eq!(snap.build.profession, "Guardian");
        assert_eq!(snap.spec3(), 62);
        assert_eq!(snap.build.specs[0].traits, [634, 653, 2017]);
        assert_eq!(snap.build.skills.utilities, [40915, 9153, 9246]);
        let helm = snap.item(GearSlot::Head).unwrap();
        assert_eq!((helm.upgrades.clone(), helm.infusions.clone(), helm.stats_id), (vec![24842], vec![37133], Some(1123)));
        assert_eq!(snap.item(GearSlot::WeaponA1).unwrap().infusions, Vec::<u32>::new());
        assert_eq!(snap.relic().unwrap().id, 101580);
        assert_eq!(http.header(0, "Authorization").as_deref(), Some("Bearer KEY"));
    }

    #[test]
    fn revenant_null_skills_become_zero() {
        let http = FakeHttp::new();
        let (b, e) = urls("Rev");
        http.on(&b, 200, r#"{"build":{"profession":"Revenant","specializations":[],"skills":{"heal":null,"utilities":[null,null,null],"elite":null}}}"#)
            .on(&e, 200, r#"{"equipment":[]}"#);
        let snap = fetch_snapshot(&http, "KEY", "Rev", Instant::now()).unwrap();
        assert_eq!(snap.build.skills, ApiSkills::default());
    }

    #[test]
    fn error_mapping() {
        let (b, _) = urls("A");
        let cases: [(u16, &str, ApiError); 5] = [
            (401, r#"{"text":"Invalid access token"}"#, ApiError::Unauthorized("Invalid access token".into())),
            (403, r#"{"text":"requires scope builds"}"#, ApiError::Unauthorized("requires scope builds".into())),
            (404, r#"{"text":"no such character"}"#, ApiError::CharacterNotFound),
            (429, "", ApiError::RateLimited),
            (503, "", ApiError::Server(503)),
        ];
        for (status, body, want) in cases {
            let http = FakeHttp::new();
            http.on(&b, status, body);
            assert_eq!(fetch_snapshot(&http, "KEY", "A", Instant::now()), Err(want));
        }
        let http = FakeHttp::new();
        http.fail(&b, "timed out");
        assert_eq!(fetch_snapshot(&http, "KEY", "A", Instant::now()), Err(ApiError::Network("timed out".into())));
        assert!(ApiError::RateLimited.backoff() && !ApiError::CharacterNotFound.backoff());
    }

    #[test]
    fn no_key_makes_no_request() {
        let http = FakeHttp::new();
        assert_eq!(fetch_snapshot(&http, "  ", "A", Instant::now()), Err(ApiError::NoKey));
        assert!(http.calls().is_empty());
    }

    #[test]
    fn tokeninfo_and_permissions() {
        let http = FakeHttp::new();
        http.on("https://api.guildwars2.com/v2/tokeninfo", 200, r#"{"id":"x","name":"axigear","permissions":["account","characters"]}"#);
        let info = tokeninfo(&http, "KEY").unwrap();
        assert_eq!(info.name, "axigear");
        assert_eq!(missing_permissions(&info), vec!["builds"]);
    }
}
