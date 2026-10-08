//! Pure logic for axigear: decoding comps, matching slots, and running checks.
//! Nothing here touches Windows, arcdps or the game; the plugin crate feeds it.

pub mod axicode;
pub mod consumables;
pub mod http;
pub mod link;
pub mod live;
pub mod loader;
pub mod matcher;
pub mod model;
pub mod mumble;
pub mod publish;
pub mod raw;
pub mod specs;
pub mod text;

#[cfg(test)]
pub(crate) mod testutil;
