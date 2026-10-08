//! Test-only helpers. Grows in later tasks (World builder in Task 14).

use crate::axicode::decode_comp_code;
use crate::model::{Build, Comp};

pub fn fixture(name: &str) -> String {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/");
    std::fs::read_to_string(format!("{path}{name}"))
        .unwrap_or_else(|e| panic!("fixture {name}: {e}"))
        .trim()
        .to_string()
}

/// The Tuesday comp: builds [Firebrand, Berserker, core Necro];
/// party 1 = [Firebrand, Berserker, DPS tag(Berserker, Necro)], party 2 = [Firebrand, Necro].
pub fn comp() -> Comp {
    decode_comp_code(&fixture("comp-tuesday.txt")).unwrap()
}

pub fn firebrand() -> Build {
    comp().builds[0].clone()
}

pub fn berserker() -> Build {
    comp().builds[1].clone()
}

pub fn necro() -> Build {
    comp().builds[2].clone()
}

use std::time::{Duration, Instant};

use crate::checks::{ApiState, Ctx};
use crate::consumables::Consumables;
use crate::gamedb::{GameDb, ItemInfo};
use crate::gw2api::{ApiBuild, ApiItem, ApiSkills, ApiSnapshot, ApiSpec};
use crate::live::LiveState;
use crate::model::{GearSlot, TraitSel};
use crate::mumble::{profession_id, Identity};
use crate::report::{CheckResult, Severities};
use crate::specs::SpecDb;

pub fn identity_for(build: &Build) -> Identity {
    Identity {
        name: "Tester".into(),
        profession: profession_id(&build.profession),
        spec: build.specs[2].id,
        map_id: 1099,
        map_type: 9,
        in_combat: false,
    }
}

pub fn api_slot(slot: GearSlot) -> &'static str {
    match slot {
        GearSlot::Head => "Helm",
        GearSlot::Shoulders => "Shoulders",
        GearSlot::Chest => "Coat",
        GearSlot::Hands => "Gloves",
        GearSlot::Legs => "Leggings",
        GearSlot::Feet => "Boots",
        GearSlot::Back => "Backpack",
        GearSlot::Amulet => "Amulet",
        GearSlot::Ring1 => "Ring1",
        GearSlot::Ring2 => "Ring2",
        GearSlot::Accessory1 => "Accessory1",
        GearSlot::Accessory2 => "Accessory2",
        GearSlot::WeaponA1 => "WeaponA1",
        GearSlot::WeaponA2 => "WeaponA2",
        GearSlot::WeaponB1 => "WeaponB1",
        GearSlot::WeaponB2 => "WeaponB2",
    }
}

pub struct World {
    pub build: Build,
    pub live: LiveState,
    pub api: ApiState,
    pub db: GameDb,
    pub now: Instant,
    pub assigned_at: Instant,
}

/// Item ID of the synthetic equipped item in `slot`.
pub fn item_id(slot: GearSlot) -> u32 {
    900_000 + GearSlot::ALL.iter().position(|s| *s == slot).unwrap() as u32
}

pub const RELIC_ITEM: u32 = 950_000;

impl World {
    pub fn matching(build: Build) -> World {
        let specs = SpecDb::bundled();
        let cons = Consumables::bundled();
        let now = Instant::now() + Duration::from_secs(3600);
        let e = build.equipment.clone();
        let mut db = GameDb::default();
        let mut stat_names: Vec<String> = Vec::new();
        let mut equipment = Vec::new();

        for slot in GearSlot::ALL {
            let weapon = e.weapons.get(slot);
            let stat = e.stats.get(&slot);
            if (slot.is_weapon() && weapon.is_none()) || (stat.is_none() && weapon.is_none()) {
                continue;
            }
            let stats_id = stat.map(|s| {
                let idx = stat_names.iter().position(|x| x == s).unwrap_or_else(|| {
                    stat_names.push(s.clone());
                    stat_names.len() - 1
                });
                let id = 1000 + idx as u32;
                db.itemstats.insert(id, s.clone());
                id
            });
            db.items.insert(
                item_id(slot),
                ItemInfo { name: format!("{} item", slot.label()), weapon_type: weapon.map(str::to_string), default_stats: stats_id },
            );
            let upgrades = if slot.is_weapon() { e.sigils.get(slot).to_vec() } else { e.runes.get(&slot).copied().into_iter().collect() };
            let infusions = if slot == GearSlot::Head { e.infusions.clone() } else { Vec::new() };
            equipment.push(ApiItem { id: item_id(slot), slot: api_slot(slot).into(), upgrades, infusions, stats_id: None });
        }
        for id in e.runes.values().chain(&e.sigils.a1).chain(&e.sigils.a2).chain(&e.sigils.b1).chain(&e.sigils.b2) {
            db.items.entry(*id).or_insert_with(|| ItemInfo { name: format!("Superior Upgrade {id}"), ..Default::default() });
        }
        for id in &e.infusions {
            db.items.entry(*id).or_insert_with(|| ItemInfo { name: format!("Infusion {id}"), ..Default::default() });
        }
        if let Some(relic) = &e.relic {
            db.items.insert(RELIC_ITEM, ItemInfo { name: relic.clone(), ..Default::default() });
            equipment.push(ApiItem { id: RELIC_ITEM, slot: "Relic".into(), upgrades: vec![], infusions: vec![], stats_id: None });
        }

        let api_specs = build
            .specs
            .iter()
            .map(|l| ApiSpec {
                id: l.id,
                traits: [0, 1, 2].map(|t| match l.majors[t] {
                    TraitSel::Id(id) => id,
                    TraitSel::Position(p) => specs.trait_at(l.id, t + 1, p).unwrap_or(0),
                    TraitSel::None => 0,
                }),
            })
            .collect();
        let snapshot = ApiSnapshot {
            fetched_at: now - Duration::from_secs(120),
            character: "Tester".into(),
            build: ApiBuild {
                profession: build.profession.clone(),
                specs: api_specs,
                skills: ApiSkills { heal: build.skills.heal, utilities: build.skills.utilities, elite: build.skills.elite },
            },
            equipment,
        };

        let mut live = LiveState::default();
        live.set_identity(identity_for(&build), now - Duration::from_secs(60));
        live.baseline = true;
        for label in [&e.food, &e.utility].into_iter().flatten() {
            live.active.insert(cons.find_label(label).expect("bundled consumable for label"));
        }
        let s = &build.skills;
        live.skills_cast.extend(s.utilities.iter().copied().chain([s.elite]).filter(|i| *i != 0));

        World {
            build,
            live,
            api: ApiState { has_key: true, snapshot: Some(snapshot), error: None },
            db,
            now,
            assigned_at: now - Duration::from_secs(60),
        }
    }

    pub fn snap_mut(&mut self) -> &mut ApiSnapshot {
        self.api.snapshot.as_mut().unwrap()
    }

    pub fn item_mut(&mut self, slot: GearSlot) -> &mut ApiItem {
        let name = api_slot(slot);
        self.snap_mut().equipment.iter_mut().find(|i| i.slot == name).unwrap()
    }

    pub fn ctx(&self) -> Ctx<'_> {
        Ctx {
            build: &self.build,
            live: &self.live,
            api: &self.api,
            db: &self.db,
            specs: SpecDb::bundled(),
            consumables: Consumables::bundled(),
            now: self.now,
            assigned_at: self.assigned_at,
        }
    }

    pub fn run(&self) -> Vec<CheckResult> {
        self.run_with(&Severities::default())
    }

    pub fn run_with(&self, severities: &Severities) -> Vec<CheckResult> {
        crate::checks::run(&self.ctx(), severities)
    }

    pub fn has(&self, id: &str) -> bool {
        self.run().iter().any(|r| r.id == id)
    }

    pub fn result(&self, id: &str) -> CheckResult {
        let all = self.run();
        let ids: Vec<String> = all.iter().map(|r| r.id.clone()).collect();
        all.into_iter().find(|r| r.id == id).unwrap_or_else(|| panic!("no result {id}; have {ids:?}"))
    }
}
