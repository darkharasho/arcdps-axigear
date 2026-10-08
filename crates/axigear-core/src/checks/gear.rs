//! Weapons, stats, runes, sigils, relic, infusions — all from the API snapshot.

use std::collections::BTreeSet;

use crate::checks::Ctx;
use crate::gamedb::GameDb;
use crate::gw2api::ApiSnapshot;
use crate::model::{is_two_handed, GearSlot};
use crate::report::{Category, CheckResult, SlotKey, Status};
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

/// Stat-combo key: AxiForge and the GW2 API disagree on the possessive
/// ("Marauder's" vs "Marauder", "Demolisher" vs "Demolisher's").
/// Dropping one trailing `s` after `norm` keeps every bundled API itemstat name distinct
/// (`every_axiforge_stat_matches_exactly_one_api_itemstat`).
pub fn stat_key(name: &str) -> String {
    let mut n = norm(name);
    if n.ends_with('s') {
        n.pop();
    }
    n
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

type Marks = Vec<(SlotKey, Status, Option<String>)>;

fn with_marks(mut r: CheckResult, marks: Marks) -> CheckResult {
    for (k, s, d) in marks {
        r = r.mark(k, s, d);
    }
    r
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
        let mut marks: Marks = Vec::new();
        for (slot, want) in [(main_slot, main), (off_slot, off)] {
            let Some(want) = want else { continue };
            match snap.item(slot) {
                None => {
                    wrong = true;
                    actual.push("empty".to_string());
                    marks.push((SlotKey::Gear(slot), Status::Fail, Some("empty".into())));
                }
                Some(item) => match ctx.db.weapon_type(item.id) {
                    None => {
                        unresolved = unresolved.or(Some(item.id));
                        actual.push("?".to_string());
                        marks.push((SlotKey::Gear(slot), Status::Unknown, None));
                    }
                    Some(have) => {
                        let bad = norm(have) != norm(want);
                        wrong |= bad;
                        actual.push(have.to_lowercase());
                        marks.push(if bad { (SlotKey::Gear(slot), Status::Fail, Some(have.to_lowercase())) } else { (SlotKey::Gear(slot), Status::Pass, None) });
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
        out.push(with_marks(r, marks));
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

/// Rings and accessories are interchangeable within their pair; every other slot stands alone.
fn stat_group(slot: GearSlot) -> GearSlot {
    match slot {
        GearSlot::Ring2 => GearSlot::Ring1,
        GearSlot::Accessory2 => GearSlot::Accessory1,
        s => s,
    }
}

pub fn stats(ctx: &Ctx, snap: &ApiSnapshot) -> Vec<CheckResult> {
    let want = &ctx.build.equipment.stats;
    if want.is_empty() {
        return Vec::new();
    }
    let mut groups: Vec<(GearSlot, Vec<GearSlot>)> = Vec::new();
    for slot in want.keys() {
        match groups.iter_mut().find(|(g, _)| *g == stat_group(*slot)) {
            Some((_, slots)) => slots.push(*slot),
            None => groups.push((stat_group(*slot), vec![*slot])),
        }
    }
    let (mut wrong, mut unresolved) = (Vec::new(), None);
    let mut marks: Marks = Vec::new();
    for (_, slots) in groups {
        // Multiset match: each worn stat may satisfy any wanted stat of its group once.
        let mut pool: Vec<String> = slots.iter().map(|s| stat_key(&want[s])).collect();
        let mut leftover = Vec::new();
        for slot in &slots {
            match snap.item(*slot) {
                None => {
                    wrong.push(format!("{}: empty", slot.label()));
                    marks.push((SlotKey::Gear(*slot), Status::Fail, Some("empty".into())));
                }
                Some(item) => match ctx.db.stat_name(item) {
                    None => {
                        unresolved = unresolved.or(Some(item.id));
                        marks.push((SlotKey::Gear(*slot), Status::Unknown, None));
                    }
                    Some(have) => match pool.iter().position(|k| *k == stat_key(have)) {
                        Some(i) => {
                            pool.swap_remove(i);
                            marks.push((SlotKey::Gear(*slot), Status::Pass, None));
                        }
                        None => {
                            leftover.push(format!("{}: {have}", slot.label()));
                            marks.push((SlotKey::Gear(*slot), Status::Fail, Some(have.to_string())));
                        }
                    },
                },
            }
        }
        wrong.extend(leftover);
    }
    vec![with_marks(finish(Category::Stats, "stats", "Stats", summarize(want.values().cloned()), wrong, unresolved), marks)]
}

pub fn runes(ctx: &Ctx, snap: &ApiSnapshot) -> Vec<CheckResult> {
    let want = &ctx.build.equipment.runes;
    if want.is_empty() {
        return Vec::new();
    }
    let (mut wrong, mut unresolved, mut good) = (Vec::new(), None, 0);
    let mut marks: Marks = Vec::new();
    for (slot, rune) in want {
        match snap.item(*slot).and_then(|i| i.upgrades.first().copied()) {
            None => {
                wrong.push(format!("{}: none", slot.label()));
                marks.push((SlotKey::Rune(*slot), Status::Fail, Some("none".into())));
            }
            Some(have) => match same_upgrade(ctx.db, *rune, have) {
                Some(true) => {
                    good += 1;
                    marks.push((SlotKey::Rune(*slot), Status::Pass, None));
                }
                Some(false) => {
                    wrong.push(format!("{}: {}", slot.label(), item_label(ctx, have)));
                    marks.push((SlotKey::Rune(*slot), Status::Fail, Some(item_label(ctx, have))));
                }
                None => {
                    unresolved = unresolved.or(Some(have));
                    marks.push((SlotKey::Rune(*slot), Status::Unknown, None));
                }
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
    vec![with_marks(r, marks)]
}

pub fn sigils(ctx: &Ctx, snap: &ApiSnapshot) -> Vec<CheckResult> {
    let s = &ctx.build.equipment.sigils;
    let mut out = Vec::new();
    for (set, slots) in [("A", [GearSlot::WeaponA1, GearSlot::WeaponA2]), ("B", [GearSlot::WeaponB1, GearSlot::WeaponB2])] {
        let wanted: Vec<(GearSlot, u8, u32)> = slots.iter().flat_map(|sl| s.get(*sl).iter().enumerate().map(move |(i, id)| (*sl, i as u8, *id))).collect();
        let want: Vec<u32> = wanted.iter().map(|(_, _, id)| *id).collect();
        if want.is_empty() {
            continue;
        }
        let have: Vec<u32> = slots.iter().filter_map(|sl| snap.item(*sl)).flat_map(|i| i.upgrades.iter().copied()).collect();
        let mut pool = have.clone();
        let (mut missing, mut unresolved) = (Vec::new(), None);
        let mut marks: Marks = Vec::new();
        for (sl, idx, w) in &wanted {
            match pool.iter().position(|h| same_upgrade(ctx.db, *w, *h) == Some(true)) {
                Some(i) => {
                    pool.swap_remove(i);
                    marks.push((SlotKey::Sigil(*sl, *idx), Status::Pass, None));
                }
                None if pool.iter().any(|h| same_upgrade(ctx.db, *w, *h).is_none()) => {
                    unresolved = unresolved.or(Some(*w));
                    marks.push((SlotKey::Sigil(*sl, *idx), Status::Unknown, None));
                }
                None => {
                    missing.push(item_label(ctx, *w));
                    marks.push((SlotKey::Sigil(*sl, *idx), Status::Fail, Some("missing".into())));
                }
            }
        }
        let expected = summarize(want.iter().map(|id| item_label(ctx, *id)));
        let actual = summarize(have.iter().map(|id| item_label(ctx, *id)));
        let wrong = if missing.is_empty() { Vec::new() } else { vec![format!("{actual}; missing {}", missing.join(", "))] };
        out.push(with_marks(finish(Category::Sigils, format!("sigils.{set}"), format!("Sigils {set}"), expected, wrong, unresolved), marks));
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
        None => vec![row(Status::Unknown).with_reason(format!("couldn't resolve item {}", item.id)).mark(SlotKey::Relic, Status::Unknown, None)],
        Some(have) => {
            let st = if norm(have) == norm(want) { Status::Pass } else { Status::Fail };
            vec![row(st).with_actual(have).mark(SlotKey::Relic, st, (st == Status::Fail).then(|| have.to_string()))]
        }
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
        return vec![row(Status::Pass).mark(SlotKey::Infusions, Status::Pass, None)];
    }
    match want.iter().chain(&have).find(|id| ctx.db.item_name(**id).is_none()) {
        Some(id) => vec![row(Status::Unknown).with_reason(format!("couldn't resolve item {id}")).mark(SlotKey::Infusions, Status::Unknown, None)],
        None => vec![row(Status::Fail).mark(SlotKey::Infusions, Status::Fail, Some(actual.clone()))],
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

    fn set_stat(w: &mut World, slot: GearSlot, want: &str, have: &str) {
        w.build.equipment.stats.insert(slot, want.into());
        let id = 3000 + slot as u32;
        w.db.itemstats.insert(id, have.into());
        w.item_mut(slot).stats_id = Some(id);
    }

    #[test]
    fn every_axiforge_stat_matches_exactly_one_api_itemstat() {
        let raw = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/gw2-itemstat-names.json")).unwrap();
        let api: Vec<String> = serde_json::from_str(&raw).unwrap();
        let labels: Vec<&str> = (1..).map(crate::axicode::tables::stat).take_while(|s| !s.is_empty()).collect();
        assert_eq!(labels.len(), 40);
        for label in &labels {
            let hits: Vec<&String> = api.iter().filter(|n| super::stat_key(n) == super::stat_key(label)).collect();
            assert_eq!(hits.len(), 1, "{label:?} matches {hits:?}");
        }
        // The key must not merge two distinct stat combos on either side.
        let mut keys: Vec<String> = api.iter().map(|n| super::stat_key(n)).collect();
        keys.sort();
        keys.dedup();
        assert_eq!(keys.len(), api.len());
        let mut keys: Vec<String> = labels.iter().map(|n| super::stat_key(n)).collect();
        keys.sort();
        keys.dedup();
        assert_eq!(keys.len(), labels.len());
    }

    #[test]
    fn axiforge_and_api_possessives_match() {
        let mut w = World::matching(firebrand());
        set_stat(&mut w, GearSlot::Head, "Marauder's", "Marauder");
        set_stat(&mut w, GearSlot::Chest, "Demolisher", "Demolisher's");
        assert_eq!(w.result("stats").status, Status::Pass, "{:?}", w.result("stats"));
        set_stat(&mut w, GearSlot::Head, "Marauder's", "Berserker's");
        assert_eq!(w.result("stats").actual.as_deref(), Some("Head: Berserker's"));
    }

    #[test]
    fn rings_and_accessories_are_unordered_pairs() {
        let mut w = World::matching(firebrand());
        set_stat(&mut w, GearSlot::Ring1, "Assassin's", "Berserker's");
        set_stat(&mut w, GearSlot::Ring2, "Berserker's", "Assassin's");
        set_stat(&mut w, GearSlot::Accessory1, "Harrier's", "Cleric's");
        set_stat(&mut w, GearSlot::Accessory2, "Cleric's", "Harrier's");
        assert_eq!(w.result("stats").status, Status::Pass, "{:?}", w.result("stats"));

        // A genuinely wrong ring still fails, naming the slot that holds it.
        set_stat(&mut w, GearSlot::Ring2, "Berserker's", "Viper's");
        let r = w.result("stats");
        assert_eq!((r.status, r.actual.as_deref()), (Status::Fail, Some("Ring 2: Viper's")));

        // Two of the same where the build wants two different ones fails too.
        set_stat(&mut w, GearSlot::Ring2, "Berserker's", "Berserker's");
        let r = w.result("stats");
        assert_eq!(r.status, Status::Fail);

        // An unresolved ring partner is Unknown, not a guess.
        let mut w = World::matching(firebrand());
        set_stat(&mut w, GearSlot::Ring1, "Assassin's", "Berserker's");
        set_stat(&mut w, GearSlot::Ring2, "Berserker's", "Assassin's");
        w.item_mut(GearSlot::Ring2).stats_id = Some(77_777);
        w.db.items.remove(&item_id(GearSlot::Ring2));
        assert_eq!(w.result("stats").status, Status::Unknown, "{:?}", w.result("stats"));
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
        w.db.skills.insert(9104, SkillInfo { name: "True Strike".into(), weapon_type: Some("Mace".into()), ..Default::default() });
        w.live.skills_cast.insert(9104);
        let r = w.result("api.Weapons");
        assert_eq!(r.status, Status::Unknown);
        assert_eq!(r.actual.as_deref(), Some("seen in use: mace"));
    }

    use crate::report::SlotKey;

    fn mark_of(w: &World, id: &str, key: SlotKey) -> Option<(Status, Option<String>)> {
        w.result(id).marks.into_iter().find(|m| m.key == key).map(|m| (m.status, m.detail))
    }

    #[test]
    fn matching_gear_marks_pass() {
        let w = World::matching(firebrand());
        assert_eq!(mark_of(&w, "stats", SlotKey::Gear(GearSlot::Head)).map(|m| m.0), Some(Status::Pass));
        let rune_slot = *firebrand().equipment.runes.keys().next().unwrap();
        assert_eq!(mark_of(&w, "runes", SlotKey::Rune(rune_slot)).map(|m| m.0), Some(Status::Pass));
        assert_eq!(mark_of(&w, "weapons.A", SlotKey::Gear(GearSlot::WeaponA1)).map(|m| m.0), Some(Status::Pass));
    }

    #[test]
    fn an_empty_slot_marks_that_slot_only() {
        let mut w = World::matching(firebrand());
        w.snap_mut().equipment.retain(|i| i.slot != "Helm");
        assert_eq!(mark_of(&w, "stats", SlotKey::Gear(GearSlot::Head)), Some((Status::Fail, Some("empty".into()))));
        assert_eq!(mark_of(&w, "stats", SlotKey::Gear(GearSlot::Feet)).map(|m| m.0), Some(Status::Pass));
    }

    #[test]
    fn a_missing_rune_marks_its_slot() {
        let mut w = World::matching(firebrand());
        let slot = *firebrand().equipment.runes.keys().next().unwrap();
        w.item_mut(slot).upgrades.clear();
        assert_eq!(mark_of(&w, "runes", SlotKey::Rune(slot)), Some((Status::Fail, Some("none".into()))));
    }

    #[test]
    fn sigils_mark_each_wanted_sigil() {
        let w = World::matching(firebrand());
        let r = w.result("sigils.A");
        let n = firebrand().equipment.sigils.a1.len() + firebrand().equipment.sigils.a2.len();
        assert_eq!(r.marks.iter().filter(|m| matches!(m.key, SlotKey::Sigil(_, _))).count(), n);
        assert!(r.marks.iter().all(|m| m.status == Status::Pass));
    }

    #[test]
    fn relic_and_infusions_are_marked() {
        let w = World::matching(firebrand());
        assert_eq!(mark_of(&w, "relic", SlotKey::Relic).map(|m| m.0), Some(Status::Pass));
        assert_eq!(mark_of(&w, "infusions", SlotKey::Infusions).map(|m| m.0), Some(Status::Pass));
    }
}
