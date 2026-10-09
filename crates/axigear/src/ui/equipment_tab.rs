//! Equipment tab: fixed-width columns. Left: armor and weapon sets; right:
//! trinkets, relic, consumables. Each row: icon, name, upgrade names, and the
//! slot's infusion chips at the right edge.

use arcdps::imgui::Ui;
use axigear_core::loadout::{GearRow, Loadout, Tile};
use axigear_core::report::{CheckReport, SlotKey};

use super::axi;
use super::tile::{self, TileStyle};
use super::theme;

const COL_W: f32 = 340.0;
const GAP: f32 = 12.0;
const ROW_ICON: f32 = 40.0;
const TRINKET: f32 = 32.0;
const UPGRADE: f32 = 16.0;
const INF: f32 = 16.0;

pub fn render(ui: &Ui, l: &Loadout, report: &CheckReport, focus: Option<SlotKey>) {
    let o = ui.cursor_screen_pos();
    let avail = ui.content_region_avail()[0];
    let side_by_side = avail >= 2.0 * COL_W + GAP;
    let left_bottom = column(ui, o, |ui| {
        section(ui, "ARMOR");
        for row in &l.armor {
            gear_row(ui, row, report, focus, None);
        }
        for set in &l.weapons {
            section(ui, &format!("WEAPONS · SET {}", set.label));
            gear_row(ui, &set.main, report, focus, set.two_handed.then_some("two-handed"));
            if let Some(off) = &set.off {
                gear_row(ui, off, report, focus, None);
            }
        }
    });
    let right_at = if side_by_side { [o[0] + COL_W + GAP, o[1]] } else { [o[0], left_bottom + 8.0] };
    let right_bottom = column(ui, right_at, |ui| {
        section(ui, "TRINKETS");
        for row in &l.trinkets {
            small_row(ui, row, report, focus);
        }
        small_row(ui, &plain(&l.relic), report, focus);
        section(ui, "CONSUMABLES");
        for t in [&l.food, &l.utility] {
            small_row(ui, &plain(t), report, focus);
        }
    });
    ui.set_cursor_screen_pos([o[0], left_bottom.max(right_bottom)]);
    ui.dummy([avail.min(2.0 * COL_W + GAP), 0.0]);
}

fn plain(t: &Tile) -> GearRow {
    GearRow { tile: t.clone(), upgrades: vec![], infusions: vec![] }
}

/// Draws `f` with the cursor at `at`, inside a group; returns the bottom y.
fn column(ui: &Ui, at: [f32; 2], f: impl FnOnce(&Ui)) -> f32 {
    ui.set_cursor_screen_pos(at);
    let g = ui.begin_group();
    f(ui);
    g.end();
    ui.item_rect_max()[1]
}

/// Eyebrow label with a hairline rule under it, one column wide.
fn section(ui: &Ui, s: &str) {
    ui.dummy([0.0, 6.0]);
    axi::label(ui, s);
    let y = ui.item_rect_max()[1] + 2.0;
    let x = ui.item_rect_min()[0];
    axi::rule(ui, [x, y], [x + COL_W, y]);
    ui.dummy([0.0, 4.0]);
}

fn pulse(focus: Option<SlotKey>, key: SlotKey) -> bool {
    focus == Some(key) || (matches!(key, SlotKey::Infusion(_)) && focus == Some(SlotKey::Infusions))
}

/// Infusion chips right-aligned at the end of the row, vertically centred on `mid`.
fn infusion_chips(ui: &Ui, row: &GearRow, o: [f32; 2], mid: f32, report: &CheckReport, focus: Option<SlotKey>) -> f32 {
    let w = row.infusions.len() as f32 * (INF + 2.0);
    for (i, t) in row.infusions.iter().enumerate() {
        ui.set_cursor_screen_pos([o[0] + COL_W - w + i as f32 * (INF + 2.0), mid - INF / 2.0]);
        tile::icon(ui, t, Some(report), TileStyle { pulse: pulse(focus, t.key), ..TileStyle::new(INF) });
    }
    w
}

/// Armor or weapon: 40 px icon, name (+ tag), one line per rune/sigil with its icon.
fn gear_row(ui: &Ui, row: &GearRow, report: &CheckReport, focus: Option<SlotKey>, tag: Option<&str>) {
    let o = ui.cursor_screen_pos();
    let line = ui.text_line_height();
    let lines = 1 + row.upgrades.len().max(row.tile.sub.is_some() as usize);
    let h = ROW_ICON.max(lines as f32 * (line + 2.0));
    ui.set_cursor_screen_pos([o[0], o[1] + (h - ROW_ICON) / 2.0]);
    tile::icon(ui, &row.tile, Some(report), TileStyle { pulse: pulse(focus, row.tile.key), ..TileStyle::new(ROW_ICON) });
    let chips_w = infusion_chips(ui, row, o, o[1] + h / 2.0, report, focus);
    let text_x = o[0] + ROW_ICON + 8.0;
    let text_w = (COL_W - ROW_ICON - 8.0 - chips_w - 6.0).max(40.0);
    let top = o[1] + (h - lines as f32 * (line + 2.0)) / 2.0;
    ui.set_cursor_screen_pos([text_x, top]);
    let name = if row.tile.empty { "-".to_string() } else { match tag { Some(t) => format!("{} · {t}", row.tile.name), None => row.tile.name.clone() } };
    tile::clipped_text(ui, &name, text_w, if row.tile.empty { theme::TEXT_FAINT } else { theme::TEXT });
    for (i, u) in row.upgrades.iter().enumerate() {
        let y = top + (i + 1) as f32 * (line + 2.0);
        ui.set_cursor_screen_pos([text_x, y + (line - UPGRADE) / 2.0]);
        tile::icon(ui, u, Some(report), TileStyle { pulse: pulse(focus, u.key), ..TileStyle::new(UPGRADE) });
        ui.set_cursor_screen_pos([text_x + UPGRADE + 4.0, y]);
        tile::clipped_text(ui, &u.name, text_w - UPGRADE - 4.0, theme::TEXT_DIM);
    }
    if row.upgrades.is_empty() {
        if let Some(sub) = &row.tile.sub {
            ui.set_cursor_screen_pos([text_x, top + line + 2.0]);
            tile::clipped_text(ui, sub, text_w, theme::TEXT_FAINT);
        }
    }
    ui.set_cursor_screen_pos([o[0], o[1] + h + 4.0]);
    ui.dummy([0.0, 0.0]);
}

/// Trinket, relic, food or utility: 32 px icon centred on two lines (name, slot or buff).
fn small_row(ui: &Ui, row: &GearRow, report: &CheckReport, focus: Option<SlotKey>) {
    let o = ui.cursor_screen_pos();
    let line = ui.text_line_height();
    let h = TRINKET.max(2.0 * line + 2.0);
    ui.set_cursor_screen_pos([o[0], o[1] + (h - TRINKET) / 2.0]);
    tile::icon(ui, &row.tile, Some(report), TileStyle { pulse: pulse(focus, row.tile.key), ..TileStyle::new(TRINKET) });
    let chips_w = infusion_chips(ui, row, o, o[1] + h / 2.0, report, focus);
    let text_x = o[0] + TRINKET + 8.0;
    let text_w = (COL_W - TRINKET - 8.0 - chips_w - 6.0).max(40.0);
    let top = o[1] + (h - (2.0 * line + 2.0)) / 2.0;
    ui.set_cursor_screen_pos([text_x, top]);
    tile::clipped_text(ui, if row.tile.empty { "-" } else { &row.tile.name }, text_w, if row.tile.empty { theme::TEXT_FAINT } else { theme::TEXT });
    ui.set_cursor_screen_pos([text_x, top + line + 2.0]);
    let sub = row.tile.sub.clone().unwrap_or_else(|| row.tile.label.clone());
    tile::clipped_text(ui, &sub, text_w, theme::TEXT_DIM);
    ui.set_cursor_screen_pos([o[0], o[1] + h + 4.0]);
    ui.dummy([0.0, 0.0]);
}
