//! Weapons, stats, runes, sigils, relic, infusions — all from the API snapshot.

use std::collections::BTreeSet;

use crate::checks::Ctx;
use crate::gamedb::GameDb;
use crate::gw2api::ApiSnapshot;
use crate::model::{is_two_handed, GearSlot};
use crate::report::{Category, CheckResult, Status};
use crate::text::{norm, summarize};

/// "Legendary Rune of the Scholar" and "Superior Rune of the Scholar" are the same upgrade.
pub fn upgrade_key(name: &str) -> String {
    let mut n = name.trim();
    for prefix in ["Superior ", "Legendary ", "Major ", "Minor "] {
        if let Some(rest) = n.strip_prefix(prefix) {
            n = rest;
            break;
        }
    }
    norm(n)
}

/// `None` when either name isn't resolved yet.
pub fn same_upgrade(db: &GameDb, want: u32, have: u32) -> Option<bool> {
    if want == have {
        return Some(true);
    }
    Some(upgrade_key(db.item_name(want)?) == upgrade_key(db.item_name(have)?))
}

fn item_label(ctx: &Ctx, id: u32) -> String {
    ctx.db.item_name(id).map(String::from).unwrap_or_else(|| format!("item {id}"))
}

/// Fail if anything is definitely wrong, else Unknown if something is unresolved, else Pass.
fn finish(cat: Category, id: impl Into<String>, label: impl Into<String>, expected: String, wrong: Vec<String>, unresolved: Option<u32>) -> CheckResult {
    if !wrong.is_empty() {
        CheckResult::new(cat, id, label, Status::Fail, expected).with_actual(wrong.join(", "))
    } else if let Some(item) = unresolved {
        CheckResult::new(cat, id, label, Status::Unknown, expected).with_reason(format!("couldn't resolve item {item}"))
    } else {
        CheckResult::new(cat, id, label, Status::Pass, expected.clone()).with_actual(expected)
    }
}

pub fn weapons(ctx: &Ctx, snap: &ApiSnapshot) -> Vec<CheckResult> {
    let w = &ctx.build.equipment.weapons;
    let mut out = Vec::new();
    for (set, main_slot, off_slot) in [("A", GearSlot::WeaponA1, GearSlot::WeaponA2), ("B", GearSlot::WeaponB1, GearSlot::WeaponB2)] {
        let (main, off) = (w.get(main_slot), w.get(off_slot));
        if main.is_none() && off.is_none() {
            continue; // the build doesn't define this set
        }
        let two_handed = main.is_some_and(is_two_handed);
        let off = if two_handed { None } else { off };
        let expected = match (main, off) {
            (Some(m), Some(o)) => format!("{m} + {o}"),
            (Some(m), None) => m.to_string(),
            (None, Some(o)) => format!("any + {o}"),
            (None, None) => unreachable!(),
        };
        let (mut wrong, mut unresolved, mut actual) = (false, None, Vec::new());
        for (slot, want) in [(main_slot, main), (off_slot, off)] {
            let Some(want) = want else { continue };
            match snap.item(slot) {
                None => {
                    wrong = true;
                    actual.push("empty".to_string());
                }
                Some(item) => match ctx.db.weapon_type(item.id) {
                    None => {
                        unresolved = unresolved.or(Some(item.id));
                        actual.push("?".to_string());
                    }
                    Some(have) => {
                        wrong |= norm(have) != norm(want);
                        actual.push(have.to_lowercase());
                    }
                },
            }
        }
        let status = if wrong { Status::Fail } else if unresolved.is_some() { Status::Unknown } else { Status::Pass };
        let mut r = CheckResult::new(Category::Weapons, format!("weapons.{set}"), format!("Weapons {set}"), status, expected)
            .with_actual(actual.join(" + "));
        if let (Status::Unknown, Some(id)) = (status, unresolved) {
            r = r.with_reason(format!("couldn't resolve item {id}"));
        }
        out.push(r);
    }
    out
}

pub fn weapon_evidence(ctx: &Ctx) -> Option<String> {
    let seen: BTreeSet<String> = ctx
        .live
        .skills_cast
        .iter()
        .filter_map(|id| ctx.db.skills.get(id)?.weapon_type.as_ref())
        .map(|w| w.to_lowercase())
        .collect();
    (!seen.is_empty()).then(|| format!("seen in use: {}", seen.into_iter().collect::<Vec<_>>().join(", ")))
}

pub fn stats(ctx: &Ctx, snap: &ApiSnapshot) -> Vec<CheckResult> {
    let want = &ctx.build.equipment.stats;
    if want.is_empty() {
        return Vec::new();
    }
    let (mut wrong, mut unresolved) = (Vec::new(), None);
    for (slot, stat) in want {
        match snap.item(*slot) {
            None => wrong.push(format!("{}: empty", slot.label())),
            Some(item) => match ctx.db.stat_name(item) {
                None => unresolved = unresolved.or(Some(item.id)),
                Some(have) if norm(have) != norm(stat) => wrong.push(format!("{}: {have}", slot.label())),
                Some(_) => {}
            },
        }
    }
    vec![finish(Category::Stats, "stats", "Stats", summarize(want.values().cloned()), wrong, unresolved)]
}

pub fn runes(ctx: &Ctx, snap: &ApiSnapshot) -> Vec<CheckResult> {
    let want = &ctx.build.equipment.runes;
    if want.is_empty() {
        return Vec::new();
    }
    let (mut wrong, mut unresolved, mut good) = (Vec::new(), None, 0);
    for (slot, rune) in want {
        match snap.item(*slot).and_then(|i| i.upgrades.first().copied()) {
            None => wrong.push(format!("{}: none", slot.label())),
            Some(have) => match same_upgrade(ctx.db, *rune, have) {
                Some(true) => good += 1,
                Some(false) => wrong.push(format!("{}: {}", slot.label(), item_label(ctx, have))),
                None => unresolved = unresolved.or(Some(have)),
            },
        }
    }
    let expected = summarize(want.values().map(|id| item_label(ctx, *id)));
    let failed = !wrong.is_empty();
    let detail = wrong.join(", ");
    let mut r = finish(Category::Runes, "runes", "Runes", expected, wrong, unresolved);
    if failed {
        r.actual = Some(format!("{good}/{} · {detail}", want.len()));
    }
    vec![r]
}

pub fn sigils(ctx: &Ctx, snap: &ApiSnapshot) -> Vec<CheckResult> {
    let s = &ctx.build.equipment.sigils;
    let mut out = Vec::new();
    for (set, slots) in [("A", [GearSlot::WeaponA1, GearSlot::WeaponA2]), ("B", [GearSlot::WeaponB1, GearSlot::WeaponB2])] {
        let want: Vec<u32> = slots.iter().flat_map(|sl| s.get(*sl).iter().copied()).collect();
        if want.is_empty() {
            continue;
        }
        let have: Vec<u32> = slots.iter().filter_map(|sl| snap.item(*sl)).flat_map(|i| i.upgrades.iter().copied()).collect();
        let mut pool = have.clone();
        let (mut missing, mut unresolved) = (Vec::new(), None);
        for w in &want {
            match pool.iter().position(|h| same_upgrade(ctx.db, *w, *h) == Some(true)) {
                Some(i) => {
                    pool.swap_remove(i);
                }
                None if pool.iter().any(|h| same_upgrade(ctx.db, *w, *h).is_none()) => unresolved = unresolved.or(Some(*w)),
                None => missing.push(item_label(ctx, *w)),
            }
        }
        let expected = summarize(want.iter().map(|id| item_label(ctx, *id)));
        let actual = summarize(have.iter().map(|id| item_label(ctx, *id)));
        let wrong = if missing.is_empty() { Vec::new() } else { vec![format!("{actual}; missing {}", missing.join(", "))] };
        out.push(finish(Category::Sigils, format!("sigils.{set}"), format!("Sigils {set}"), expected, wrong, unresolved));
    }
    out
}

pub fn relic(ctx: &Ctx, snap: &ApiSnapshot) -> Vec<CheckResult> {
    let Some(want) = &ctx.build.equipment.relic else { return Vec::new() };
    let row = |status| CheckResult::new(Category::Relic, "relic", "Relic", status, want.clone());
    let Some(item) = snap.relic() else {
        return vec![row(Status::Unknown).with_reason("the API doesn't report the relic")];
    };
    match ctx.db.item_name(item.id) {
        None => vec![row(Status::Unknown).with_reason(format!("couldn't resolve item {}", item.id))],
        Some(have) => vec![row(if norm(have) == norm(want) { Status::Pass } else { Status::Fail }).with_actual(have)],
    }
}

pub fn infusions(ctx: &Ctx, snap: &ApiSnapshot) -> Vec<CheckResult> {
    let want = &ctx.build.equipment.infusions;
    if want.is_empty() {
        return Vec::new();
    }
    let have: Vec<u32> = snap
        .equipment
        .iter()
        .filter(|i| GearSlot::from_api_slot(&i.slot).is_some())
        .flat_map(|i| i.infusions.iter().copied())
        .collect();
    let key = |id: &u32| ctx.db.item_name(*id).map(norm).unwrap_or_else(|| format!("#{id}"));
    let (mut want_keys, mut have_keys): (Vec<String>, Vec<String>) = (want.iter().map(key).collect(), have.iter().map(key).collect());
    want_keys.sort();
    have_keys.sort();
    let expected = summarize(want.iter().map(|id| item_label(ctx, *id)));
    let actual = summarize(have.iter().map(|id| item_label(ctx, *id)));
    let row = |status| CheckResult::new(Category::Infusions, "infusions", "Infusions", status, expected.clone()).with_actual(actual.clone());
    if want_keys == have_keys {
        return vec![row(Status::Pass)];
    }
    match want.iter().chain(&have).find(|id| ctx.db.item_name(**id).is_none()) {
        Some(id) => vec![row(Status::Unknown).with_reason(format!("couldn't resolve item {id}"))],
        None => vec![row(Status::Fail)],
    }
}

#[cfg(test)]
mod tests {
    use crate::gamedb::{ItemInfo, SkillInfo};
    use crate::model::GearSlot;
    use crate::report::{Severity, Status};
    use crate::testutil::{berserker, firebrand, item_id, World, RELIC_ITEM};

    fn rename(w: &mut World, id: u32, name: &str) {
        w.db.items.insert(id, ItemInfo { name: name.into(), ..Default::default() });
    }

    #[test]
    fn a_matching_firebrand_passes_every_gear_check() {
        let w = World::matching(firebrand());
        for id in ["weapons.A", "weapons.B", "stats", "runes", "sigils.A", "sigils.B", "relic", "infusions"] {
            assert_eq!(w.result(id).status, Status::Pass, "{id}: {:?}", w.result(id));
        }
        assert_eq!(w.result("weapons.A").expected, "mace + shield");
    }

    #[test]
    fn two_handers_ignore_the_offhand_slot() {
        let w = World::matching(berserker());
        let r = w.result("weapons.A");
        assert_eq!((r.status, r.expected.as_str()), (Status::Pass, "greatsword"));
    }

    #[test]
    fn wrong_or_unresolved_weapons() {
        let mut w = World::matching(firebrand());
        w.db.items.get_mut(&item_id(GearSlot::WeaponB1)).unwrap().weapon_type = Some("Hammer".into());
        let r = w.result("weapons.B");
        assert_eq!((r.status, r.actual.as_deref()), (Status::Fail, Some("hammer")));

        let mut w = World::matching(firebrand());
        w.db.items.remove(&item_id(GearSlot::WeaponA1));
        assert_eq!(w.result("weapons.A").status, Status::Unknown);
    }

    #[test]
    fn wrong_or_missing_stats_name_the_slot() {
        let mut w = World::matching(firebrand());
        w.db.itemstats.insert(2000, "Assassin's".into());
        w.item_mut(GearSlot::Ring1).stats_id = Some(2000);
        let r = w.result("stats");
        assert_eq!(r.status, Status::Fail);
        assert_eq!(r.actual.as_deref(), Some("Ring 1: Assassin's"));
        assert_eq!(r.expected, "14× Minstrel's, 1× Harrier's");

        let mut w = World::matching(firebrand());
        w.snap_mut().equipment.retain(|i| i.slot != "Ring2");
        assert!(w.result("stats").actual.unwrap().contains("Ring 2: empty"));
    }

    #[test]
    fn wrong_runes_are_counted() {
        let mut w = World::matching(firebrand());
        rename(&mut w, 99_999, "Superior Rune of Nope");
        w.item_mut(GearSlot::Legs).upgrades = vec![99_999];
        let r = w.result("runes");
        assert_eq!(r.status, Status::Fail);
        assert!(r.actual.unwrap().starts_with("5/6 · Legs: Superior Rune of Nope"));
    }

    #[test]
    fn legendary_runes_and_sigils_match_superior_ones() {
        let mut w = World::matching(firebrand());
        for slot in GearSlot::ARMOR {
            let orig = w.item_mut(slot).upgrades[0];
            rename(&mut w, 190_000 + orig, &format!("Legendary Upgrade {orig}"));
            w.item_mut(slot).upgrades = vec![190_000 + orig];
        }
        rename(&mut w, 190_000 + 24865, "Legendary Upgrade 24865");
        w.item_mut(GearSlot::WeaponA1).upgrades = vec![190_000 + 24865];
        assert_eq!(w.result("runes").status, Status::Pass);
        assert_eq!(w.result("sigils.A").status, Status::Pass);
    }

    #[test]
    fn sigils_are_a_set_per_weapon_set() {
        let mut w = World::matching(firebrand());
        // A: mace [24865] + shield [24612] → swap which weapon holds which.
        w.item_mut(GearSlot::WeaponA1).upgrades = vec![24612];
        w.item_mut(GearSlot::WeaponA2).upgrades = vec![24865];
        assert_eq!(w.result("sigils.A").status, Status::Pass);

        rename(&mut w, 24_000, "Superior Sigil of Nope");
        w.item_mut(GearSlot::WeaponB1).upgrades = vec![24865, 24_000];
        let r = w.result("sigils.B");
        assert_eq!(r.status, Status::Fail);
        assert!(r.actual.unwrap().contains("missing Superior Upgrade 24612"));
    }

    #[test]
    fn relic_missing_from_the_api_is_unknown() {
        let mut w = World::matching(firebrand());
        rename(&mut w, RELIC_ITEM, "Relic of the Monk");
        let r = w.result("relic");
        assert_eq!((r.status, r.actual.as_deref()), (Status::Fail, Some("Relic of the Monk")));

        w.snap_mut().equipment.retain(|i| i.slot != "Relic");
        assert_eq!(w.result("relic").status, Status::Unknown);
    }

    #[test]
    fn infusion_totals_are_advisory() {
        let mut w = World::matching(firebrand());
        w.item_mut(GearSlot::Head).infusions.pop();
        let r = w.result("infusions");
        assert_eq!((r.status, r.severity), (Status::Fail, Severity::Advisory));
        assert_eq!(r.expected, "19× Infusion 37133, 1× Infusion 86180");
    }

    #[test]
    fn without_api_weapons_show_live_evidence() {
        let mut w = World::matching(firebrand());
        w.api.has_key = false;
        w.db.skills.insert(9104, SkillInfo { name: "True Strike".into(), weapon_type: Some("Mace".into()) });
        w.live.skills_cast.insert(9104);
        let r = w.result("api.Weapons");
        assert_eq!(r.status, Status::Unknown);
        assert_eq!(r.actual.as_deref(), Some("seen in use: mace"));
    }
}
