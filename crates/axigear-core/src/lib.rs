//! Pure logic for axigear: decoding comps, matching slots, and running checks.
//! Nothing here touches Windows, arcdps or the game; the plugin crate feeds it.

pub mod axicode;
pub mod checks;
pub mod consumables;
pub mod driver;
pub mod fsutil;
pub mod gamedb;
pub mod gw2api;
pub mod http;
pub mod icons;
pub mod link;
pub mod loadout;
pub mod live;
pub mod loader;
pub mod matcher;
pub mod model;
pub mod mumble;
pub mod publish;
pub mod raw;
pub mod report;
pub mod schedule;
pub mod session;
pub mod settings;
pub mod specs;
pub mod text;

#[cfg(test)]
pub(crate) mod testutil;
