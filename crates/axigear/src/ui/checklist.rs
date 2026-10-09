//! The checklist window: comp/slot/API header, problems panel, then the
//! Build and Equipment tabs.

use arcdps::imgui::{Condition, StyleColor, Ui, WindowFlags};
use axigear_core::driver::{Command, SettingsPatch, UiSnapshot};
use axigear_core::report::Tab;

use super::state::UiState;
use super::theme;
use crate::plugin::send;

const PICKER: &str = "axigear-slot-picker";

pub fn render(ui: &Ui, snap: &UiSnapshot, state: &mut UiState) {
    if !state.checklist_open {
        return;
    }
    let _form = theme::push_form(ui);
    let _bg = ui.push_style_color(StyleColor::WindowBg, theme::GROUND);
    let _text = ui.push_style_color(StyleColor::Text, theme::TEXT);
    let mut open = true;
    ui.window("axigear")
        .opened(&mut open)
        .size([540.0, 640.0], Condition::FirstUseEver)
        .size_constraints([520.0, 200.0], [f32::MAX, f32::MAX])
        .flags(WindowFlags::NO_COLLAPSE)
        .build(|| {
            header(ui, snap);
            ui.separator();
            let (Some(report), Some(loadout)) = (&snap.report, &snap.loadout) else {
                ui.text_colored(
                    theme::TEXT_DIM,
                    snap.header.note.as_deref().unwrap_or(
                        "Load a comp in arcdps options (Alt+Shift+T) > Extensions > axigear.",
                    ),
                );
                return;
            };
            super::problems::render(ui, report, state);
            ui.separator();
            let tab = state.tab.unwrap_or(snap.settings.loadout_tab);
            let origin = ui.cursor_screen_pos();
            let (build_clicked, w) = super::axi::chip(
                ui,
                origin,
                "tab-build",
                "BUILD",
                tab == Tab::Build,
                theme::GOLD,
                [10.0, 4.0],
            );
            let (equip_clicked, _) = super::axi::chip(
                ui,
                [origin[0] + w + 6.0, origin[1]],
                "tab-equip",
                "EQUIPMENT",
                tab == Tab::Equipment,
                theme::GOLD,
                [10.0, 4.0],
            );
            for (clicked, t) in [(build_clicked, Tab::Build), (equip_clicked, Tab::Equipment)] {
                if clicked && t != tab {
                    state.tab = Some(t);
                    send(Command::Settings(SettingsPatch::LoadoutTab(t)));
                }
            }
            ui.dummy([0.0, 8.0]);
            let focus = state.focus.filter(|f| f.active(ui.time())).map(|f| f.key);
            match state.tab.unwrap_or(snap.settings.loadout_tab) {
                Tab::Build => super::build_tab::render(ui, loadout, report, focus),
                Tab::Equipment => super::equipment_tab::render(ui, loadout, report, focus),
            }
        });
    state.checklist_open = open;
}

fn header(ui: &Ui, snap: &UiSnapshot) {
    let h = &snap.header;
    if snap.comps.is_empty() {
        ui.text_colored(theme::TEXT_FAINT, "Comp: none");
    } else {
        ui.text("Comp:");
        ui.same_line();
        let names: Vec<&str> = snap.comps.iter().map(|r| r.name.as_str()).collect();
        let mut idx = snap.comps.iter().position(|r| r.active).unwrap_or(0);
        ui.set_next_item_width(220.0);
        if super::axi::combo(ui, "##axigear-comp-pick", &names, &mut idx, theme::GOLD) && !snap.comps[idx].active {
            send(Command::UseComp(snap.comps[idx].input.clone()));
        }
        if h.offline {
            ui.same_line();
            ui.text_colored(theme::WARN, "offline");
        }
        if h.source.starts_with("link") {
            ui.same_line();
            if ui.small_button("Refresh##comp") {
                if let Some(active) = &snap.settings.active_comp {
                    send(Command::RefreshComp(active.clone()));
                }
            }
        }
    }

    match (&h.slot_label, &h.note) {
        (Some(slot), _) => ui.text(format!("Slot: {slot}")),
        (None, Some(note)) => ui.text_colored(theme::WARN, note),
        (None, None) => {}
    }
    if !snap.picker.is_empty() {
        ui.same_line();
        if ui.small_button(if h.slot_label.is_some() {
            "Change slot"
        } else {
            "Pick slot"
        }) {
            ui.open_popup(PICKER);
        }
        ui.popup(PICKER, || {
            for opt in &snap.picker {
                let label = format!(
                    "{}{}##{}-{}-{}",
                    if opt.current { "> " } else { "" },
                    opt.label,
                    opt.slot.line,
                    opt.slot.slot,
                    opt.slot.build
                );
                if opt.enabled {
                    if ui.selectable(&label) {
                        send(Command::Pick(opt.slot));
                    }
                } else {
                    ui.text_disabled(&opt.label);
                }
            }
        });
    }

    if h.slot_label.is_some() || h.note.is_some() || !snap.picker.is_empty() {
        ui.same_line();
    }
    ui.text_colored(theme::TEXT_DIM, &h.api_line);
    ui.same_line();
    if ui.small_button("Refresh API") {
        send(Command::RefreshApi);
    }
}
