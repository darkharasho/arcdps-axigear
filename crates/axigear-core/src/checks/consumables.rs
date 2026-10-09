//! Food and utility from live buffs. Never ✗ without evidence: absence only
//! counts once arcdps has shown us the full buff state (or the removal).

use std::time::Duration;

use crate::checks::Ctx;
use crate::consumables::ConsumableKind;
use crate::report::{Category, CheckResult, SlotKey, Status};

pub const GRACE: Duration = Duration::from_secs(30);

pub fn food(ctx: &Ctx) -> Vec<CheckResult> {
    check(ctx, ConsumableKind::Food, ctx.build.equipment.food.as_deref())
}

pub fn utility(ctx: &Ctx) -> Vec<CheckResult> {
    check(ctx, ConsumableKind::Utility, ctx.build.equipment.utility.as_deref())
}

fn check(ctx: &Ctx, kind: ConsumableKind, want: Option<&str>) -> Vec<CheckResult> {
    let Some(want) = want else { return Vec::new() };
    let resolved;
    let want = match crate::model::consumable_item_id(want) {
        None => want,
        Some(id) => match ctx.db.item_name(id) {
            Some(name) => {
                resolved = name.to_string();
                resolved.as_str()
            }
            None => {
                let (cat, id_s, label, key) = match kind {
                    ConsumableKind::Food => (Category::Food, "food", "Food", SlotKey::Food),
                    ConsumableKind::Utility => (Category::Utility, "utility", "Utility", SlotKey::Utility),
                };
                return vec![CheckResult::new(cat, id_s, label, Status::Unknown, format!("Item {id}")).with_reason("looking up item name").mark(key, Status::Unknown, None)];
            }
        },
    };
    let (cat, id, label, key) = match kind {
        ConsumableKind::Food => (Category::Food, "food", "Food", SlotKey::Food),
        ConsumableKind::Utility => (Category::Utility, "utility", "Utility", SlotKey::Utility),
    };
    let row = |status| CheckResult::new(cat, id, label, status, want);

    let since = ctx.live.map_loaded_at.map_or(ctx.assigned_at, |m| m.max(ctx.assigned_at));
    if ctx.now.saturating_duration_since(since) < GRACE {
        return vec![row(Status::Unknown).with_reason("checking shortly after load")];
    }
    let active = ctx.live.active_of(kind, ctx.consumables);
    if active.iter().any(|b| ctx.consumables.matches(*b, want)) {
        return vec![row(Status::Pass).with_actual(want).mark(key, Status::Pass, None)];
    }
    if let Some(other) = active.first() {
        let name = ctx.consumables.name_of(*other).unwrap_or("another one");
        return vec![row(Status::Fail).with_actual(name).force_advisory().mark(key, Status::Fail, Some(name.into()))];
    }
    if ctx.live.knows_absence(kind) {
        return vec![row(Status::Fail).with_actual("none").mark(key, Status::Fail, Some("none".into()))];
    }
    vec![row(Status::Unknown).with_reason("no buff data yet - updates when arcdps reports buffs").mark(key, Status::Unknown, None)]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::consumables::Consumables;
    use crate::report::{Severity, Status};
    use crate::testutil::{firebrand, necro, World};

    #[test]
    fn numeric_food_passes_once_its_name_is_known() {
        let label = firebrand().equipment.food.clone().unwrap();
        let mut w = World::matching(firebrand());
        w.build.equipment.food = Some("91835".into());
        assert_eq!(w.result("food").status, Status::Unknown);
        assert_eq!(w.result("food").reason.as_deref(), Some("looking up item name"));
        assert!(w.result("food").marks.iter().any(|m| m.key == SlotKey::Food && m.status == Status::Unknown));
        w.db.items.insert(91835, crate::gamedb::ItemInfo { name: label, ..Default::default() });
        assert_eq!(w.result("food").status, Status::Pass);
    }

    #[test]
    fn numeric_food_matches_mists_infused_variant() {
        let mut w = World::matching(firebrand());
        w.build.equipment.food = Some("91835".into());
        w.db.items.insert(91835, crate::gamedb::ItemInfo { name: "Peppercorn-Crusted Sous-Vide Steak".into(), ..Default::default() });
        w.live.active.clear();
        w.live.active.insert(Consumables::bundled().find("Mists-Infused Peppercorn-Crusted Sous-Vide Steak").unwrap());
        assert_eq!(w.result("food").status, Status::Pass);
    }

    fn rendang() -> u32 {
        Consumables::bundled().find("Plate of Beef Rendang").unwrap()
    }

    #[test]
    fn expected_food_and_utility_pass() {
        let w = World::matching(firebrand()); // Truffle Steak Dinner (alias) + Bountiful Maintenance Oil
        assert_eq!(w.result("food").status, Status::Pass);
        assert_eq!(w.result("utility").status, Status::Pass);
    }

    #[test]
    fn wrong_food_is_always_advisory() {
        let mut w = World::matching(firebrand());
        w.live.active.clear();
        w.live.active.insert(rendang());
        let r = w.result("food");
        assert_eq!((r.status, r.severity), (Status::Fail, Severity::Advisory));
        assert_eq!(r.actual.as_deref(), Some("Plate of Beef Rendang"));
    }

    #[test]
    fn missing_food_after_a_baseline_fails_at_category_severity() {
        let mut w = World::matching(firebrand());
        w.live.active.clear();
        let r = w.result("food");
        assert_eq!((r.status, r.severity, r.actual.as_deref()), (Status::Fail, Severity::Required, Some("none")));
    }

    #[test]
    fn missing_food_without_a_baseline_is_unknown() {
        let mut w = World::matching(firebrand());
        w.live.active.clear();
        w.live.baseline = false;
        assert_eq!(w.result("food").status, Status::Unknown);
        w.live.removed.insert(ConsumableKind::Food);
        assert_eq!(w.result("food").status, Status::Fail);
    }

    #[test]
    fn grace_after_map_load_or_assignment() {
        let mut w = World::matching(firebrand());
        w.live.active.clear();
        w.live.map_loaded_at = Some(w.now - Duration::from_secs(10));
        assert_eq!(w.result("food").status, Status::Unknown);

        let mut w = World::matching(firebrand());
        w.live.active.clear();
        w.assigned_at = w.now - Duration::from_secs(29);
        assert_eq!(w.result("food").status, Status::Unknown);
        w.assigned_at = w.now - Duration::from_secs(31);
        assert_eq!(w.result("food").status, Status::Fail);
    }

    #[test]
    fn mists_infused_food_counts() {
        let mut b = firebrand();
        b.equipment.food = Some("Peppercorn-Crusted Sous-Vide Steak".into());
        let mut w = World::matching(b);
        w.live.active.clear();
        w.live.active.insert(Consumables::bundled().find("Mists-Infused Peppercorn-Crusted Sous-Vide Steak").unwrap());
        assert_eq!(w.result("food").status, Status::Pass);
    }

    #[test]
    fn unspecified_food_has_no_row() {
        let w = World::matching(necro());
        assert!(!w.has("food") && !w.has("utility"));
    }

    #[test]
    fn food_and_utility_are_marked() {
        use crate::report::SlotKey;
        let w = World::matching(firebrand());
        assert!(w.result("food").marks.iter().any(|m| m.key == SlotKey::Food && m.status == Status::Pass));
        let mut w = World::matching(firebrand());
        w.live.active.clear();
        let r = w.result("utility");
        assert!(r.marks.iter().any(|m| m.key == SlotKey::Utility && m.status == Status::Fail && m.detail.as_deref() == Some("none")));
    }
}
