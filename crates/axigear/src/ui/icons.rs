//! Status marks drawn with primitives: the arcdps font has no ✓ ⚠ ✗ glyphs.

use arcdps::imgui::Ui;
use axigear_core::report::Tone;

use super::theme;

pub fn ink(tone: Tone) -> [f32; 4] {
    match tone {
        Tone::Neutral => theme::TEXT_FAINT,
        Tone::Ok => theme::OK,
        Tone::Warn => theme::WARN,
        Tone::Danger => theme::DANGER,
    }
}

/// Reserve a `size`×`size` cell at the cursor and draw the mark for `tone`.
/// Takes (and drops) its own draw list: never call while holding one.
pub fn draw(ui: &Ui, tone: Tone, size: f32) {
    let origin = ui.cursor_screen_pos();
    ui.dummy([size, size]);
    draw_at(ui, origin, tone, size);
}

/// Draw the mark for `tone` at `origin` without reserving layout space or
/// touching imgui's last-item state. Takes its own draw list.
pub fn draw_at(ui: &Ui, origin: [f32; 2], tone: Tone, size: f32) {
    let [x, y] = origin;
    let ink = ink(tone);
    let t = (size / 7.0).max(1.5);
    let p = |fx: f32, fy: f32| [x + fx * size, y + fy * size];
    let dl = ui.get_window_draw_list();
    match tone {
        Tone::Ok => {
            dl.add_line(p(0.15, 0.55), p(0.40, 0.80), ink).thickness(t).build();
            dl.add_line(p(0.40, 0.80), p(0.85, 0.20), ink).thickness(t).build();
        }
        Tone::Warn => {
            dl.add_triangle(p(0.50, 0.10), p(0.92, 0.88), p(0.08, 0.88), ink).thickness(t).build();
            dl.add_line(p(0.50, 0.38), p(0.50, 0.62), ink).thickness(t).build();
            dl.add_line(p(0.50, 0.72), p(0.50, 0.78), ink).thickness(t).build();
        }
        Tone::Danger => {
            dl.add_line(p(0.20, 0.20), p(0.80, 0.80), ink).thickness(t).build();
            dl.add_line(p(0.80, 0.20), p(0.20, 0.80), ink).thickness(t).build();
        }
        Tone::Neutral => dl.add_text(p(0.30, 0.0), ink, "?"),
    }
}
