//! `comp_cache.json`: the cached copy of every saved comp, so switching
//! comps or restarting needs no network. v0.1.x wrote a single comp.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::session::LoadedComp;

#[derive(Serialize, Deserialize)]
struct File {
    comps: Vec<LoadedComp>,
}

/// Every cached comp. Accepts the v0.1.x single-comp file; anything
/// unreadable is treated as empty (comps then reload on use).
pub fn load(path: &Path) -> Vec<LoadedComp> {
    let Ok(text) = std::fs::read_to_string(path) else { return Vec::new() };
    if let Ok(f) = serde_json::from_str::<File>(&text) {
        return f.comps;
    }
    serde_json::from_str::<LoadedComp>(&text).map(|c| vec![c]).unwrap_or_default()
}

pub fn save(path: &Path, comps: &[LoadedComp]) -> std::io::Result<()> {
    let json = serde_json::to_vec(&File { comps: comps.to_vec() }).map_err(std::io::Error::other)?;
    crate::fsutil::write_atomic(path, &json)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::loader::{detect, Input};
    use crate::session::CompOrigin;
    use crate::testutil::fixture;

    fn code_comp(input: &str) -> LoadedComp {
        let Ok(Input::Comp { comp, key }) = detect(&fixture("comp-tuesday.txt")) else { panic!("fixture") };
        LoadedComp { comp, key, input: input.into(), origin: CompOrigin::Code }
    }

    #[test]
    fn round_trips_several_comps() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("comp_cache.json");
        let comps = vec![code_comp("A"), code_comp("B")];
        save(&path, &comps).unwrap();
        assert_eq!(load(&path), comps);
        assert!(std::fs::read_to_string(&path).unwrap().starts_with("{\"comps\":"));
    }

    #[test]
    fn reads_the_v01_single_comp_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("comp_cache.json");
        std::fs::write(&path, serde_json::to_vec(&code_comp("OLD")).unwrap()).unwrap();
        assert_eq!(load(&path), vec![code_comp("OLD")]);
    }

    #[test]
    fn missing_or_broken_file_is_empty() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("comp_cache.json");
        assert!(load(&path).is_empty());
        std::fs::write(&path, "{nope").unwrap();
        assert!(load(&path).is_empty());
    }
}
