//! Spec, specializations, traits, skill bar, skills seen.

use crate::checks::Ctx;
use crate::gw2api::ApiSnapshot;
use crate::matcher::live_elite;
use crate::model::TraitSel;
use crate::mumble::{profession_name, unknown_profession};
use crate::report::{Category, CheckResult, Status};

const TIERS: [&str; 3] = ["Adept", "Master", "Grandmaster"];

fn position_word(p: u8) -> &'static str {
    match p {
        1 => "top",
        2 => "middle",
        3 => "bottom",
        _ => "none",
    }
}

fn spec_name(ctx: &Ctx, id: u16) -> String {
    ctx.specs.name(id).map(String::from).unwrap_or_else(|| format!("spec {id}"))
}

pub fn skill_name(ctx: &Ctx, id: u32) -> String {
    if id == 0 {
        return "empty".into();
    }
    ctx.build
        .skill_names
        .get(&id)
        .cloned()
        .or_else(|| ctx.db.skill_name(id).map(String::from))
        .unwrap_or_else(|| format!("skill {id}"))
}

pub fn spec(ctx: &Ctx) -> Vec<CheckResult> {
    let expected = ctx.build.spec_label(ctx.specs);
    let row = |status| CheckResult::new(Category::Spec, "spec", "Spec", status, expected.clone());
    let Some(id) = &ctx.live.identity else {
        return vec![row(Status::Unknown).with_reason("waiting for MumbleLink")];
    };
    let Some(profession) = profession_name(id.profession) else {
        return vec![row(Status::Unknown).with_reason(unknown_profession(id.profession))];
    };
    let elite = live_elite(id, ctx.specs);
    let actual = elite
        .and_then(|e| ctx.specs.name(e))
        .map(String::from)
        .unwrap_or_else(|| format!("{profession} (core)"));
    let ok = ctx.build.profession.eq_ignore_ascii_case(profession) && ctx.build.elite_spec(ctx.specs) == elite;
    vec![row(if ok { Status::Pass } else { Status::Fail }).with_actual(actual)]
}

pub fn specializations(ctx: &Ctx, snap: &ApiSnapshot) -> Vec<CheckResult> {
    let want: Vec<u16> = ctx.build.specs.iter().map(|s| s.id).filter(|id| *id != 0).collect();
    if want.is_empty() {
        return Vec::new();
    }
    let have: Vec<u16> = snap.build.specs.iter().map(|s| s.id).filter(|id| *id != 0).collect();
    let names = |ids: &[u16]| ids.iter().map(|i| spec_name(ctx, *i)).collect::<Vec<_>>().join(" · ");
    let elite_ok = ctx.build.elite_spec(ctx.specs).map_or(true, |e| snap.spec3() == e);
    let ok = want.iter().all(|w| have.contains(w)) && elite_ok;
    vec![CheckResult::new(
        Category::Specializations,
        "specs",
        "Specializations",
        if ok { Status::Pass } else { Status::Fail },
        names(&want),
    )
    .with_actual(names(&have))]
}

pub fn traits(ctx: &Ctx, snap: &ApiSnapshot) -> Vec<CheckResult> {
    let mut out = Vec::new();
    for line in ctx.build.specs.iter().filter(|l| l.id != 0) {
        if line.majors.iter().all(|m| *m == TraitSel::None) {
            continue;
        }
        // A line that isn't equipped is reported once, by Specializations.
        let Some(api) = snap.build.specs.iter().find(|s| s.id == line.id) else { continue };
        let name = spec_name(ctx, line.id);
        let (mut want_words, mut have_words) = (Vec::new(), Vec::new());
        let mut status = Status::Pass;
        for t in 0..3 {
            let have_pos = ctx.specs.position(line.id, t + 1, api.traits[t]);
            let (want_pos, ok) = match line.majors[t] {
                TraitSel::None => continue,
                TraitSel::Position(p) => (Some(p), have_pos.map(|h| h == p)),
                TraitSel::Id(id) => (ctx.specs.position(line.id, t + 1, id), Some(id == api.traits[t])),
            };
            want_words.push(format!("{} {}", TIERS[t], want_pos.map_or("?", position_word)));
            have_words.push(format!("{} {}", TIERS[t], have_pos.map_or("?", position_word)));
            match ok {
                Some(true) => {}
                Some(false) => status = Status::Fail,
                None if status == Status::Pass => status = Status::Unknown,
                None => {}
            }
        }
        let mut r = CheckResult::new(Category::Traits, format!("traits.{}", line.id), format!("Traits: {name}"), status, want_words.join(", "))
            .with_actual(have_words.join(", "));
        if status == Status::Unknown {
            r = r.with_reason(format!("no trait data for {name} - update axigear"));
        }
        out.push(r);
    }
    out
}

pub fn skill_bar(ctx: &Ctx, snap: &ApiSnapshot) -> Vec<CheckResult> {
    let want = &ctx.build.skills;
    let named: Vec<String> = [want.heal].into_iter().chain(want.utilities).chain([want.elite])
        .filter(|id| *id != 0)
        .map(|id| skill_name(ctx, id))
        .collect();
    if named.is_empty() {
        return Vec::new();
    }
    let expected = named.join(" · ");
    let row = |status| CheckResult::new(Category::SkillBar, "skills", "Skill bar", status, expected.clone());
    if snap.build.profession.eq_ignore_ascii_case("Revenant") {
        return vec![row(Status::Unknown).with_reason("Revenant skills come from legends - not checked")];
    }
    let have = &snap.build.skills;
    let mut problems = Vec::new();
    if want.heal != 0 && want.heal != have.heal {
        problems.push(format!("heal: {}", skill_name(ctx, have.heal)));
    }
    if want.elite != 0 && want.elite != have.elite {
        problems.push(format!("elite: {}", skill_name(ctx, have.elite)));
    }
    let mut pool = have.utilities.to_vec();
    for u in want.utilities.iter().filter(|u| **u != 0) {
        match pool.iter().position(|h| h == u) {
            Some(i) => {
                pool.swap_remove(i);
            }
            None => problems.push(format!("missing {}", skill_name(ctx, *u))),
        }
    }
    if problems.is_empty() {
        vec![row(Status::Pass).with_actual(expected.clone())]
    } else {
        vec![row(Status::Fail).with_actual(problems.join("; "))]
    }
}

pub fn skills_seen(ctx: &Ctx) -> Vec<CheckResult> {
    let s = &ctx.build.skills;
    s.utilities
        .iter()
        .copied()
        .chain([s.elite])
        .filter(|id| *id != 0)
        .map(|id| {
            let name = skill_name(ctx, id);
            if ctx.live.skills_cast.contains(&id) {
                CheckResult::new(Category::SkillsSeen, format!("seen.{id}"), name.clone(), Status::Pass, name).with_actual("cast this map")
            } else {
                CheckResult::new(Category::SkillsSeen, format!("seen.{id}"), name.clone(), Status::Unknown, name).with_reason("not cast yet this map")
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use crate::model::TraitSel;
    use crate::report::{Category, Severities, Severity, SeveritySetting, Status};
    use crate::testutil::{firebrand, necro, World};

    fn statuses(w: &World, prefix: &str) -> Vec<(String, Status)> {
        w.run().into_iter().filter(|r| r.id.starts_with(prefix)).map(|r| (r.id, r.status)).collect()
    }

    #[test]
    fn a_matching_firebrand_passes_every_build_check() {
        let w = World::matching(firebrand());
        for id in ["spec", "specs", "traits.42", "traits.46", "traits.62", "skills", "seen.40915", "seen.9153", "seen.9246", "seen.43357"] {
            assert_eq!(w.result(id).status, Status::Pass, "{id}: {:?}", w.result(id));
        }
        assert_eq!(w.result("spec").expected, "Firebrand");
    }

    #[test]
    fn wrong_elite_fails_spec() {
        let mut w = World::matching(firebrand());
        w.live.identity.as_mut().unwrap().spec = 27; // Dragonhunter
        let r = w.result("spec");
        assert_eq!((r.status, r.actual.as_deref()), (Status::Fail, Some("Dragonhunter")));
    }

    #[test]
    fn an_unknown_profession_id_is_unknown_not_a_fail() {
        let mut w = World::matching(firebrand());
        w.live.identity.as_mut().unwrap().profession = 12;
        let r = w.result("spec");
        assert_eq!((r.status, r.reason.as_deref()), (Status::Unknown, Some("unknown profession id 12 - update axigear")));
    }

    #[test]
    fn no_mumblelink_means_spec_unknown() {
        let mut w = World::matching(firebrand());
        w.live.identity = None;
        let r = w.result("spec");
        assert_eq!((r.status, r.reason.as_deref()), (Status::Unknown, Some("waiting for MumbleLink")));
    }

    #[test]
    fn missing_line_or_elite_out_of_slot_three_fails_specializations() {
        let mut w = World::matching(firebrand());
        w.snap_mut().build.specs[1].id = 16; // Radiance instead of Virtues
        assert_eq!(w.result("specs").status, Status::Fail);
        assert!(!w.has("traits.46"), "a missing line is reported once, by Specializations");

        let mut w = World::matching(firebrand());
        w.snap_mut().build.specs.swap(0, 2);
        w.live.identity.as_mut().unwrap().spec = 42; // MumbleLink reports line 3
        assert_eq!(w.result("specs").status, Status::Fail);
    }

    #[test]
    fn a_wrong_trait_fails_that_line_with_positions() {
        let mut w = World::matching(firebrand());
        let wanted = w.snap_mut().build.specs[0].traits[0];
        let other = crate::specs::SpecDb::bundled().get(42).unwrap().majors[0].iter().copied().find(|t| *t != wanted).unwrap();
        w.snap_mut().build.specs[0].traits[0] = other;
        let r = w.result("traits.42");
        assert_eq!(r.status, Status::Fail);
        assert!(r.expected.starts_with("Adept middle"), "{}", r.expected);
        assert_eq!(r.label, "Traits: Zeal");
    }

    #[test]
    fn published_trait_ids_compare_directly() {
        let mut b = firebrand();
        b.specs[2].majors = [TraitSel::Id(2086), TraitSel::Id(2063), TraitSel::Id(2105)];
        assert_eq!(World::matching(b).result("traits.62").status, Status::Pass);
    }

    #[test]
    fn unknown_spec_data_is_unknown_not_fail() {
        let mut b = firebrand();
        b.specs[0].id = 999;
        let mut w = World::matching(b);
        w.snap_mut().build.specs[0] = crate::gw2api::ApiSpec { id: 999, traits: [1, 2, 3] };
        assert_eq!(w.result("traits.999").status, Status::Unknown);
    }

    #[test]
    fn utilities_are_a_set_and_empty_slots_are_ignored() {
        let mut w = World::matching(firebrand());
        w.snap_mut().build.skills.utilities.reverse();
        assert_eq!(w.result("skills").status, Status::Pass);

        w.snap_mut().build.skills.utilities[0] = 12345;
        let r = w.result("skills");
        assert_eq!(r.status, Status::Fail);
        assert!(r.actual.unwrap().starts_with("missing"));

        let mut w = World::matching(necro()); // utilities [10545, 0, 10685]
        w.snap_mut().build.skills.utilities[1] = 10533;
        assert_eq!(w.result("skills").status, Status::Pass);
    }

    #[test]
    fn revenant_skill_bar_is_unknown() {
        let mut w = World::matching(firebrand());
        w.snap_mut().build.profession = "Revenant".into();
        let r = w.result("skills");
        assert_eq!(r.status, Status::Unknown);
        assert!(r.reason.unwrap().contains("legends"));
    }

    #[test]
    fn skills_not_yet_cast_are_unknown_and_advisory() {
        let mut w = World::matching(firebrand());
        w.live.skills_cast.clear();
        let r = w.result("seen.9153");
        assert_eq!((r.status, r.severity), (Status::Unknown, Severity::Advisory));
        assert!(w.run().iter().filter(|r| r.category == Category::SkillsSeen).all(|r| r.status != Status::Fail));
    }

    #[test]
    fn the_api_gate_explains_itself() {
        let mut w = World::matching(firebrand());
        w.api.has_key = false;
        let r = w.result("api.Traits");
        assert_eq!((r.status, r.reason.as_deref()), (Status::Unknown, Some("needs API key (characters, builds)")));
        assert!(!w.has("traits.42"));

        let mut w = World::matching(firebrand());
        w.api.snapshot = None;
        w.api.error = Some("character not on this API key".into());
        assert_eq!(w.result("api.Skill bar").reason.as_deref(), Some("character not on this API key"));

        let mut w = World::matching(firebrand());
        w.live.identity.as_mut().unwrap().name = "Alt".into();
        assert!(w.result("api.Specializations").reason.unwrap().starts_with("stale"));
    }

    #[test]
    fn severity_off_removes_rows_and_api_rows_carry_age() {
        let w = World::matching(firebrand());
        let mut sev = Severities::default();
        sev.set(Category::Traits, SeveritySetting::Off);
        assert!(w.run_with(&sev).iter().all(|r| r.category != Category::Traits));
        assert_eq!(w.result("specs").age_secs, Some(120));
        assert_eq!(w.result("spec").age_secs, None);
        assert!(statuses(&w, "traits.").iter().all(|(_, s)| *s == Status::Pass));
    }
}
