//! One tile: icon (or text fallback), ink outline, status chip, tooltip.

use arcdps::imgui::Ui;
use axigear_core::loadout::Tile;
use axigear_core::report::{Category, CheckReport, Source, Tone};
use axigear_core::text;

use super::axi::{self, Rect};
use super::{icons, textures, theme};

pub struct TileStyle {
    pub size: f32,
    pub faded: bool,
    pub underline: bool,
    pub pulse: bool,
    pub skip: &'static [Category],
}

impl TileStyle {
    pub fn new(size: f32) -> Self {
        TileStyle { size, faded: false, underline: false, pulse: false, skip: &[] }
    }
}

const CHIP: f32 = 14.0;

pub fn icon(ui: &Ui, t: &Tile, report: Option<&CheckReport>, style: TileStyle) -> bool {
    let origin = ui.cursor_screen_pos();
    let r = Rect::at(origin, [style.size, style.size]);
    ui.invisible_button(format!("##tile-{:?}-{}-{}", t.key, origin[0] as i32, origin[1] as i32), [style.size, style.size]);
    let hovered = ui.is_item_hovered();
    let tone = report.and_then(|rep| rep.worst(t.key, style.skip));
    let tex = t.icon.as_deref().and_then(textures::get);
    {
        let draw = ui.get_window_draw_list();
        draw.add_rect(r.min, r.max, theme::SURFACE_RAISED).filled(true).build();
        match tex {
            Some(h) => {
                let tint = if style.faded || t.empty { theme::TINT_DIM } else { theme::TINT_FULL };
                draw.add_image(h.tex, r.min, r.max).col(tint).build();
            }
            None => {
                let label = axi::truncate_to_width(if t.name.is_empty() { &t.label } else { &t.name }, style.size - 4.0, |s| ui.calc_text_size(s)[0]);
                draw.add_text([r.min[0] + 2.0, r.min[1] + 2.0], theme::TEXT_FAINT, label);
            }
        }
        let border = match tone {
            Some(Tone::Danger) => theme::DANGER,
            Some(Tone::Warn) => theme::WARN,
            _ if style.pulse => theme::GOLD,
            _ => theme::INK_LINE,
        };
        if t.empty {
            dashed(&draw, r, theme::RULE);
        } else {
            axi::outline_on(&draw, r, if tone.is_some_and(|x| x.rank() < 2) || style.pulse { theme::BORDER_CONTROL } else { theme::BORDER_HAIRLINE }, border);
        }
        if style.underline {
            draw.add_rect([r.min[0], r.max[1] + 2.0], [r.max[0], r.max[1] + 5.0], theme::GOLD).filled(true).build();
        }
    }
    if let Some(tone) = tone.filter(|t| *t != Tone::Ok) {
        ui.get_window_draw_list().add_rect([r.max[0] - CHIP, r.min[1]], [r.max[0], r.min[1] + CHIP], theme::INK_LINE).filled(true).build();
        icons::draw_at(ui, [r.max[0] - CHIP, r.min[1]], tone, CHIP);
    }
    if hovered {
        tooltip(ui, t, report);
    }
    hovered
}

fn dashed(draw: &arcdps::imgui::DrawListMut, r: Rect, ink: [f32; 4]) {
    let (dash, gap) = (4.0, 3.0);
    let mut x = r.min[0];
    while x < r.max[0] {
        let x2 = (x + dash).min(r.max[0]);
        draw.add_line([x, r.min[1]], [x2, r.min[1]], ink).thickness(1.0).build();
        draw.add_line([x, r.max[1]], [x2, r.max[1]], ink).thickness(1.0).build();
        x += dash + gap;
    }
    let mut y = r.min[1];
    while y < r.max[1] {
        let y2 = (y + dash).min(r.max[1]);
        draw.add_line([r.min[0], y], [r.min[0], y2], ink).thickness(1.0).build();
        draw.add_line([r.max[0], y], [r.max[0], y2], ink).thickness(1.0).build();
        y += dash + gap;
    }
}

pub fn tooltip(ui: &Ui, t: &Tile, report: Option<&CheckReport>) {
    ui.tooltip(|| {
        ui.text(if t.name.is_empty() { &t.label } else { &t.name });
        if t.empty {
            ui.text_colored(theme::TEXT_FAINT, "Not set in the comp");
        } else {
            ui.text_colored(theme::TEXT_DIM, format!("Comp: {}", t.name));
        }
        if let Some(sub) = &t.sub {
            ui.text_colored(theme::TEXT_FAINT, sub);
        }
        for (r, m) in report.map(|rep| rep.marks_for(t.key)).unwrap_or_default() {
            let source = match r.source {
                Source::Live => "live",
                Source::Api => "GW2 API",
            };
            let age = r.age_secs.map(|s| format!(" · {}", text::ago(s))).unwrap_or_default();
            let you = m.detail.as_deref().map(|d| format!(" · You: {d}")).unwrap_or_default();
            ui.text_colored(icons::ink(r.mark_tone(m)), format!("{}{you} ({source}{age})", r.label));
        }
    });
}

pub fn clipped_text(ui: &Ui, text: &str, w: f32, color: [f32; 4]) {
    let s = axi::truncate_to_width(text, w, |s| ui.calc_text_size(s)[0]);
    ui.text_colored(color, s);
}
