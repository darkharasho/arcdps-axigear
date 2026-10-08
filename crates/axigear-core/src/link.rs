//! Port of AxiForge's `parseAxiLink` (src/main/axiLinkImport.js) plus the
//! `/r/<id>/` short-link resolution. Forms, first match wins:
//! `?c=id.key` comp · `?b=` / `?legacy=` / `#id.key` build · `/r/<id>/` short.

use serde::{Deserialize, Serialize};
use url::Url;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LinkKind {
    Comp,
    Build,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AxiLink {
    pub kind: LinkKind,
    pub file_id: Option<String>,
    pub key: Option<String>,
    /// Data roots to try in order, each ending in `/` (raw.githubusercontent first).
    pub bases: Vec<String>,
    /// Set for `/r/<id>/` links until resolved.
    pub short_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LinkError {
    #[error("Paste an AxiForge code or link.")]
    Empty,
    #[error("That doesn't look like a link or an AxiForge code.")]
    NotALink,
    #[error("That link has no build or comp in it. Copy the full link.")]
    NoRef,
    #[error("That short link doesn't point at a build or comp.")]
    BadShortLink,
}

impl AxiLink {
    pub fn is_resolved(&self) -> bool {
        self.file_id.is_some() && self.key.is_some()
    }

    /// `.enc` URLs to try, in order.
    pub fn enc_urls(&self) -> Vec<String> {
        let Some(id) = &self.file_id else { return Vec::new() };
        let dir = match self.kind {
            LinkKind::Comp => "comps",
            LinkKind::Build => "builds",
        };
        let id: String = url::form_urlencoded::byte_serialize(id.as_bytes()).collect();
        self.bases.iter().map(|b| format!("{b}{dir}/{id}.enc")).collect()
    }
}

fn split_ref(value: &str) -> Option<(String, String)> {
    let v = value.trim();
    let dot = v.find('.')?;
    if dot < 1 || dot == v.len() - 1 {
        return None;
    }
    Some((v[..dot].to_string(), v[dot + 1..].to_string()))
}

fn query(url: &Url, key: &str) -> Option<String> {
    url.query_pairs().find(|(k, _)| k == key).map(|(_, v)| v.into_owned())
}

fn data_bases(url: &Url) -> Vec<String> {
    if let Some(explicit) = query(url, "remoteBase") {
        return vec![if explicit.ends_with('/') { explicit } else { format!("{explicit}/") }];
    }
    let mut bases = Vec::new();
    let repo = url.path().split('/').find(|s| !s.is_empty()).unwrap_or("");
    let owner = url
        .host_str()
        .and_then(|h| h.strip_suffix(".github.io"))
        .filter(|o| !o.is_empty() && !o.contains('.'));
    if let (Some(owner), false) = (owner, repo.is_empty()) {
        bases.push(format!("https://raw.githubusercontent.com/{owner}/{repo}/main/site/"));
    }
    let path = url.path();
    let dir = &path[..path.rfind('/').map_or(0, |i| i + 1)];
    bases.push(format!("{}{}", url.origin().ascii_serialization(), if dir.is_empty() { "/" } else { dir }));
    bases
}

fn resolved(kind: LinkKind, (file_id, key): (String, String), bases: Vec<String>) -> AxiLink {
    AxiLink { kind, file_id: Some(file_id), key: Some(key), bases, short_url: None }
}

pub fn parse(input: &str) -> Result<AxiLink, LinkError> {
    let raw = input.trim();
    if raw.is_empty() {
        return Err(LinkError::Empty);
    }
    let text = if raw.starts_with("http") { raw.to_string() } else { format!("https://{raw}") };
    let url = Url::parse(&text).map_err(|_| LinkError::NotALink)?;
    if !url.host_str().is_some_and(|h| h.contains('.')) {
        return Err(LinkError::NotALink);
    }
    let bases = data_bases(&url);

    if let Some(r) = query(&url, "c").and_then(|v| split_ref(&v)) {
        return Ok(resolved(LinkKind::Comp, r, bases));
    }
    let build = query(&url, "b")
        .and_then(|v| split_ref(&v))
        .or_else(|| query(&url, "legacy").and_then(|v| split_ref(&v)))
        .or_else(|| url.fragment().and_then(split_ref));
    if let Some(r) = build {
        return Ok(resolved(LinkKind::Build, r, bases));
    }

    let path = url.path().trim_end_matches('/');
    if let Some(i) = path.rfind("/r/") {
        let id = &path[i + 3..];
        if !id.is_empty() && !id.contains('/') {
            let short_url = format!("{}{}/", url.origin().ascii_serialization(), path);
            return Ok(AxiLink { kind: LinkKind::Build, file_id: None, key: None, bases, short_url: Some(short_url) });
        }
    }
    Err(LinkError::NoRef)
}

/// The `/r/<id>/` page is a one-line meta refresh to `../../?c=<id>.<key>`
/// (axiforge siteBundle.js `buildRedirectFile`). Same match as AxiForge:
/// `[?&](b|c)=([^"'&\s>]+)`.
pub fn resolve_short(short_url: &str, body: &str) -> Result<AxiLink, LinkError> {
    let bytes = body.as_bytes();
    let found = (0..bytes.len().saturating_sub(2)).find_map(|i| {
        let marker = (bytes[i] == b'?' || bytes[i] == b'&') && matches!(bytes[i + 1], b'b' | b'c') && bytes[i + 2] == b'=';
        marker.then(|| {
            let rest = &body[i + 3..];
            let end = rest
                .find(|c: char| matches!(c, '"' | '\'' | '&' | '>') || c.is_whitespace())
                .unwrap_or(rest.len());
            (bytes[i + 1], &rest[..end])
        })
    });
    let (kind, value) = found.ok_or(LinkError::BadShortLink)?;
    let r = split_ref(value).ok_or(LinkError::BadShortLink)?;
    // The page lives at <root>/r/<id>/, so <root> is two levels up.
    let root = Url::parse(short_url).and_then(|u| u.join("../../")).map_err(|_| LinkError::BadShortLink)?;
    let kind = if kind == b'c' { LinkKind::Comp } else { LinkKind::Build };
    Ok(resolved(kind, r, data_bases(&root)))
}

#[cfg(test)]
mod tests {
    use super::*;

    const RAW: &str = "https://raw.githubusercontent.com/someone/axibuilds/main/site/";
    const PAGES: &str = "https://someone.github.io/axibuilds/";

    #[test]
    fn build_link_with_name_and_theme() {
        let l = parse("https://someone.github.io/axibuilds/?n=u-chrono&b=f4c38d4f.abc123&t=prof-mesmer").unwrap();
        assert_eq!(l.kind, LinkKind::Build);
        assert_eq!(l.file_id.as_deref(), Some("f4c38d4f"));
        assert_eq!(l.key.as_deref(), Some("abc123"));
        assert_eq!(l.bases, vec![RAW.to_string(), PAGES.to_string()]);
    }

    #[test]
    fn legacy_and_hash_forms() {
        for link in ["https://x.github.io/axibuilds/?legacy=aa.bb", "https://x.github.io/axibuilds/#aa.bb"] {
            let l = parse(link).unwrap();
            assert_eq!((l.kind, l.file_id.as_deref(), l.key.as_deref()), (LinkKind::Build, Some("aa"), Some("bb")));
        }
    }

    #[test]
    fn comp_link_and_enc_urls() {
        let l = parse("https://someone.github.io/axibuilds/?n=1200-range&c=e4369a53.KEY").unwrap();
        assert_eq!(l.kind, LinkKind::Comp);
        assert_eq!(
            l.enc_urls(),
            vec![format!("{RAW}comps/e4369a53.enc"), format!("{PAGES}comps/e4369a53.enc")]
        );
    }

    #[test]
    fn key_split_is_at_the_first_dot() {
        let l = parse("https://x.github.io/axibuilds/?c=aa.b.c").unwrap();
        assert_eq!((l.file_id.as_deref(), l.key.as_deref()), (Some("aa"), Some("b.c")));
    }

    #[test]
    fn scheme_is_optional() {
        assert_eq!(parse("someone.github.io/axibuilds/?c=aa.bb").unwrap().kind, LinkKind::Comp);
    }

    #[test]
    fn short_link_with_or_without_slash() {
        for link in ["https://x.github.io/axibuilds/r/f4c38d4f/", "https://x.github.io/axibuilds/r/f4c38d4f"] {
            let l = parse(link).unwrap();
            assert!(!l.is_resolved());
            assert_eq!(l.short_url.as_deref(), Some("https://x.github.io/axibuilds/r/f4c38d4f/"));
        }
    }

    #[test]
    fn other_hosts_and_remote_base() {
        assert_eq!(parse("https://builds.example.com/?b=aa.bb").unwrap().bases, vec!["https://builds.example.com/"]);
        assert_eq!(parse("https://x.github.io/axibuilds/?b=aa.bb&remoteBase=http://x/site").unwrap().bases, vec!["http://x/site/"]);
    }

    #[test]
    fn errors() {
        assert_eq!(parse("  "), Err(LinkError::Empty));
        assert_eq!(parse("hello"), Err(LinkError::NotALink));
        assert_eq!(parse("https://x.github.io/axibuilds/"), Err(LinkError::NoRef));
        assert_eq!(parse("https://x.github.io/axibuilds/?c=nodot"), Err(LinkError::NoRef));
    }

    #[test]
    fn resolves_a_short_link_page() {
        let body = r#"<!DOCTYPE html><meta http-equiv=refresh content="0;url=../../?c=e4369a53.KEY">"#;
        let l = resolve_short("https://someone.github.io/axibuilds/r/e4369a53/", body).unwrap();
        assert_eq!(l.kind, LinkKind::Comp);
        assert_eq!((l.file_id.as_deref(), l.key.as_deref()), (Some("e4369a53"), Some("KEY")));
        assert_eq!(l.bases, vec![RAW.to_string(), PAGES.to_string()]);
        assert_eq!(l.short_url, None);

        let build = resolve_short("https://someone.github.io/axibuilds/r/f4/", "url=../../?b=f4.k\"").unwrap();
        assert_eq!(build.kind, LinkKind::Build);
        assert_eq!(resolve_short("https://someone.github.io/axibuilds/r/f4/", "<html>nope</html>"), Err(LinkError::BadShortLink));
    }
}
