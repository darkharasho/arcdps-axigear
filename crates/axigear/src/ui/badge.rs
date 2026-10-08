//! The always-on badge: icon + count, coloured by tone. Click toggles the
//! checklist; drag moves it (position persisted).

use arcdps::imgui::{Condition, MouseButton, StyleColor, Ui, WindowFlags};
use axigear_core::driver::UiSnapshot;
use axigear_core::report::Badge;
use axigear_core::settings::BadgeSettings;

use super::axi::{self, Rect};
use super::state::UiState;
use super::{icons, theme};

const FLAGS: WindowFlags = WindowFlags::NO_TITLE_BAR
    .union(WindowFlags::NO_RESIZE)
    .union(WindowFlags::NO_SCROLLBAR)
    .union(WindowFlags::NO_COLLAPSE)
    .union(WindowFlags::ALWAYS_AUTO_RESIZE)
    .union(WindowFlags::NO_FOCUS_ON_APPEARING)
    .union(WindowFlags::NO_NAV);

pub fn render(ui: &Ui, snap: &UiSnapshot, state: &mut UiState) {
    if !snap.badge_visible() {
        return;
    }
    let settings = state.badge_settings(snap);
    draw(ui, snap.badge, snap.badge.unknown_suffix(), snap.flash, &settings, Some((state, &snap.badge_tooltip)));
}

/// Shown after a panic disabled the plugin; needs nothing from the worker.
pub fn render_error(ui: &Ui) {
    draw(ui, Badge::Error, None, false, &BadgeSettings::default(), None);
}

fn draw(ui: &Ui, badge: Badge, suffix: Option<String>, flash: bool, settings: &BadgeSettings, mut state: Option<(&mut UiState, &str)>) {
    let _form = theme::push_form(ui);
    let _bg = ui.push_style_color(StyleColor::WindowBg, theme::with_alpha(theme::GROUND, theme::ALPHA_HUD));
    let flags = if settings.lock_position { FLAGS | WindowFlags::NO_MOVE } else { FLAGS };
    let mut window = ui.window("##axigear-badge").flags(flags);
    if let Some(pos) = settings.pos {
        window = window.position(pos, Condition::FirstUseEver);
    }
    window.build(|| {
        ui.set_window_font_scale(super::clamp_scale(settings.scale));
        icons::draw(ui, badge.tone(), ui.text_line_height());
        ui.same_line();
        ui.text_colored(icons::ink(badge.tone()), badge.text());
        if let Some(suffix) = &suffix {
            ui.same_line_with_spacing(0.0, 0.0);
            ui.text_colored(theme::TEXT_FAINT, suffix);
        }
        let pos = ui.window_pos();
        if flash {
            let size = ui.window_size();
            let dl = ui.get_window_draw_list();
            axi::outline_on(&dl, Rect::new(pos, [pos[0] + size[0], pos[1] + size[1]]), theme::BORDER_CONTROL, theme::DANGER);
        }
        let Some((state, tooltip)) = state.as_mut() else { return };
        if ui.is_window_hovered() && !ui.is_mouse_down(MouseButton::Left) {
            ui.tooltip_text(*tooltip);
        }
        if ui.is_window_hovered() && ui.is_mouse_clicked(MouseButton::Left) {
            state.press_pos = Some(pos);
        }
        if ui.is_mouse_released(MouseButton::Left) {
            if state.press_pos.take() == Some(pos) {
                state.checklist_open = !state.checklist_open;
            }
        }
        if !ui.is_mouse_down(MouseButton::Left) && state.badge_pos != Some(pos) {
            if state.badge_pos.is_some() {
                state.edit_badge(BadgeSettings { pos: Some(pos), ..settings.clone() });
            }
            state.badge_pos = Some(pos);
        }
    });
}
