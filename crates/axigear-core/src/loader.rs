//! Turns whatever the user pasted into a comp: codes decode offline, links
//! fetch the published `.enc` (raw host first, Pages second), with ETag.

use crate::axicode::{decode_build_code, decode_comp_code, is_comp_code};
use crate::http::Http;
use crate::link::{self, AxiLink, LinkKind};
use crate::model::Comp;
use crate::publish::{self, PublishError};

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

#[derive(Debug, Clone, PartialEq)]
pub enum Fetched {
    NotModified,
    Fresh { comp: Comp, etag: Option<String>, link: AxiLink },
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
        200 => link::resolve_short(short, &resp.body).map_err(|e| LoadError::Link(e.to_string())),
        404 => Err(LoadError::NotPublished),
        s => Err(LoadError::Offline(format!("HTTP {s}"))),
    }
}

pub fn fetch(http: &dyn Http, link: &AxiLink, etag: Option<&str>) -> Result<Fetched, LoadError> {
    let link = resolve(http, link)?;
    let key = link.key.clone().unwrap_or_default();
    let mut saw_404 = false;
    let mut last_err = String::from("no data source");
    for url in link.enc_urls() {
        let headers: Vec<(&str, &str)> = etag.map(|e| ("If-None-Match", e)).into_iter().collect();
        match http.get(&url, &headers) {
            Err(e) => last_err = e,
            Ok(r) if r.status == 304 => return Ok(Fetched::NotModified),
            Ok(r) if r.status == 200 => {
                let plain = publish::decrypt(&r.body, &key)?;
                let comp = match link.kind {
                    LinkKind::Comp => publish::parse_comp(&plain)?,
                    LinkKind::Build => Comp::single(publish::parse_build(&plain)?),
                };
                return Ok(Fetched::Fresh { comp, etag: r.etag, link });
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
    use crate::testutil::fixture;

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
        match fetch(&http, &comp_link(), None).unwrap() {
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
        assert_eq!(fetch(&http, &comp_link(), Some("\"v1\"")).unwrap(), Fetched::NotModified);
        assert_eq!(http.header(0, "If-None-Match").as_deref(), Some("\"v1\""));
    }

    #[test]
    fn falls_back_to_pages_then_reports_unpublished_or_offline() {
        let http = FakeHttp::new();
        http.on(RAW, 404, "").on(PAGES, 200, &fixture("comp-tuesday.enc"));
        assert!(matches!(fetch(&http, &comp_link(), None).unwrap(), Fetched::Fresh { .. }));
        assert_eq!(http.calls(), vec![RAW, PAGES]);

        let gone = FakeHttp::new();
        assert_eq!(fetch(&gone, &comp_link(), None), Err(LoadError::NotPublished));

        let offline = FakeHttp::new();
        offline.fail(RAW, "dns").fail(PAGES, "dns");
        assert!(matches!(fetch(&offline, &comp_link(), None), Err(LoadError::Offline(_))));
    }

    #[test]
    fn wrong_key_stops_at_the_first_file() {
        let http = FakeHttp::new();
        http.on(RAW, 200, &fixture("comp-tuesday.enc"));
        let link = link::parse("https://someone.github.io/axibuilds/?c=e4369a53.AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh4").unwrap();
        assert_eq!(fetch(&http, &link, None), Err(LoadError::Decrypt));
        assert_eq!(http.calls().len(), 1);
    }

    #[test]
    fn resolves_short_links_before_fetching() {
        let http = FakeHttp::new();
        let short = "https://someone.github.io/axibuilds/r/e4369a53/";
        let page = format!(r#"<!DOCTYPE html><meta http-equiv=refresh content="0;url=../../?c=e4369a53.{}">"#, fixture("fixture.key"));
        http.on(short, 200, &page).on(RAW, 200, &fixture("comp-tuesday.enc"));
        let link = link::parse("https://someone.github.io/axibuilds/r/e4369a53").unwrap();
        match fetch(&http, &link, None).unwrap() {
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
        match fetch(&http, &link, None).unwrap() {
            Fetched::Fresh { comp, .. } => assert_eq!(comp.name, "Quickbrand"),
            other => panic!("{other:?}"),
        }
    }
}
