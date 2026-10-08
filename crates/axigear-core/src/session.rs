//! One player's view: the loaded comp, which slot is theirs, and what the
//! live/API state says about it.

use std::collections::BTreeMap;
use std::time::Instant;

use serde::{Deserialize, Serialize};

use crate::checks::{self, ApiState, Ctx};
use crate::consumables::Consumables;
use crate::gamedb::GameDb;
use crate::link::AxiLink;
use crate::live::LiveState;
use crate::matcher::{self, MatchOutcome};
use crate::model::{Build, Comp, SlotEntry, SlotRef};
use crate::report::{Badge, CheckReport, Severities};
use crate::specs::SpecDb;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CompOrigin {
    Code,
    Link { link: AxiLink, etag: Option<String>, fetched_at_unix: u64 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoadedComp {
    pub comp: Comp,
    /// `code:<hash>` or `link:<fileId>`; keys remembered picks.
    pub key: String,
    /// What the user pasted, so a restart can tell the cache is still current.
    pub input: String,
    pub origin: CompOrigin,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Assignment {
    Unassigned,
    Auto(SlotRef),
    Manual(SlotRef),
    NoMatch,
    Ambiguous(Vec<SlotRef>),
}

impl Assignment {
    pub fn slot(&self) -> Option<SlotRef> {
        match self {
            Assignment::Auto(r) | Assignment::Manual(r) => Some(*r),
            _ => None,
        }
    }
}

pub fn pick_key(comp_key: &str, character: &str) -> String {
    format!("{comp_key}|{character}")
}

/// "Party 2 · Quickbrand", or "Party 1 · DPS · Berserker" for a tag slot.
pub fn slot_label(comp: &Comp, r: SlotRef, specs: &SpecDb) -> String {
    let build = &comp.builds[r.build];
    let name = build.title.clone().unwrap_or_else(|| build.spec_label(specs));
    match comp.lines.get(r.line).and_then(|l| l.slots.get(r.slot)) {
        Some(SlotEntry::Tag { name: tag, .. }) => format!("Party {} · {tag} · {name}", r.line + 1),
        _ => format!("Party {} · {name}", r.line + 1),
    }
}

pub struct Session {
    pub comp: Option<LoadedComp>,
    pub assignment: Assignment,
    pub assigned_at: Option<Instant>,
    pub live: LiveState,
    pub api: ApiState,
    pub db: GameDb,
}

impl Session {
    pub fn new(db: GameDb) -> Session {
        Session { comp: None, assignment: Assignment::Unassigned, assigned_at: None, live: LiveState::default(), api: ApiState::default(), db }
    }

    pub fn set_comp(&mut self, comp: Option<LoadedComp>, picks: &BTreeMap<String, SlotRef>, specs: &SpecDb, now: Instant) {
        self.comp = comp;
        self.rematch(picks, specs, now);
    }

    /// Remembered pick if it still exists and fits the current spec, else auto-match.
    pub fn rematch(&mut self, picks: &BTreeMap<String, SlotRef>, specs: &SpecDb, now: Instant) {
        let next = match (&self.comp, &self.live.identity) {
            (Some(lc), Some(id)) => {
                let remembered = picks
                    .get(&pick_key(&lc.key, &id.name))
                    .copied()
                    .filter(|r| lc.comp.is_valid(*r) && matcher::fits(&lc.comp.builds[r.build], id, specs));
                match remembered {
                    Some(r) => Assignment::Manual(r),
                    None => match matcher::match_slot(&lc.comp, id, specs) {
                        MatchOutcome::Auto(r) => Assignment::Auto(r),
                        MatchOutcome::NoMatch => Assignment::NoMatch,
                        MatchOutcome::Ambiguous(v) => Assignment::Ambiguous(v),
                    },
                }
            }
            _ => Assignment::Unassigned,
        };
        if next.slot() != self.assignment.slot() {
            self.assigned_at = Some(now);
        }
        self.assignment = next;
    }

    /// Manual pick; only slots whose build fits the current spec are allowed.
    pub fn pick(&mut self, slot: SlotRef, picks: &mut BTreeMap<String, SlotRef>, specs: &SpecDb, now: Instant) -> bool {
        let (Some(lc), Some(id)) = (&self.comp, &self.live.identity) else { return false };
        if !lc.comp.is_valid(slot) || !matcher::fits(&lc.comp.builds[slot.build], id, specs) {
            return false;
        }
        picks.insert(pick_key(&lc.key, &id.name), slot);
        if self.assignment.slot() != Some(slot) {
            self.assigned_at = Some(now);
        }
        self.assignment = Assignment::Manual(slot);
        true
    }

    pub fn build(&self) -> Option<&Build> {
        let lc = self.comp.as_ref()?;
        Some(&lc.comp.builds[self.assignment.slot()?.build])
    }

    pub fn report(&self, specs: &SpecDb, consumables: &Consumables, severities: &Severities, now: Instant) -> Option<CheckReport> {
        let lc = self.comp.as_ref()?;
        let slot = self.assignment.slot()?;
        let build = &lc.comp.builds[slot.build];
        let ctx = Ctx {
            build,
            live: &self.live,
            api: &self.api,
            db: &self.db,
            specs,
            consumables,
            now,
            assigned_at: self.assigned_at.unwrap_or(now),
        };
        Some(CheckReport {
            comp_key: lc.key.clone(),
            comp_name: lc.comp.name.clone(),
            slot,
            slot_label: slot_label(&lc.comp, slot, specs),
            results: checks::run(&ctx, severities),
        })
    }

    pub fn badge(&self, report: Option<&CheckReport>) -> Badge {
        match (&self.comp, report) {
            (None, _) => Badge::NoComp,
            (Some(_), None) => Badge::PickSlot,
            (Some(_), Some(r)) => Badge::Checked(r.summary()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{comp, firebrand, identity_for, necro};
    use std::time::Duration;

    fn loaded() -> LoadedComp {
        LoadedComp { comp: comp(), key: "code:test".into(), input: "<AxiForge:Comp:…>".into(), origin: CompOrigin::Code }
    }

    fn session_as(build: &Build, picks: &BTreeMap<String, SlotRef>, now: Instant) -> Session {
        let mut s = Session::new(GameDb::default());
        s.live.set_identity(identity_for(build), now);
        s.set_comp(Some(loaded()), picks, SpecDb::bundled(), now);
        s
    }

    fn slot(line: usize, slot: usize, build: usize) -> SlotRef {
        SlotRef { line, slot, build }
    }

    #[test]
    fn auto_assigns_and_reports() {
        let now = Instant::now();
        let s = session_as(&firebrand(), &BTreeMap::new(), now);
        assert_eq!(s.assignment, Assignment::Auto(slot(0, 0, 0)));
        let report = s.report(SpecDb::bundled(), Consumables::bundled(), &Severities::default(), now).unwrap();
        assert_eq!(report.slot_label, "Party 1 · Firebrand");
        assert_eq!(report.comp_name, "Tuesday Zerg");
        assert!(matches!(s.badge(Some(&report)), Badge::Checked(_)));
    }

    #[test]
    fn tag_slots_are_labelled_with_the_tag() {
        let s = session_as(&necro(), &BTreeMap::new(), Instant::now());
        assert_eq!(slot_label(&s.comp.as_ref().unwrap().comp, s.assignment.slot().unwrap(), SpecDb::bundled()), "Party 1 · DPS · Necromancer (core)");
    }

    #[test]
    fn picks_are_remembered_per_comp_and_character() {
        let now = Instant::now();
        let mut picks = BTreeMap::new();
        let mut s = session_as(&necro(), &picks, now);
        assert!(s.pick(slot(1, 1, 2), &mut picks, SpecDb::bundled(), now));
        assert_eq!(s.assignment, Assignment::Manual(slot(1, 1, 2)));
        assert_eq!(picks.get("code:test|Tester"), Some(&slot(1, 1, 2)));

        let restored = session_as(&necro(), &picks, now);
        assert_eq!(restored.assignment, Assignment::Manual(slot(1, 1, 2)));
    }

    #[test]
    fn picks_that_dont_fit_are_refused_or_ignored() {
        let now = Instant::now();
        let mut picks = BTreeMap::new();
        let mut s = session_as(&firebrand(), &picks, now);
        assert!(!s.pick(slot(0, 1, 1), &mut picks, SpecDb::bundled(), now), "berserker slot for a firebrand");
        assert!(picks.is_empty());

        picks.insert("code:test|Tester".into(), slot(9, 0, 0)); // comp was republished smaller
        assert_eq!(session_as(&firebrand(), &picks, now).assignment, Assignment::Auto(slot(0, 0, 0)));
    }

    #[test]
    fn respec_drops_a_pick_that_no_longer_fits() {
        let now = Instant::now();
        let mut picks = BTreeMap::new();
        let mut s = session_as(&firebrand(), &picks, now);
        s.pick(slot(1, 0, 0), &mut picks, SpecDb::bundled(), now);
        s.live.identity.as_mut().unwrap().spec = 27; // Dragonhunter
        s.rematch(&picks, SpecDb::bundled(), now);
        assert_eq!(s.assignment, Assignment::NoMatch);
        assert!(s.report(SpecDb::bundled(), Consumables::bundled(), &Severities::default(), now).is_none());
    }

    #[test]
    fn assigned_at_moves_only_when_the_slot_does() {
        let t0 = Instant::now();
        let mut s = session_as(&firebrand(), &BTreeMap::new(), t0);
        assert_eq!(s.assigned_at, Some(t0));
        s.rematch(&BTreeMap::new(), SpecDb::bundled(), t0 + Duration::from_secs(5));
        assert_eq!(s.assigned_at, Some(t0));
    }

    #[test]
    fn badge_without_comp_or_slot() {
        let s = Session::new(GameDb::default());
        assert_eq!(s.badge(None), Badge::NoComp);
        let mut s = Session::new(GameDb::default());
        s.set_comp(Some(loaded()), &BTreeMap::new(), SpecDb::bundled(), Instant::now());
        assert_eq!(s.assignment, Assignment::Unassigned, "no MumbleLink identity yet");
        assert_eq!(s.badge(None), Badge::PickSlot);
    }
}
