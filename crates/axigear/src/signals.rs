#![cfg(windows)]
//! arcdps realtime events → core `LiveEvent`s. Only events about self pass.
//! (Buff events: dst = recipient for apply/initial, src = agent that lost it
//! for remove; verified in game in Task 24.)

use arcdps::{Activation, Agent, BuffRemove, Event, StateChange};
use axigear_core::live::LiveEvent;
use evtc::EventCategory;

fn is_self(a: Option<&Agent>) -> bool {
    a.is_some_and(|a| a.is_self != 0)
}

pub fn translate(ev: Option<&Event>, src: Option<&Agent>, dst: Option<&Agent>) -> Option<LiveEvent> {
    let ev = ev?;
    match ev.categorize() {
        EventCategory::StateChange => match ev.get_statechange() {
            StateChange::EnterCombat if is_self(src) => Some(LiveEvent::Combat { active: true }),
            StateChange::ExitCombat if is_self(src) => Some(LiveEvent::Combat { active: false }),
            StateChange::WeaponSwap if is_self(src) => Some(LiveEvent::WeaponSwap { set: ev.dst_agent as u8 }),
            StateChange::BuffInitial if is_self(dst) => Some(LiveEvent::BuffApply { id: ev.skill_id, initial: true }),
            _ => None,
        },
        EventCategory::BuffApply if is_self(dst) => Some(LiveEvent::BuffApply { id: ev.skill_id, initial: false }),
        EventCategory::BuffRemove if is_self(src) && matches!(ev.get_buffremove(), BuffRemove::All) => {
            Some(LiveEvent::BuffRemove { id: ev.skill_id })
        }
        EventCategory::Activation if is_self(src) && matches!(ev.get_activation(), Activation::Start | Activation::QuicknessUnused) => {
            Some(LiveEvent::SkillCast { id: ev.skill_id })
        }
        _ => None,
    }
}
