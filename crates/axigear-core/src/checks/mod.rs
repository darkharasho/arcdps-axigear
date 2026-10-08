//! The check engine: expected build + live state + API snapshot → results.

pub mod build;
pub mod consumables;
pub mod gear;

use std::time::Instant;

use crate::consumables::Consumables;
use crate::gamedb::GameDb;
use crate::gw2api::ApiSnapshot;
use crate::live::LiveState;
use crate::model::Build;
use crate::report::{Category, CheckResult, Severities, Severity, Status};
use crate::specs::SpecDb;

#[derive(Debug, Clone, Default)]
pub struct ApiState {
    pub has_key: bool,
    /// Last good snapshot; kept across errors so its age stays visible.
    pub snapshot: Option<ApiSnapshot>,
    pub error: Option<String>,
}

pub struct Ctx<'a> {
    pub build: &'a Build,
    pub live: &'a LiveState,
    pub api: &'a ApiState,
    pub db: &'a GameDb,
    pub specs: &'a SpecDb,
    pub consumables: &'a Consumables,
    pub now: Instant,
    /// When the current slot was assigned (starts the consumable grace period).
    pub assigned_at: Instant,
}

pub const API_CATEGORIES: [Category; 9] = [
    Category::Specializations, Category::Traits, Category::SkillBar, Category::Weapons,
    Category::Stats, Category::Runes, Category::Sigils, Category::Relic, Category::Infusions,
];

/// Whether the build specifies anything for an API category (unspecified → no check).
pub fn expects(build: &Build, cat: Category) -> bool {
    let e = &build.equipment;
    match cat {
        Category::Specializations | Category::Traits => build.specs.iter().any(|s| s.id != 0),
        Category::SkillBar => build.skills.heal != 0 || build.skills.elite != 0 || build.skills.utilities.iter().any(|u| *u != 0),
        Category::Weapons => e.weapons.a1.is_some() || e.weapons.a2.is_some() || e.weapons.b1.is_some() || e.weapons.b2.is_some(),
        Category::Stats => !e.stats.is_empty(),
        Category::Runes => !e.runes.is_empty(),
        Category::Sigils => [&e.sigils.a1, &e.sigils.a2, &e.sigils.b1, &e.sigils.b2].iter().any(|s| !s.is_empty()),
        Category::Relic => e.relic.is_some(),
        Category::Infusions => !e.infusions.is_empty(),
        _ => false,
    }
}

/// Why API checks can't run right now, if they can't.
pub(crate) fn api_gate(ctx: &Ctx) -> Option<String> {
    if !ctx.api.has_key {
        return Some("needs API key (characters, builds)".into());
    }
    let Some(snap) = &ctx.api.snapshot else {
        return Some(ctx.api.error.clone().unwrap_or_else(|| "waiting for API".into()));
    };
    if let Some(id) = &ctx.live.identity {
        if id.name != snap.character || (id.spec != 0 && snap.spec3() != id.spec) {
            return Some("stale - waiting for API refresh".into());
        }
    }
    None
}

pub fn run(ctx: &Ctx, severities: &Severities) -> Vec<CheckResult> {
    let mut out = build::spec(ctx);
    match api_gate(ctx) {
        Some(reason) => {
            for cat in API_CATEGORIES.into_iter().filter(|c| expects(ctx.build, *c)) {
                let mut row = CheckResult::new(cat, format!("api.{}", cat.label()), cat.label(), Status::Unknown, "")
                    .with_reason(reason.clone());
                if cat == Category::Weapons {
                    if let Some(seen) = gear::weapon_evidence(ctx) {
                        row = row.with_actual(seen);
                    }
                }
                out.push(row);
            }
        }
        None => {
            let snap = ctx.api.snapshot.as_ref().expect("gate checked the snapshot");
            let mut api = Vec::new();
            api.extend(build::specializations(ctx, snap));
            api.extend(build::traits(ctx, snap));
            api.extend(build::skill_bar(ctx, snap));
            api.extend(gear::weapons(ctx, snap));
            api.extend(gear::stats(ctx, snap));
            api.extend(gear::runes(ctx, snap));
            api.extend(gear::sigils(ctx, snap));
            api.extend(gear::relic(ctx, snap));
            api.extend(gear::infusions(ctx, snap));
            let age = ctx.now.saturating_duration_since(snap.fetched_at).as_secs();
            for r in &mut api {
                r.age_secs = Some(age);
            }
            out.extend(api);
        }
    }
    out.extend(build::skills_seen(ctx));
    out.extend(consumables::food(ctx));
    out.extend(consumables::utility(ctx));
    out.retain_mut(|r| match severities.get(r.category).severity() {
        None => false,
        Some(s) => {
            r.severity = if r.forced_advisory { Severity::Advisory } else { s };
            true
        }
    });
    out
}
