//! ImGui overlay. `theme` and `axi` are host-compilable (their geometry and
//! tokens are pure); everything that touches `Ui` is Windows-only.

pub mod axi;
pub mod theme;

/// Badge scale bounds. Hand-edited configs can carry 0 or negative values.
pub const SCALE_MIN: f32 = 0.75;
pub const SCALE_MAX: f32 = 2.0;

/// Clamp a badge scale into the sane range; non-finite values fall back to 1.0.
pub fn clamp_scale(s: f32) -> f32 {
    if s.is_finite() { s.clamp(SCALE_MIN, SCALE_MAX) } else { 1.0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scale_is_clamped() {
        assert_eq!(clamp_scale(0.0), SCALE_MIN);
        assert_eq!(clamp_scale(-2.0), SCALE_MIN);
        assert_eq!(clamp_scale(99.0), SCALE_MAX);
        assert_eq!(clamp_scale(f32::NAN), 1.0);
        assert_eq!(clamp_scale(1.25), 1.25);
    }
}

#[cfg(windows)]
pub mod badge;
#[cfg(windows)]
pub mod checklist;
#[cfg(windows)]
pub mod icons;
#[cfg(windows)]
pub mod settings;
#[cfg(windows)]
pub mod state;
