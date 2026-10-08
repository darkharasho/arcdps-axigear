//! Turns whatever the user pasted into a comp: codes decode offline, links
//! fetch the published `.enc` (raw host first, Pages second), with ETag.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::axicode::{decode_build_code, decode_comp_code, is_comp_code};
use crate::http::{self, Http};
use crate::link::{self, AxiLink, LinkKind};
use crate::model::{Build, Comp};
use crate::publish::{self, Member, PublishError};

#[derive(Debug, Clone, PartialEq)]
pub enum Input {
    Comp { comp: Comp, key: String },
    Link(AxiLink),
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LoadError {
    #[error("{0}")]
    Code(String),
    #[error("{0}")]
    Link(String),
    #[error("That link isn't published anymore.")]
    NotPublished,
    #[error("offline ({0})")]
    Offline(String),
    #[error("Couldn't decrypt - check the link.")]
    Decrypt,
    #[error("Comp file is too large - republish it from AxiForge.")]
    TooLarge,
    #[error("Comp made with a newer AxiForge - update axigear.")]
    NewerSchema,
    #[error("Couldn't read the comp: {0}")]
    Corrupt(String),
}

impl LoadError {
    /// Worth retrying with backoff (network, or a file GitHub hasn't served yet).
    pub fn retryable(&self) -> bool {
        matches!(self, LoadError::Offline(_) | LoadError::NotPublished)
    }
}

impl From<PublishError> for LoadError {
    fn from(e: PublishError) -> Self {
        match e {
            PublishError::BadKey | PublishError::Decrypt => LoadError::Decrypt,
            PublishError::NewerSchema(_) => LoadError::NewerSchema,
            PublishError::Corrupt | PublishError::Json(_) => LoadError::Corrupt(e.to_string()),
        }
    }
}

/// What a v2 member file last returned: its ETag and the build it held (`None` = 404 or
/// unusable). Kept per member URL so a comp that answers 304 can still re-poll its members.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MemberState {
    pub etag: Option<String>,
    pub build: Option<Build>,
}

/// Member URL -> last known state.
pub type MemberCache = BTreeMap<String, MemberState>;

#[derive(Debug, Clone, PartialEq)]
pub enum Fetched {
    NotModified,
    Fresh {
        comp: Comp,
        etag: Option<String>,
        link: AxiLink,
        /// Decrypted comp plaintext (comp links only), for re-polling members on a later 304.
        plain: Option<Vec<u8>>,
        members: MemberCache,
    },
}

fn fnv1a(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// Strip what chat apps wrap around a pasted code: whitespace, backticks, quotes.
fn clean(input: &str) -> &str {
    input.trim_matches(|c: char| c.is_whitespace() || matches!(c, '`' | '"' | '\''))
}

pub fn code_key(code: &str) -> String {
    format!("code:{:016x}", fnv1a(clean(code).as_bytes()))
}

pub fn link_key(link: &AxiLink) -> String {
    match (&link.file_id, &link.short_url) {
        (Some(id), _) => format!("link:{id}"),
        (None, Some(short)) => format!("short:{short}"),
        (None, None) => "link:?".into(),
    }
}

pub fn detect(input: &str) -> Result<Input, LoadError> {
    let text = clean(input);
    if is_comp_code(text) {
        let comp = decode_comp_code(text).map_err(|e| LoadError::Code(e.to_string()))?;
        return Ok(Input::Comp { comp, key: code_key(text) });
    }
    if text.starts_with("<AxiForge:") {
        let build = decode_build_code(text).map_err(|e| LoadError::Code(e.to_string()))?;
        return Ok(Input::Comp { comp: Comp::single(build), key: code_key(text) });
    }
    link::parse(text).map(Input::Link).map_err(|e| LoadError::Link(e.to_string()))
}

fn resolve(http: &dyn Http, link: &AxiLink) -> Result<AxiLink, LoadError> {
    if link.is_resolved() {
        return Ok(link.clone());
    }
    let short = link.short_url.as_deref().ok_or_else(|| LoadError::Link(link::LinkError::NoRef.to_string()))?;
    let resp = http.get(short, &[]).map_err(LoadError::Offline)?;
    match resp.status {
        200 => link::resolve_short(short, &resp.text()).map_err(|e| LoadError::Link(e.to_string())),
        404 => Err(LoadError::NotPublished),
        s => Err(LoadError::Offline(format!("HTTP {s}"))),
    }
}

fn valid_part(s: &str, max: usize) -> bool {
    (1..=max).contains(&s.len()) && !s.starts_with('.') && s.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'-'))
}

fn member_url(m: &Member) -> String {
    format!("https://raw.githubusercontent.com/{}/axibuilds/main/site/builds/{}.enc", m.owner, m.file_id)
}

/// Fetch one linked member build, revalidating against `cached` with `If-None-Match`.
/// A usable build in the result may be `None` (404, undecryptable, unparsable);
/// `Err` = could not reach it (transport error or a status other than 200/304/404).
fn fetch_member(http: &dyn Http, url: &str, m: &Member, cached: Option<&MemberState>) -> Result<MemberState, String> {
    let cached = cached.filter(|c| c.etag.is_some() && c.build.is_some());
    let headers: Vec<(&str, &str)> = cached.and_then(|c| c.etag.as_deref()).map(|e| ("If-None-Match", e)).into_iter().collect();
    let resp = http.get(url, &headers)?;
    match (resp.status, cached) {
        (200, _) => Ok(MemberState {
            etag: resp.etag.clone(),
            build: publish::decrypt(&resp.body, &m.key).and_then(|plain| publish::parse_build(&plain)).ok(),
        }),
        (304, Some(c)) => Ok(c.clone()),
        (404, _) => Ok(MemberState::default()),
        (s, _) => Err(format!("HTTP {s}")),
    }
}

/// v1 comps embed their builds; v2 comps link to member files, fetched here. Members that
/// are gone or unusable are left out (and counted in `Comp::missing_members`). The first
/// member that can't be reached fails the whole load as `Offline`, so a retry replaces the
/// comp instead of committing (and ETag-pinning) a partial one.
fn parse_comp_loading_members(http: &dyn Http, plain: &[u8], cache: &MemberCache) -> Result<(Comp, MemberCache), LoadError> {
    let members = publish::comp_members(plain)?;
    let (mut builds, mut next) = (BTreeMap::new(), MemberCache::new());
    for m in members.iter().filter(|m| valid_part(&m.owner, 100) && valid_part(&m.file_id, 64)) {
        let url = member_url(m);
        let state = fetch_member(http, &url, m, cache.get(&url)).map_err(LoadError::Offline)?;
        if let Some(build) = &state.build {
            builds.insert(m.build_id.clone(), build.clone());
        }
        next.insert(url, state);
    }
    Ok((publish::parse_comp_with(plain, &builds)?, next))
}

/// Re-poll the members of a comp whose own file answered 304. `Ok(None)` when nothing
/// changed (every member 304); otherwise the rebuilt comp and the new member states.
pub fn refresh_members(http: &dyn Http, plain: &[u8], cache: &MemberCache) -> Result<Option<(Comp, MemberCache)>, LoadError> {
    if publish::comp_members(plain)?.is_empty() {
        return Ok(None); // v1: builds are embedded, nothing to re-poll
    }
    let (comp, next) = parse_comp_loading_members(http, plain, cache)?;
    Ok((next != *cache).then_some((comp, next)))
}

/// `members` revalidates v2 member files fetched before (pass an empty cache for a first load).
pub fn fetch(http: &dyn Http, link: &AxiLink, etag: Option<&str>, members: &MemberCache) -> Result<Fetched, LoadError> {
    let link = resolve(http, link)?;
    let key = link.key.clone().unwrap_or_default();
    let mut saw_404 = false;
    let mut last_err = String::from("no data source");
    for url in link.enc_urls() {
        let headers: Vec<(&str, &str)> = etag.map(|e| ("If-None-Match", e)).into_iter().collect();
        match http.get(&url, &headers) {
            Err(e) if e == http::TOO_LARGE => return Err(LoadError::TooLarge), // the other host serves the same file
            Err(e) => last_err = e,
            Ok(r) if r.status == 304 => return Ok(Fetched::NotModified),
            Ok(r) if r.status == 200 => {
                let plain = publish::decrypt(&r.body, &key)?;
                return Ok(match link.kind {
                    LinkKind::Comp => {
                        let (comp, members) = parse_comp_loading_members(http, &plain, members)?;
                        Fetched::Fresh { comp, etag: r.etag, link, plain: Some(plain), members }
                    }
                    LinkKind::Build => {
                        let comp = Comp::single(publish::parse_build(&plain)?);
                        Fetched::Fresh { comp, etag: r.etag, link, plain: None, members: MemberCache::new() }
                    }
                });
            }
            Ok(r) => {
                saw_404 |= r.status == 404;
                last_err = format!("HTTP {}", r.status);
            }
        }
    }
    Err(if saw_404 { LoadError::NotPublished } else { LoadError::Offline(last_err) })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::fake::FakeHttp;
    use crate::testutil::{fixture, fixture_bytes, seal_v2};

    const RAW: &str = "https://raw.githubusercontent.com/someone/axibuilds/main/site/comps/e4369a53.enc";
    const PAGES: &str = "https://someone.github.io/axibuilds/comps/e4369a53.enc";

    fn comp_link() -> AxiLink {
        link::parse(&format!("https://someone.github.io/axibuilds/?c=e4369a53.{}", fixture("fixture.key"))).unwrap()
    }

    #[test]
    fn detects_codes_and_links() {
        match detect(&fixture("comp-tuesday.txt")).unwrap() {
            Input::Comp { comp, key } => {
                assert_eq!(comp.name, "Tuesday Zerg");
                assert!(key.starts_with("code:"));
            }
            other => panic!("{other:?}"),
        }
        match detect(&fixture("build-firebrand.txt")).unwrap() {
            Input::Comp { comp, .. } => assert_eq!(comp.builds.len(), 1),
            other => panic!("{other:?}"),
        }
        assert!(matches!(detect("https://someone.github.io/axibuilds/?c=aa.bb").unwrap(), Input::Link(_)));
    }

    #[test]
    fn pasted_code_wrapped_by_chat_still_loads() {
        let wrapped = format!("  `{}`\n", fixture("comp-tuesday.txt"));
        assert!(matches!(detect(&wrapped).unwrap(), Input::Comp { .. }));
        assert_eq!(code_key(&wrapped), code_key(&fixture("comp-tuesday.txt")));
    }

    #[test]
    fn garbage_reports_the_right_error() {
        assert!(matches!(detect("<AxiForge:Comp:@@>"), Err(LoadError::Code(_))));
        assert!(matches!(detect("hello"), Err(LoadError::Link(_))));
    }

    #[test]
    fn fetches_from_raw_with_etag() {
        let http = FakeHttp::new();
        http.on_etag(RAW, 200, &fixture("comp-tuesday.enc"), "\"v1\"");
        match fetch(&http, &comp_link(), None, &MemberCache::new()).unwrap() {
            Fetched::Fresh { comp, etag, .. } => {
                assert_eq!(comp.name, "Tuesday Zerg");
                assert_eq!(etag.as_deref(), Some("\"v1\""));
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(http.calls(), vec![RAW]);
    }

    #[test]
    fn not_modified_sends_if_none_match() {
        let http = FakeHttp::new();
        http.on(RAW, 304, "");
        assert_eq!(fetch(&http, &comp_link(), Some("\"v1\""), &MemberCache::new()).unwrap(), Fetched::NotModified);
        assert_eq!(http.header(0, "If-None-Match").as_deref(), Some("\"v1\""));
    }

    #[test]
    fn falls_back_to_pages_then_reports_unpublished_or_offline() {
        let http = FakeHttp::new();
        http.on(RAW, 404, "").on(PAGES, 200, &fixture("comp-tuesday.enc"));
        assert!(matches!(fetch(&http, &comp_link(), None, &MemberCache::new()).unwrap(), Fetched::Fresh { .. }));
        assert_eq!(http.calls(), vec![RAW, PAGES]);

        let gone = FakeHttp::new();
        assert_eq!(fetch(&gone, &comp_link(), None, &MemberCache::new()), Err(LoadError::NotPublished));

        let offline = FakeHttp::new();
        offline.fail(RAW, "dns").fail(PAGES, "dns");
        assert!(matches!(fetch(&offline, &comp_link(), None, &MemberCache::new()), Err(LoadError::Offline(_))));
    }

    #[test]
    fn oversized_file_stops_without_trying_the_other_host() {
        let http = FakeHttp::new();
        http.fail(RAW, crate::http::TOO_LARGE).on(PAGES, 200, &fixture("comp-tuesday.enc"));
        let err = fetch(&http, &comp_link(), None, &MemberCache::new()).unwrap_err();
        assert_eq!(err, LoadError::TooLarge);
        assert!(!err.retryable());
        assert_eq!(http.calls(), vec![RAW]);
    }

    #[test]
    fn wrong_key_stops_at_the_first_file() {
        let http = FakeHttp::new();
        http.on(RAW, 200, &fixture("comp-tuesday.enc"));
        let link = link::parse("https://someone.github.io/axibuilds/?c=e4369a53.AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh4").unwrap();
        assert_eq!(fetch(&http, &link, None, &MemberCache::new()), Err(LoadError::Decrypt));
        assert_eq!(http.calls().len(), 1);
    }

    #[test]
    fn resolves_short_links_before_fetching() {
        let http = FakeHttp::new();
        let short = "https://someone.github.io/axibuilds/r/e4369a53/";
        let page = format!(r#"<!DOCTYPE html><meta http-equiv=refresh content="0;url=../../?c=e4369a53.{}">"#, fixture("fixture.key"));
        http.on(short, 200, &page).on(RAW, 200, &fixture("comp-tuesday.enc"));
        let link = link::parse("https://someone.github.io/axibuilds/r/e4369a53").unwrap();
        match fetch(&http, &link, None, &MemberCache::new()).unwrap() {
            Fetched::Fresh { link, .. } => assert!(link.is_resolved()),
            other => panic!("{other:?}"),
        }
        assert_eq!(http.calls(), vec![short, RAW]);
    }

    #[test]
    fn build_links_become_one_slot_comps() {
        let http = FakeHttp::new();
        let raw = "https://raw.githubusercontent.com/someone/axibuilds/main/site/builds/f4c38d4f.enc";
        http.on(raw, 200, &fixture("build-firebrand.enc"));
        let link = link::parse(&format!("https://someone.github.io/axibuilds/?b=f4c38d4f.{}", fixture("fixture.key"))).unwrap();
        match fetch(&http, &link, None, &MemberCache::new()).unwrap() {
            Fetched::Fresh { comp, .. } => assert_eq!(comp.name, "Quickbrand"),
            other => panic!("{other:?}"),
        }
    }

    const MEMBER_BASE: &str = "https://raw.githubusercontent.com/teammate/axibuilds/main/site/builds/";
    const MEMBER_FILES: [(&str, &str); 3] =
        [("aaaa0001", "member-firebrand.enc"), ("bbbb0002", "member-berserker.enc.v2"), ("cccc0003", "member-necro.enc.v2")];

    fn v2_http() -> FakeHttp {
        v2_http_without("")
    }

    /// The v2 comp plus its member files, except member `skip` (a 404).
    fn v2_http_without(skip: &str) -> FakeHttp {
        let http = FakeHttp::new();
        http.on_bytes(RAW, 200, &fixture_bytes("comp-tuesday.v2.enc.v2"));
        for (id, file) in MEMBER_FILES.into_iter().filter(|(id, _)| *id != skip) {
            http.on_bytes(&format!("{MEMBER_BASE}{id}.enc"), 200, &fixture_bytes(file));
        }
        http
    }

    fn fresh_comp(r: Result<Fetched, LoadError>) -> Comp {
        match r.unwrap() {
            Fetched::Fresh { comp, .. } => comp,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn v2_comp_loads_its_members_like_the_v1_comp() {
        let http = v2_http();
        let comp = fresh_comp(fetch(&http, &comp_link(), None, &MemberCache::new()));
        let v1 = FakeHttp::new();
        v1.on(RAW, 200, &fixture("comp-tuesday.enc"));
        assert_eq!(comp, fresh_comp(fetch(&v1, &comp_link(), None, &MemberCache::new())));
        assert_eq!(http.calls().len(), 4);
        assert!(http.header(1, "If-None-Match").is_none());
    }

    #[test]
    fn v2_build_links_load() {
        let http = FakeHttp::new();
        let raw = "https://raw.githubusercontent.com/someone/axibuilds/main/site/builds/f4c38d4f.enc";
        http.on_bytes(raw, 200, &fixture_bytes("build-firebrand.enc.v2"));
        let link = link::parse(&format!("https://someone.github.io/axibuilds/?b=f4c38d4f.{}", fixture("fixture.key"))).unwrap();
        assert_eq!(fresh_comp(fetch(&http, &link, None, &MemberCache::new())).name, "Quickbrand");
    }

    #[test]
    fn a_missing_member_is_omitted() {
        let http = v2_http_without("bbbb0002");
        let comp = fresh_comp(fetch(&http, &comp_link(), None, &MemberCache::new()));
        let profs: Vec<&str> = comp.builds.iter().map(|b| b.profession.as_str()).collect();
        assert_eq!(profs, ["Guardian", "Necromancer"]);
        assert_eq!(comp.name, "Tuesday Zerg");
    }

    #[test]
    fn an_undecryptable_member_is_omitted() {
        let http = v2_http_without("cccc0003");
        http.on_bytes(&format!("{MEMBER_BASE}cccc0003.enc"), 200, b"garbage");
        assert_eq!(fresh_comp(fetch(&http, &comp_link(), None, &MemberCache::new())).builds.len(), 2);
    }

    #[test]
    fn any_unreachable_member_makes_the_load_offline() {
        for failure in [Err("dns"), Ok(503)] {
            let http = v2_http_without("bbbb0002");
            match failure {
                Err(e) => http.fail(&format!("{MEMBER_BASE}bbbb0002.enc"), e),
                Ok(status) => http.on(&format!("{MEMBER_BASE}bbbb0002.enc"), status, ""),
            };
            assert!(matches!(fetch(&http, &comp_link(), None, &MemberCache::new()), Err(LoadError::Offline(_))));
        }
    }

    #[test]
    fn a_404_member_is_counted_as_missing() {
        let comp = fresh_comp(fetch(&v2_http_without("aaaa0001"), &comp_link(), None, &MemberCache::new()));
        assert_eq!(comp.missing_members, 1);
        let all = fresh_comp(fetch(&v2_http(), &comp_link(), None, &MemberCache::new()));
        assert_eq!(all.missing_members, 0);
    }

    #[test]
    fn invalid_owner_or_file_id_never_reaches_the_network() {
        let comp = br#"{"v":2,"name":"Bad","partyLines":[{"slots":["a","b","c","d","e","f"]}],"members":{
            "a":{"fileId":"f1","key":"k","owner":"../x"},
            "b":{"fileId":"../f","key":"k","owner":"ok"},
            "c":{"fileId":"","key":"k","owner":"ok"},
            "d":{"fileId":"f4","key":"k","owner":"o/p"},
            "e":{"fileId":".hidden","key":"k","owner":"ok"},
            "f":{"fileId":"f6","key":"k","owner":".."}}}"#;
        let http = FakeHttp::new();
        http.on_bytes(RAW, 200, &seal_v2(comp, &fixture("fixture.key")));
        let loaded = fresh_comp(fetch(&http, &comp_link(), None, &MemberCache::new()));
        assert_eq!(loaded.name, "Bad");
        assert!(loaded.builds.is_empty());
        assert_eq!(http.calls(), vec![RAW]);
    }
}
