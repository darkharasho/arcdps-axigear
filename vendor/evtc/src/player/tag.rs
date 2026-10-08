#![allow(deprecated)]

use crate::{AgentId, Event, StateChange, TryExtract, extract::Extract};

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Agent has a tag.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[deprecated(since = "0.9.0", note = "replaced by agent marker event")]
pub struct TagEvent {
    /// Time of registering the event.
    pub time: u64,

    /// Agent that has the tag.
    pub agent: AgentId,

    /// Tag id.
    ///
    /// Id is volatile, depends on game build.
    pub tag: i32,
}

impl Extract for TagEvent {
    #[inline]
    unsafe fn extract(event: &Event) -> Self {
        Self {
            time: event.time,
            agent: AgentId::from_src(event),
            tag: event.value,
        }
    }
}

impl TryExtract for TagEvent {
    #[inline]
    fn can_extract(event: &Event) -> bool {
        event.get_statechange() == StateChange::Marker
    }
}
