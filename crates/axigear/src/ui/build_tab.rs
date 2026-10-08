//! Build tab: skill bar, then a card per specialization line.

use arcdps::imgui::Ui;
use axigear_core::loadout::{Loadout, SpecCard, Tile};
use axigear_core::report::{Category, CheckReport, SlotKey, Status, Tone};

use super::axi::{self, Rect};
use super::tile::{self, TileStyle};
use super::{textures, theme};

const SKILL: f32 = 48.0;
const MAJOR: f32 = 32.0;
const MINOR: f32 = 26.0;
const EMBLEM: f32 = 56.0;
const CARD_H: f32 = 140.0;
const TITLE: f32 = 22.0;
const GAP: f32 = 10.0;

pub fn render(ui: &Ui, l: &Loadout, report: &CheckReport, focus: Option<SlotKey>) {
    eyebrow(ui, "Skills");
    let start = ui.cursor_screen_pos();
    for (i, t) in l.skills.iter().enumerate() {
        let x = start[0] + i as f32 * (SKILL + 8.0);
        ui.set_cursor_screen_pos([x, start[1]]);
        tile::icon(
            ui,
            t,
            Some(report),
            TileStyle {
                pulse: focus == Some(t.key),
                skip: &[Category::SkillsSeen],
                ..TileStyle::new(SKILL)
            },
        );
        let unseen = report
            .marks_for(t.key)
            .iter()
            .any(|(r, m)| r.category == Category::SkillsSeen && m.status == Status::Unknown);
        if unseen {
            let c = [x + SKILL / 2.0, start[1] + SKILL + 5.0];
            ui.get_window_draw_list()
                .add_circle(c, 2.0, theme::TEXT_FAINT)
                .filled(true)
                .build();
        }
    }
    ui.set_cursor_screen_pos([start[0], start[1] + SKILL + 14.0]);
    eyebrow(ui, "Specializations");
    for card in &l.specs {
        spec_card(ui, card, report, focus);
        ui.dummy([0.0, 8.0]);
    }
}

fn eyebrow(ui: &Ui, s: &str) {
    ui.text_colored(theme::GOLD, s.to_uppercase());
}

fn spec_card(ui: &Ui, c: &SpecCard, report: &CheckReport, focus: Option<SlotKey>) {
    let o = ui.cursor_screen_pos();
    let w = ui.content_region_avail()[0].max(320.0);
    let r = Rect::at(o, [w, CARD_H]);
    axi::card(ui, r, theme::SURFACE, false);
    {
        let draw = ui.get_window_draw_list();
        if let Some(bg) = c.background.as_deref().and_then(textures::get) {
            draw.add_image(bg.tex, r.min, r.max)
                .col(theme::with_alpha(theme::TINT_DIM, 0.35))
                .build();
        }
    }
    ui.set_cursor_screen_pos([o[0] + 8.0, o[1] + 6.0]);
    tile::clipped_text(ui, &c.name.to_uppercase(), w - 16.0, theme::GOLD);
    let mid = o[1] + TITLE + (CARD_H - TITLE) / 2.0;
    let mut x = o[0] + 8.0;
    // Emblem
    ui.set_cursor_screen_pos([x, mid - EMBLEM / 2.0]);
    let emblem = Tile {
        key: c.key,
        label: "Spec".into(),
        name: c.name.clone(),
        sub: None,
        icon: c.icon.clone(),
        empty: false,
    };
    tile::icon(
        ui,
        &emblem,
        Some(report),
        TileStyle {
            pulse: focus == Some(c.key),
            ..TileStyle::new(EMBLEM)
        },
    );
    x += EMBLEM + GAP;
    let col_h = 3.0 * MAJOR + 2.0 * 3.0;
    for tier in 0..3 {
        if let Some(minor) = c.minors.get(tier) {
            ui.set_cursor_screen_pos([x, mid - MINOR / 2.0]);
            tile::icon(ui, minor, None, TileStyle::new(MINOR));
        }
        x += MINOR + GAP;
        for (j, t) in c.majors[tier].iter().enumerate() {
            ui.set_cursor_screen_pos([x, mid - col_h / 2.0 + j as f32 * (MAJOR + 3.0)]);
            tile::icon(
                ui,
                &t.tile,
                (t.selected || c.any[tier]).then_some(report),
                TileStyle {
                    faded: !t.selected && !c.any[tier],
                    underline: t.selected,
                    pulse: focus == Some(t.tile.key) && t.selected,
                    ..TileStyle::new(MAJOR)
                },
            );
        }
        x += MAJOR + GAP;
    }
    if report.worst(c.key, &[]) == Some(Tone::Danger) {
        axi::outline_on(&ui.get_window_draw_list(), r, 3.0, theme::DANGER);
    }
    ui.set_cursor_screen_pos([o[0], o[1] + CARD_H]);
    ui.dummy([w, 0.0]);
}
