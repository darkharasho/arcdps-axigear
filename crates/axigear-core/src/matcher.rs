//! Which slot is mine? Elite builds match that elite spec; core builds match
//! the profession with no elite. Several slots holding the same build count
//! as one match.

use std::collections::BTreeSet;

use crate::model::{Build, Comp, SlotRef};
use crate::mumble::{profession_name, Identity};
use crate::specs::SpecDb;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MatchOutcome {
    Auto(SlotRef),
    NoMatch,
    Ambiguous(Vec<SlotRef>),
}

/// MumbleLink's `spec` is the third line; it is the elite only if it is one.
pub fn live_elite(identity: &Identity, specs: &SpecDb) -> Option<u16> {
    (identity.spec != 0 && specs.is_elite(identity.spec)).then_some(identity.spec)
}

pub fn fits(build: &Build, identity: &Identity, specs: &SpecDb) -> bool {
    profession_name(identity.profession).is_some_and(|p| build.profession.eq_ignore_ascii_case(p))
        && build.elite_spec(specs) == live_elite(identity, specs)
}

pub fn match_slot(comp: &Comp, identity: &Identity, specs: &SpecDb) -> MatchOutcome {
    let fitting: Vec<SlotRef> = comp
        .candidates()
        .into_iter()
        .filter(|r| fits(&comp.builds[r.build], identity, specs))
        .collect();
    let distinct: BTreeSet<usize> = fitting.iter().map(|r| r.build).collect();
    match (fitting.first(), distinct.len()) {
        (None, _) => MatchOutcome::NoMatch,
        (Some(first), 1) => MatchOutcome::Auto(*first),
        _ => MatchOutcome::Ambiguous(fitting),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{PartyLine, SlotEntry};
    use crate::testutil::comp;

    fn who(profession: u8, spec: u16) -> Identity {
        Identity { name: "T".into(), profession, spec, map_id: 1, map_type: 9, in_combat: false }
    }

    fn slot(line: usize, slot: usize, build: usize) -> SlotRef {
        SlotRef { line, slot, build }
    }

    #[test]
    fn same_build_in_two_slots_auto_assigns() {
        // Firebrand sits in party 1 slot 1 and party 2 slot 1.
        assert_eq!(match_slot(&comp(), &who(1, 62), SpecDb::bundled()), MatchOutcome::Auto(slot(0, 0, 0)));
    }

    #[test]
    fn a_build_reachable_directly_and_through_a_tag_auto_assigns() {
        // Berserker: party 1 slot 2, and the DPS tag in party 1 slot 3.
        assert_eq!(match_slot(&comp(), &who(2, 18), SpecDb::bundled()), MatchOutcome::Auto(slot(0, 1, 1)));
    }

    #[test]
    fn core_builds_match_core_characters() {
        // Core necro: spec 53 (Spite) in line 3, which is not an elite.
        assert_eq!(match_slot(&comp(), &who(8, 53), SpecDb::bundled()), MatchOutcome::Auto(slot(0, 2, 2)));
    }

    #[test]
    fn wrong_elite_or_profession_is_no_match() {
        let db = SpecDb::bundled();
        assert_eq!(match_slot(&comp(), &who(1, 27), db), MatchOutcome::NoMatch); // Dragonhunter
        assert_eq!(match_slot(&comp(), &who(7, 40), db), MatchOutcome::NoMatch); // Chronomancer
        assert_eq!(match_slot(&comp(), &who(0, 0), db), MatchOutcome::NoMatch);
    }

    #[test]
    fn two_different_builds_for_one_spec_are_ambiguous() {
        let mut c = comp();
        let mut alt = c.builds[0].clone();
        alt.title = Some("Heal Firebrand".into());
        c.builds.push(alt);
        c.lines.push(PartyLine { capacity: 5, slots: vec![SlotEntry::Build(3)] });
        match match_slot(&c, &who(1, 62), SpecDb::bundled()) {
            MatchOutcome::Ambiguous(slots) => {
                assert_eq!(slots, vec![slot(0, 0, 0), slot(1, 0, 0), slot(2, 0, 3)]);
            }
            other => panic!("{other:?}"),
        }
    }
}
