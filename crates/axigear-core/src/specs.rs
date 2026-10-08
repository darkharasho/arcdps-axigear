//! Bundled GW2 specialization data (scripts/gen-specdb.py): which specs are
//! elite, and each spec's major traits grouped by tier in API order — the
//! grouping AxiForge uses for trait positions.

use std::collections::HashMap;

use once_cell::sync::Lazy;
use serde::Deserialize;

use crate::model::Build;

#[derive(Debug, Clone, Default, Deserialize)]
pub struct TraitInfo {
    pub name: String,
    pub icon: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SpecInfo {
    pub id: u16,
    pub name: String,
    pub profession: String,
    pub elite: bool,
    pub majors: [Vec<u32>; 3],
    #[serde(default)]
    pub icon: String,
    #[serde(default)]
    pub background: String,
    #[serde(default)]
    pub minors: Vec<u32>,
    /// Minor and major traits of this spec by ID (JSON keys are strings; serde_json parses them as u32).
    #[serde(default)]
    pub traits: HashMap<u32, TraitInfo>,
}

pub struct SpecDb {
    by_id: HashMap<u16, SpecInfo>,
}

static BUNDLED: Lazy<SpecDb> = Lazy::new(|| {
    SpecDb::from_json(include_str!("../data/specializations.json")).expect("bundled specializations.json")
});

impl SpecDb {
    pub fn bundled() -> &'static SpecDb {
        &BUNDLED
    }

    pub fn from_json(s: &str) -> Result<SpecDb, serde_json::Error> {
        let rows: Vec<SpecInfo> = serde_json::from_str(s)?;
        Ok(SpecDb { by_id: rows.into_iter().map(|r| (r.id, r)).collect() })
    }

    pub fn get(&self, id: u16) -> Option<&SpecInfo> {
        self.by_id.get(&id)
    }

    pub fn is_elite(&self, id: u16) -> bool {
        self.get(id).is_some_and(|s| s.elite)
    }

    pub fn name(&self, id: u16) -> Option<&str> {
        self.get(id).map(|s| s.name.as_str())
    }

    pub fn trait_info(&self, spec: u16, trait_id: u32) -> Option<&TraitInfo> {
        self.get(spec)?.traits.get(&trait_id)
    }

    /// Position (1..=3) of `trait_id` in `tier` (1..=3) of `spec`.
    pub fn position(&self, spec: u16, tier: usize, trait_id: u32) -> Option<u8> {
        let tier = self.get(spec)?.majors.get(tier.checked_sub(1)?)?;
        tier.iter().position(|t| *t == trait_id).map(|i| i as u8 + 1)
    }

    /// Trait ID at `pos` (1..=3) in `tier` (1..=3) of `spec`.
    pub fn trait_at(&self, spec: u16, tier: usize, pos: u8) -> Option<u32> {
        let tier = self.get(spec)?.majors.get(tier.checked_sub(1)?)?;
        tier.get((pos as usize).checked_sub(1)?).copied()
    }
}

impl Build {
    /// The elite spec in line 3, if it is one. Published records say so
    /// themselves; codes are looked up.
    pub fn elite_spec(&self, specs: &SpecDb) -> Option<u16> {
        let line = &self.specs[2];
        let elite = line.elite.unwrap_or_else(|| specs.is_elite(line.id));
        (line.id != 0 && elite).then_some(line.id)
    }

    /// "Firebrand" or "Necromancer (core)".
    pub fn spec_label(&self, specs: &SpecDb) -> String {
        match self.elite_spec(specs).and_then(|id| specs.name(id)) {
            Some(name) => name.to_string(),
            None => format!("{} (core)", self.profession),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::axicode::decode_build_code;
    use crate::testutil::fixture;

    #[test]
    fn knows_elites_and_names() {
        let db = SpecDb::bundled();
        assert!(db.is_elite(62));
        assert!(!db.is_elite(42));
        assert_eq!(db.name(62), Some("Firebrand"));
        assert!(db.get(9999).is_none());
    }

    #[test]
    fn positions_and_trait_ids_round_trip() {
        let db = SpecDb::bundled();
        assert_eq!(db.position(62, 1, 2086), Some(3));
        assert_eq!(db.trait_at(62, 1, 3), Some(2086));
        assert_eq!(db.trait_at(62, 3, 1), Some(2105));
        assert_eq!(db.position(62, 1, 2105), None);
        assert_eq!(db.trait_at(62, 0, 1), None);
        assert_eq!(db.trait_at(62, 1, 0), None);
    }

    #[test]
    fn build_elite_and_label() {
        let db = SpecDb::bundled();
        let fb = decode_build_code(&fixture("build-firebrand.txt")).unwrap();
        let necro = decode_build_code(&fixture("build-necro.txt")).unwrap();
        assert_eq!(fb.elite_spec(db), Some(62));
        assert_eq!(fb.spec_label(db), "Firebrand");
        assert_eq!(necro.elite_spec(db), None);
        assert_eq!(necro.spec_label(db), "Necromancer (core)");

        let mut told = fb.clone();
        told.specs[2].elite = Some(false);
        assert_eq!(told.elite_spec(db), None, "a published flag wins over the lookup");
    }

    #[test]
    fn every_spec_has_art_minors_and_trait_icons() {
        let db = SpecDb::bundled();
        let mut n = 0;
        for s in db.by_id.values() {
            n += 1;
            assert!(s.icon.starts_with("https://render.guildwars2.com/"), "{} icon", s.name);
            assert!(s.background.starts_with("https://render.guildwars2.com/"), "{} background", s.name);
            assert_eq!(s.minors.len(), 3, "{} minors", s.name);
            for t in s.minors.iter().chain(s.majors.iter().flatten()) {
                let info = s.traits.get(t).unwrap_or_else(|| panic!("{} trait {t}", s.name));
                assert!(!info.name.is_empty() && info.icon.starts_with("https://render.guildwars2.com/"), "{} trait {t}", s.name);
            }
        }
        assert!(n > 60);
    }

    #[test]
    fn trait_info_lookup() {
        let db = SpecDb::bundled();
        let t = db.trait_info(62, 2086).expect("Firebrand adept bottom");
        assert!(!t.name.is_empty());
        assert!(db.trait_info(62, 1).is_none());
    }
}
