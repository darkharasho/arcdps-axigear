//! Food and utility from live buffs. Never ✗ without evidence: absence only
//! counts once arcdps has shown us the full buff state (or the removal).

use std::time::Duration;

use crate::checks::Ctx;
use crate::consumables::ConsumableKind;
use crate::report::{Category, CheckResult, Status};

pub const GRACE: Duration = Duration::from_secs(30);

pub fn food(ctx: &Ctx) -> Vec<CheckResult> {
    check(ctx, ConsumableKind::Food, ctx.build.equipment.food.as_deref())
}

pub fn utility(ctx: &Ctx) -> Vec<CheckResult> {
    check(ctx, ConsumableKind::Utility, ctx.build.equipment.utility.as_deref())
}

fn check(ctx: &Ctx, kind: ConsumableKind, want: Option<&str>) -> Vec<CheckResult> {
    let Some(want) = want else { return Vec::new() };
    let (cat, id, label) = match kind {
        ConsumableKind::Food => (Category::Food, "food", "Food"),
        ConsumableKind::Utility => (Category::Utility, "utility", "Utility"),
    };
    let row = |status| CheckResult::new(cat, id, label, status, want);

    let since = ctx.live.map_loaded_at.map_or(ctx.assigned_at, |m| m.max(ctx.assigned_at));
    if ctx.now.saturating_duration_since(since) < GRACE {
        return vec![row(Status::Unknown).with_reason("checking shortly after load")];
    }
    let active = ctx.live.active_of(kind, ctx.consumables);
    if active.iter().any(|b| ctx.consumables.matches(*b, want)) {
        return vec![row(Status::Pass).with_actual(want)];
    }
    if let Some(other) = active.first() {
        let name = ctx.consumables.name_of(*other).unwrap_or("another one");
        return vec![row(Status::Fail).with_actual(name).force_advisory()];
    }
    if ctx.live.knows_absence(kind) {
        return vec![row(Status::Fail).with_actual("none")];
    }
    vec![row(Status::Unknown).with_reason("no buff data yet - updates when arcdps reports buffs")]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::consumables::Consumables;
    use crate::report::{Severity, Status};
    use crate::testutil::{firebrand, necro, World};

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
}
