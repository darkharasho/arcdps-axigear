//! What arcdps and MumbleLink tell us right now. The plugin translates arcdps
//! events into `LiveEvent`s; this reducer is the only state they touch.

use std::collections::BTreeSet;
use std::time::Instant;

use crate::consumables::{ConsumableKind, Consumables};
use crate::mumble::Identity;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LiveEvent {
    /// A buff landed on self. `initial` = arcdps's BuffInitial statechange
    /// (buff already present when arcdps started tracking).
    BuffApply { id: u32, initial: bool },
    /// All stacks of a buff left self.
    BuffRemove { id: u32 },
    SkillCast { id: u32 },
    WeaponSwap { set: u8 },
    Combat { active: bool },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdentityChange {
    None,
    First,
    Character,
    Spec,
    Map,
}

#[derive(Debug, Default)]
pub struct LiveState {
    pub identity: Option<Identity>,
    /// Consumable buff IDs currently on self.
    pub active: BTreeSet<u32>,
    /// arcdps reported initial buff state for self, so absence is meaningful.
    pub baseline: bool,
    /// Kinds whose removal we saw, so their absence is meaningful too.
    pub removed: BTreeSet<ConsumableKind>,
    /// Skills self cast since the last map load.
    pub skills_cast: BTreeSet<u32>,
    pub weapon_set: Option<u8>,
    pub arc_combat: bool,
    pub map_loaded_at: Option<Instant>,
}

impl LiveState {
    pub fn apply(&mut self, ev: &LiveEvent, cons: &Consumables) {
        match *ev {
            LiveEvent::BuffApply { id, initial } => {
                if initial {
                    self.baseline = true;
                }
                if let Some(kind) = cons.kind_of(id) {
                    // One food and one utility at a time: a new one replaces the old.
                    self.active.retain(|a| cons.kind_of(*a) != Some(kind));
                    self.active.insert(id);
                }
            }
            LiveEvent::BuffRemove { id } => {
                if let Some(kind) = cons.kind_of(id) {
                    self.active.remove(&id);
                    self.removed.insert(kind);
                }
            }
            LiveEvent::SkillCast { id } => {
                self.skills_cast.insert(id);
            }
            LiveEvent::WeaponSwap { set } => self.weapon_set = Some(set),
            LiveEvent::Combat { active } => self.arc_combat = active,
        }
    }

    pub fn set_identity(&mut self, id: Identity, now: Instant) -> IdentityChange {
        let change = match &self.identity {
            None => IdentityChange::First,
            Some(old) if old.name != id.name => IdentityChange::Character,
            Some(old) if old.map_id != id.map_id => IdentityChange::Map,
            Some(old) if old.spec != id.spec || old.profession != id.profession => IdentityChange::Spec,
            Some(_) => IdentityChange::None,
        };
        match change {
            IdentityChange::Character => {
                self.active.clear();
                self.baseline = false;
                self.removed.clear();
                self.skills_cast.clear();
                self.weapon_set = None;
                self.map_loaded_at = Some(now);
            }
            IdentityChange::First => self.map_loaded_at = Some(now),
            IdentityChange::Map => {
                self.skills_cast.clear();
                self.map_loaded_at = Some(now);
            }
            IdentityChange::Spec | IdentityChange::None => {}
        }
        self.identity = Some(id);
        change
    }

    pub fn in_combat(&self) -> bool {
        self.arc_combat || self.identity.as_ref().is_some_and(|i| i.in_combat)
    }

    pub fn active_of(&self, kind: ConsumableKind, cons: &Consumables) -> Vec<u32> {
        self.active.iter().copied().filter(|id| cons.kind_of(*id) == Some(kind)).collect()
    }

    pub fn knows_absence(&self, kind: ConsumableKind) -> bool {
        self.baseline || self.removed.contains(&kind)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn ids() -> (u32, u32, u32) {
        let c = Consumables::bundled();
        (
            c.find("Plate of Truffle Steak").unwrap(),
            c.find("Plate of Beef Rendang").unwrap(),
            c.find("Superior Sharpening Stone").unwrap(),
        )
    }

    fn who(name: &str, spec: u16, map: u32) -> Identity {
        Identity { name: name.into(), profession: 1, spec, map_id: map, map_type: 9, in_combat: false }
    }

    #[test]
    fn tracks_one_food_at_a_time() {
        let c = Consumables::bundled();
        let (truffle, rendang, stone) = ids();
        let mut s = LiveState::default();
        s.apply(&LiveEvent::BuffApply { id: truffle, initial: false }, c);
        s.apply(&LiveEvent::BuffApply { id: stone, initial: false }, c);
        s.apply(&LiveEvent::BuffApply { id: rendang, initial: false }, c);
        assert_eq!(s.active_of(ConsumableKind::Food, c), vec![rendang]);
        assert_eq!(s.active_of(ConsumableKind::Utility, c), vec![stone]);
        s.apply(&LiveEvent::BuffApply { id: 717, initial: false }, c); // a boon: ignored
        assert_eq!(s.active.len(), 2);
    }

    #[test]
    fn absence_is_known_only_after_baseline_or_removal() {
        let c = Consumables::bundled();
        let (truffle, _, _) = ids();
        let mut s = LiveState::default();
        assert!(!s.knows_absence(ConsumableKind::Food));
        s.apply(&LiveEvent::BuffApply { id: truffle, initial: false }, c);
        s.apply(&LiveEvent::BuffRemove { id: truffle }, c);
        assert!(s.knows_absence(ConsumableKind::Food));
        assert!(!s.knows_absence(ConsumableKind::Utility));
        s.apply(&LiveEvent::BuffApply { id: 717, initial: true }, c);
        assert!(s.knows_absence(ConsumableKind::Utility));
    }

    #[test]
    fn map_change_clears_casts_but_keeps_buffs() {
        let c = Consumables::bundled();
        let (truffle, _, _) = ids();
        let t0 = Instant::now();
        let mut s = LiveState::default();
        assert_eq!(s.set_identity(who("A", 62, 1), t0), IdentityChange::First);
        s.apply(&LiveEvent::BuffApply { id: truffle, initial: true }, c);
        s.apply(&LiveEvent::SkillCast { id: 9153 }, c);
        assert_eq!(s.set_identity(who("A", 62, 1), t0), IdentityChange::None);
        let t1 = t0 + Duration::from_secs(5);
        assert_eq!(s.set_identity(who("A", 62, 2), t1), IdentityChange::Map);
        assert!(s.skills_cast.is_empty());
        assert!(s.active.contains(&truffle) && s.baseline);
        assert_eq!(s.map_loaded_at, Some(t1));
    }

    #[test]
    fn character_change_resets_everything() {
        let c = Consumables::bundled();
        let (truffle, _, _) = ids();
        let t0 = Instant::now();
        let mut s = LiveState::default();
        s.set_identity(who("A", 62, 1), t0);
        s.apply(&LiveEvent::BuffApply { id: truffle, initial: true }, c);
        assert_eq!(s.set_identity(who("B", 62, 1), t0), IdentityChange::Character);
        assert!(s.active.is_empty() && !s.baseline && s.removed.is_empty());
        assert_eq!(s.set_identity(who("B", 27, 1), t0), IdentityChange::Spec);
    }

    #[test]
    fn early_events_survive_the_first_identity() {
        let c = Consumables::bundled();
        let (truffle, _, _) = ids();
        let mut s = LiveState::default();
        s.apply(&LiveEvent::BuffApply { id: truffle, initial: true }, c);
        s.set_identity(who("A", 62, 1), Instant::now());
        assert!(s.active.contains(&truffle) && s.baseline);
    }

    #[test]
    fn combat_from_either_source() {
        let c = Consumables::bundled();
        let mut s = LiveState::default();
        assert!(!s.in_combat());
        s.apply(&LiveEvent::Combat { active: true }, c);
        assert!(s.in_combat());
        s.apply(&LiveEvent::Combat { active: false }, c);
        s.set_identity(Identity { in_combat: true, ..who("A", 62, 1) }, Instant::now());
        assert!(s.in_combat());
    }
}
