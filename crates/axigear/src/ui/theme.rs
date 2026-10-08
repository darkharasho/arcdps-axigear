//! The axi-design chrome layer: surface and text ramp, the eleven
//! accents, and the form's two weight steps.
//!
//! This is the ONLY file in the crate permitted a chrome colour
//! literal — the same rule `tokens.css` follows in the web package.
//!
//! Everything but `push_form` compiles on the host, so the token
//! values and the accent lookup are unit-tested on Linux without GW2.

/// Convert 8-bit channels to imgui's normalized RGBA at full alpha.
///
/// The tokens below are written as hex so they stay diffable by eye
/// against the design package's `accents.json` and `tokens.css`;
/// `[0.133, 0.094, 0.114]` is not reviewable.
pub const fn rgb(r: u8, g: u8, b: u8) -> [f32; 4] {
    [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0]
}

/// Restate a colour at a different alpha. For fills against `ALPHA_HUD`
/// — ink is never scaled by that constant. The one exception is a
/// whole surface dissolving as a unit (`notifier.rs`'s toast fade),
/// where the ink is carried along with the surface leaving, not drawn
/// dim on its own.
pub const fn with_alpha(c: [f32; 4], a: f32) -> [f32; 4] {
    [c[0], c[1], c[2], a]
}

// --- the surface and text ramp -------------------------------------------

pub const GROUND: [f32; 4]         = rgb(0x15, 0x18, 0x1d); // --axi-ground
pub const SURFACE: [f32; 4]        = rgb(0x22, 0x27, 0x31); // --axi-surface
pub const SURFACE_RAISED: [f32; 4] = rgb(0x2b, 0x31, 0x3d); // --axi-surface-raised
pub const INK_LINE: [f32; 4]       = rgb(0x0c, 0x0e, 0x12); // --axi-ink-line
pub const RULE: [f32; 4]           = rgb(0x3a, 0x42, 0x50); // --axi-rule
pub const TEXT: [f32; 4]           = rgb(0xf4, 0xf6, 0xf9); // --axi-text
pub const TEXT_DIM: [f32; 4]       = rgb(0xa7, 0xb0, 0xbe); // --axi-text-dim
pub const TEXT_FAINT: [f32; 4]     = rgb(0x7c, 0x86, 0x95); // --axi-text-faint
pub const META: [f32; 4]           = rgb(0x4e, 0xc3, 0xff); // --axi-meta, meta only
pub const OK: [f32; 4]             = rgb(0x2f, 0xd3, 0x8a); // --axi-ok
pub const WARN: [f32; 4]           = rgb(0xff, 0x7a, 0x2f); // --axi-warn
pub const DANGER: [f32; 4]         = rgb(0xff, 0x52, 0x52); // --axi-danger

/// Pushed as `WindowBg` so imgui paints nothing and we own the fill.
/// Not a colour in the language's sense — the absence of one.
pub const TRANSPARENT: [f32; 4] = [0.0, 0.0, 0.0, 0.0];

/// Text drawn on an accent fill. `--axi-accent-ink` resolves to
/// `--axi-ink-line`, and every accent below is bright enough on this
/// ground for near-black to be right.
pub const ACCENT_INK: [f32; 4] = INK_LINE;

// --- the eleven accents --------------------------------------------------

/// `(id, label, value)` for each accent in the design package's
/// `accents.json`, in its order. Carried by value rather than read from
/// a shared file so the plugin and the desktop app agree without the
/// plugin depending on a path that differs between Windows and Wine.
pub const ACCENTS: [(&str, &str, [f32; 4]); 11] = [
    ("axi-gold",      "Axi Gold",      rgb(0xff, 0xc5, 0x3d)),
    ("electric-blue", "Electric Blue", rgb(0x3b, 0x82, 0xf6)),
    ("refined-cyan",  "Refined Cyan",  rgb(0x5e, 0xad, 0xd5)),
    ("amber-warm",    "Amber Warm",    rgb(0xf5, 0x9e, 0x0b)),
    ("emerald-mint",  "Emerald Mint",  rgb(0x34, 0xd3, 0x99)),
    ("rose-pink",     "Rose Pink",     rgb(0xf4, 0x3f, 0x5e)),
    ("violet-purple", "Violet Purple", rgb(0x8b, 0x5c, 0xf6)),
    ("crimson-red",   "Crimson Red",   rgb(0xef, 0x44, 0x44)),
    ("slate-silver",  "Slate Silver",  rgb(0x94, 0xa3, 0xb8)),
    ("teal-ocean",    "Teal Ocean",    rgb(0x14, 0xb8, 0xa6)),
    ("gold-bronze",   "Gold Bronze",   rgb(0xd4, 0xa0, 0x17)),
];

pub const DEFAULT_ACCENT_ID: &str = "emerald-mint";

/// Position of `id` in `ACCENTS`, for the options combo. An unknown id
/// resolves to the default's position.
pub fn accent_index(id: &str) -> usize {
    ACCENTS
        .iter()
        .position(|(k, _, _)| *k == id)
        .unwrap_or_else(|| {
            ACCENTS
                .iter()
                .position(|(k, _, _)| *k == DEFAULT_ACCENT_ID)
                .unwrap_or(0)
        })
}

/// Resolve the configured accent. Matching is exact: an id that differs
/// in case or whitespace is not a near miss to be repaired, it is a
/// config we did not write. An unknown id — hand-edited, or left by a
/// downgrade — falls back to the default rather than panicking: this
/// runs inside a render callback, and a panic there takes GW2's frame
/// with it.
pub fn accent(id: &str) -> [f32; 4] {
    ACCENTS[accent_index(id)].2
}

/// Display labels in `ACCENTS` order, for `combo_simple_string`.
pub fn accent_labels() -> Vec<&'static str> {
    ACCENTS.iter().map(|(_, label, _)| *label).collect()
}

// --- form geometry ------------------------------------------------------

/// Every border and offset below is this times its token value. If the
/// form reads too heavy at your GW2 UI scale, change this and nothing
/// else. Never tune the two steps independently: the point of two steps
/// is that a panel outweighs a control, and independent tuning destroys
/// that relationship.
pub const SCALE: f32 = 1.0;

/// The unscaled token values as `(panel, control)` pairs. The shipped
/// consts below are these times `SCALE`. They are public because the
/// host test walks candidate scales over them to prove the two steps
/// stay ordered for any positive scale — something the flat consts
/// alone cannot express.
pub const TOKEN_BORDERS: (f32, f32)       = (4.0, 3.0);  // --axi-border-panel / -control
pub const TOKEN_OFFSETS: (f32, f32)       = (6.0, 3.0);  // --axi-offset-panel / -control
pub const TOKEN_OFFSETS_HOVER: (f32, f32) = (10.0, 6.0); // --axi-offset-*-hover

pub const BORDER_PANEL: f32   = TOKEN_BORDERS.0 * SCALE;
pub const BORDER_CONTROL: f32 = TOKEN_BORDERS.1 * SCALE;
pub const OFFSET_PANEL: f32   = TOKEN_OFFSETS.0 * SCALE;
pub const OFFSET_CONTROL: f32 = TOKEN_OFFSETS.1 * SCALE;
pub const OFFSET_PANEL_HOVER: f32   = TOKEN_OFFSETS_HOVER.0 * SCALE;
pub const OFFSET_CONTROL_HOVER: f32 = TOKEN_OFFSETS_HOVER.1 * SCALE;

/// Prose-rule weight ONLY — `--axi-border-hairline`. Never a panel or
/// control outline; those are the two steps above.
pub const BORDER_HAIRLINE: f32 = 2.0 * SCALE;

// --- opacity ------------------------------------------------------------

/// Reading surfaces (shell, Pulse, Timeline) fill solid. You open these
/// to read them, so the language lands at full strength.
pub const ALPHA_READING: f32 = 1.0;

/// HUD surfaces (team bar, notifier) are small, always-on, and sit over
/// gameplay rather than being opened and read. Here translucency does
/// real work. Matches the team bar's pre-conversion value.
pub const ALPHA_HUD: f32 = 0.82;

// --- style vars ---------------------------------------------------------

/// Push the axi form's style vars: zero rounding on every corner imgui
/// can round, and no imgui-drawn borders — we draw our own outlines.
/// Returns the tokens; drop them to pop.
///
/// All eight rounding vars are covered deliberately. A missed one shows
/// up as a single softened widget, which is very hard to spot in a
/// screenshot and trivially easy to leave in.
#[cfg(windows)]
pub fn push_form<'ui>(
    ui: &'ui arcdps::imgui::Ui,
) -> Vec<arcdps::imgui::StyleStackToken<'ui>> {
    use arcdps::imgui::StyleVar;
    vec![
        ui.push_style_var(StyleVar::WindowRounding(0.0)),
        ui.push_style_var(StyleVar::ChildRounding(0.0)),
        ui.push_style_var(StyleVar::PopupRounding(0.0)),
        ui.push_style_var(StyleVar::FrameRounding(0.0)),
        ui.push_style_var(StyleVar::ScrollbarRounding(0.0)),
        ui.push_style_var(StyleVar::GrabRounding(0.0)),
        ui.push_style_var(StyleVar::ImageRounding(0.0)),
        ui.push_style_var(StyleVar::TabRounding(0.0)),
        ui.push_style_var(StyleVar::WindowBorderSize(0.0)),
        ui.push_style_var(StyleVar::ChildBorderSize(0.0)),
        ui.push_style_var(StyleVar::PopupBorderSize(0.0)),
        ui.push_style_var(StyleVar::FrameBorderSize(0.0)),
    ]
}
