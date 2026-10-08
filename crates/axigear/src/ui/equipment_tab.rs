//! Stub until the tab body lands.

use arcdps::imgui::Ui;
use axigear_core::loadout::Loadout;
use axigear_core::report::{CheckReport, SlotKey};

pub fn render(ui: &Ui, _l: &Loadout, _r: &CheckReport, _focus: Option<SlotKey>) {
    ui.text("...");
}
