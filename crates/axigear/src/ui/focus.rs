//! Problem click → which slot to pulse. Host-testable.

use axigear_core::report::{CheckResult, SlotKey, Status};

pub const PULSE_SECS: f64 = 1.5;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Focus {
    pub key: SlotKey,
    pub until: f64,
}

impl Focus {
    pub fn active(&self, now: f64) -> bool {
        now < self.until
    }
}

/// The slot a problem line jumps to: its first non-passing mark, else its first mark.
pub fn target(r: &CheckResult) -> Option<SlotKey> {
    r.marks
        .iter()
        .find(|m| m.status != Status::Pass)
        .or(r.marks.first())
        .map(|m| m.key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axigear_core::model::GearSlot;
    use axigear_core::report::{Category, Tab};

    #[test]
    fn target_prefers_the_failing_mark() {
        let r = CheckResult::new(Category::Runes, "runes", "Runes", Status::Fail, "x")
            .mark(SlotKey::Rune(GearSlot::Head), Status::Pass, None)
            .mark(SlotKey::Rune(GearSlot::Feet), Status::Fail, None);
        assert_eq!(target(&r), Some(SlotKey::Rune(GearSlot::Feet)));
        assert_eq!(target(&r).unwrap().tab(), Tab::Equipment);
        assert_eq!(
            target(&CheckResult::new(
                Category::Spec,
                "spec",
                "Spec",
                Status::Fail,
                "x"
            )),
            None
        );
    }

    #[test]
    fn pulse_expires() {
        let f = Focus {
            key: SlotKey::Relic,
            until: 10.0 + PULSE_SECS,
        };
        assert!(f.active(11.0) && !f.active(11.5));
    }
}
