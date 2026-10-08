//! Equipment tab: armor and weapons left; trinkets, infusions, consumables right.

use arcdps::imgui::Ui;
use axigear_core::loadout::{GearRow, Loadout, Tile};
use axigear_core::report::{CheckReport, SlotKey};

use super::axi::Rect;
use super::tile::{self, TileStyle};
use super::{icons, theme};

const ROW_ICON: f32 = 40.0;
const CHIP: f32 = 24.0;
const TRINKET: f32 = 32.0;
const ROW_H: f32 = 44.0;

pub fn render(ui: &Ui, l: &Loadout, report: &CheckReport, focus: Option<SlotKey>) {
    let o = ui.cursor_screen_pos();
    let avail = ui.content_region_avail()[0];
    let col_w = ((avail - 12.0) / 2.0).max(240.0);
    let left = column(ui, [o[0], o[1]], col_w, |ui, w| {
        eyebrow(ui, "Armor");
        for row in &l.armor {
            gear_row(ui, row, w, report, focus);
        }
        eyebrow(ui, "Weapons");
        for set in &l.weapons {
            ui.text_colored(theme::TEXT_FAINT, format!("SET {}", set.label));
            gear_row(ui, &set.main, w, report, focus);
            if set.two_handed {
                ui.text_colored(theme::TEXT_FAINT, "     Two-Handed");
            } else if let Some(off) = &set.off {
                gear_row(ui, off, w, report, focus);
            }
        }
    });
    let right = column(ui, [o[0] + col_w + 12.0, o[1]], col_w, |ui, w| {
        eyebrow(ui, "Trinkets");
        let split = l.trinkets.len().min(3);
        let first: Vec<&Tile> = l.trinkets[..split]
            .iter()
            .chain(std::iter::once(&l.relic))
            .collect();
        let second: Vec<&Tile> = l.trinkets[split..].iter().collect();
        for row in [first, second] {
            trinket_row(ui, &row, w, report, focus);
        }
        eyebrow(ui, "Infusions");
        let mut ring = Rect::new(ui.item_rect_min(), ui.item_rect_max());
        if let Some(t) = report.worst(SlotKey::Infusions, &[]) {
            ui.same_line();
            icons::draw(ui, t, ui.text_line_height());
            ring.max = ui.item_rect_max();
        }
        // Infusions have no tile of their own, so a problem click rings
        // the header and its status chip instead.
        if focus == Some(SlotKey::Infusions) {
            tile::pulse_ring(ui, ring);
        }
        if l.infusions.is_empty() {
            ui.text_colored(theme::TEXT_FAINT, "none set");
        } else {
            let start = ui.cursor_screen_pos();
            let per_row = ((w + 3.0) / (CHIP + 3.0)).floor().max(1.0) as usize;
            for (i, t) in l.infusions.iter().enumerate() {
                ui.set_cursor_screen_pos([
                    start[0] + (i % per_row) as f32 * (CHIP + 3.0),
                    start[1] + (i / per_row) as f32 * (CHIP + 3.0),
                ]);
                tile::icon(ui, t, None, TileStyle::new(CHIP));
            }
            let rows = l.infusions.len().div_ceil(per_row);
            ui.set_cursor_screen_pos([start[0], start[1] + rows as f32 * (CHIP + 3.0)]);
            ui.dummy([0.0, 0.0]);
        }
        eyebrow(ui, "Consumables");
        for t in [&l.food, &l.utility] {
            gear_row(
                ui,
                &GearRow {
                    tile: t.clone(),
                    upgrades: vec![],
                },
                w,
                report,
                focus,
            );
        }
    });
    ui.set_cursor_screen_pos([o[0], left.max(right)]);
    ui.dummy([avail, 0.0]);
}

/// Draws `f` with the cursor at `at`, inside a group so items lay out top-down; returns the bottom y.
fn column(ui: &Ui, at: [f32; 2], w: f32, f: impl FnOnce(&Ui, f32)) -> f32 {
    ui.set_cursor_screen_pos(at);
    let g = ui.begin_group();
    f(ui, w);
    g.end();
    ui.item_rect_max()[1]
}

fn eyebrow(ui: &Ui, s: &str) {
    ui.dummy([0.0, 4.0]);
    ui.text_colored(theme::GOLD, s.to_uppercase());
}

fn gear_row(ui: &Ui, row: &GearRow, w: f32, report: &CheckReport, focus: Option<SlotKey>) {
    let o = ui.cursor_screen_pos();
    tile::icon(
        ui,
        &row.tile,
        Some(report),
        TileStyle {
            pulse: focus == Some(row.tile.key),
            ..TileStyle::new(ROW_ICON)
        },
    );
    let chips_w = row.upgrades.len() as f32 * (CHIP + 3.0);
    let text_x = o[0] + ROW_ICON + 8.0;
    let text_w = (w - ROW_ICON - 8.0 - chips_w - 4.0).max(20.0);
    ui.set_cursor_screen_pos([text_x, o[1] + 2.0]);
    ui.text_colored(theme::TEXT_FAINT, row.tile.label.to_uppercase());
    ui.set_cursor_screen_pos([text_x, o[1] + 2.0 + ui.text_line_height()]);
    let name = if row.tile.empty {
        "-"
    } else {
        row.tile.name.as_str()
    };
    tile::clipped_text(
        ui,
        name,
        text_w,
        if row.tile.empty {
            theme::TEXT_FAINT
        } else {
            theme::TEXT
        },
    );
    if let Some(sub) = &row.tile.sub {
        ui.set_cursor_screen_pos([text_x, o[1] + 2.0 + 2.0 * ui.text_line_height()]);
        tile::clipped_text(ui, sub, text_w, theme::TEXT_FAINT);
    }
    for (i, u) in row.upgrades.iter().enumerate() {
        ui.set_cursor_screen_pos([
            o[0] + w - chips_w + i as f32 * (CHIP + 3.0),
            o[1] + (ROW_ICON - CHIP) / 2.0,
        ]);
        tile::icon(
            ui,
            u,
            Some(report),
            TileStyle {
                pulse: focus == Some(u.key),
                ..TileStyle::new(CHIP)
            },
        );
    }
    ui.set_cursor_screen_pos([
        o[0],
        o[1] + ROW_H.max(if row.tile.sub.is_some() {
            3.0 * ui.text_line_height() + 4.0
        } else {
            0.0
        }),
    ]);
    ui.dummy([0.0, 0.0]);
}

fn trinket_row(ui: &Ui, tiles: &[&Tile], w: f32, report: &CheckReport, focus: Option<SlotKey>) {
    let o = ui.cursor_screen_pos();
    let cell = w / 4.0;
    let line = ui.text_line_height();
    for (i, t) in tiles.iter().enumerate() {
        let x = o[0] + i as f32 * cell;
        ui.set_cursor_screen_pos([x + (cell - TRINKET) / 2.0, o[1]]);
        tile::icon(
            ui,
            t,
            Some(report),
            TileStyle {
                pulse: focus == Some(t.key),
                ..TileStyle::new(TRINKET)
            },
        );
        ui.set_cursor_screen_pos([x, o[1] + TRINKET + 2.0]);
        tile::clipped_text(ui, &t.label.to_uppercase(), cell - 4.0, theme::TEXT_FAINT);
        ui.set_cursor_screen_pos([x, o[1] + TRINKET + 2.0 + line]);
        tile::clipped_text(
            ui,
            if t.empty { "-" } else { &t.name },
            cell - 4.0,
            theme::TEXT_DIM,
        );
    }
    ui.set_cursor_screen_pos([o[0], o[1] + TRINKET + 4.0 + 2.0 * line + 6.0]);
    ui.dummy([0.0, 0.0]);
}
