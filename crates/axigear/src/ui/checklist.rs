//! The checklist: comp/slot/API header, then results grouped by category,
//! failures first and expanded; all-passing categories fold into one line.

use arcdps::imgui::{Condition, StyleColor, TreeNodeFlags, Ui, WindowFlags};
use axigear_core::driver::{Command, UiSnapshot};
use axigear_core::report::{CheckReport, CheckResult, Source, Tone};
use axigear_core::text;

use super::state::UiState;
use super::{icons, theme};
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
        .size([480.0, 0.0], Condition::FirstUseEver)
        .flags(WindowFlags::NO_COLLAPSE | WindowFlags::ALWAYS_AUTO_RESIZE)
        .build(|| {
            header(ui, snap);
            ui.separator();
            match &snap.report {
                Some(report) => results(ui, report),
                None => ui.text_colored(
                    theme::TEXT_DIM,
                    snap.header.note.as_deref().unwrap_or("Load a comp in arcdps options (Alt+Shift+T) > Extensions > axigear."),
                ),
            }
        });
    state.checklist_open = open;
}

fn header(ui: &Ui, snap: &UiSnapshot) {
    let h = &snap.header;
    match &h.comp_name {
        Some(name) => {
            ui.text(format!("Comp: \"{name}\" ({})", h.source));
            if h.offline {
                ui.same_line();
                ui.text_colored(theme::WARN, "offline");
            }
            if h.source.starts_with("link") {
                ui.same_line();
                if ui.small_button("Refresh##comp") {
                    send(Command::RefreshComp);
                }
            }
        }
        None => ui.text_colored(theme::TEXT_FAINT, "Comp: none"),
    }

    match (&h.slot_label, &h.note) {
        (Some(slot), _) => ui.text(format!("Slot: {slot}")),
        (None, Some(note)) => ui.text_colored(theme::WARN, note),
        (None, None) => {}
    }
    if !snap.picker.is_empty() {
        ui.same_line();
        if ui.small_button(if h.slot_label.is_some() { "Change slot" } else { "Pick slot" }) {
            ui.open_popup(PICKER);
        }
        ui.popup(PICKER, || {
            for opt in &snap.picker {
                let label = format!("{}{}##{}-{}-{}", if opt.current { "> " } else { "" }, opt.label, opt.slot.line, opt.slot.slot, opt.slot.build);
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

    ui.text_colored(theme::TEXT_DIM, &h.api_line);
    ui.same_line();
    if ui.small_button("Refresh API") {
        send(Command::RefreshApi);
    }
}

fn results(ui: &Ui, report: &CheckReport) {
    let line = ui.text_line_height();
    let mut passing = Vec::new();
    for group in report.groups() {
        if group.tone == Tone::Ok {
            passing.push((group.category.label(), group.results.len()));
            continue;
        }
        icons::draw(ui, group.tone, line);
        ui.same_line();
        let title = format!("{} ({})##{:?}", group.category.label(), group.results.len(), group.category);
        let flags = if group.tone == Tone::Neutral { TreeNodeFlags::empty() } else { TreeNodeFlags::DEFAULT_OPEN };
        if ui.collapsing_header(title, flags) {
            for r in &group.results {
                row(ui, r);
            }
        }
    }
    if !passing.is_empty() {
        icons::draw(ui, Tone::Ok, line);
        ui.same_line();
        let n: usize = passing.iter().map(|(_, n)| n).sum();
        let names: Vec<&str> = passing.iter().map(|(name, _)| *name).collect();
        ui.text_colored(theme::TEXT_DIM, format!("{} ({n})", names.join(" · ")));
    }
}

fn row(ui: &Ui, r: &CheckResult) {
    ui.indent();
    icons::draw(ui, r.tone(), ui.text_line_height());
    ui.same_line();
    ui.text(&r.label);
    ui.same_line();
    ui.text_colored(theme::TEXT_DIM, r.detail());
    if ui.is_item_hovered() {
        let source = match r.source {
            Source::Live => "live (arcdps/MumbleLink)",
            Source::Api => "GW2 API",
        };
        let age = r.age_secs.map(|s| format!(" · {}", text::ago(s))).unwrap_or_default();
        ui.tooltip_text(format!("{source}{age}"));
    }
    ui.unindent();
}
