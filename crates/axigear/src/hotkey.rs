//! Hotkey string parsing ("Ctrl+Shift+R") and matching.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hotkey {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub vk: u32,
}

impl Hotkey {
    pub fn parse(s: &str) -> Option<Self> {
        let mut hk = Hotkey { ctrl: false, shift: false, alt: false, vk: 0 };
        for part in s.split('+').map(|p| p.trim()) {
            match part.to_ascii_lowercase().as_str() {
                "" => return None,
                "ctrl" | "control" => hk.ctrl = true,
                "shift" => hk.shift = true,
                "alt" => hk.alt = true,
                key => match vk_from_key(key) {
                    Some(v) => hk.vk = v,
                    None => return None,
                },
            }
        }
        if hk.vk == 0 { None } else { Some(hk) }
    }
}

fn vk_from_key(s: &str) -> Option<u32> {
    if s.len() == 1 {
        let c = s.chars().next().unwrap();
        let cu = c.to_ascii_uppercase();
        if cu.is_ascii_alphanumeric() {
            return Some(cu as u32);
        }
        // Single-char punctuation maps back to its OEM VK code.
        if let Some(vk) = oem_punct_vk(c) { return Some(vk); }
    }
    // "VK0x<hex>" fallback for keys without a friendly name.
    if let Some(rest) = s.to_ascii_uppercase().strip_prefix("VK0X") {
        if let Ok(v) = u32::from_str_radix(rest, 16) {
            return Some(v);
        }
    }
    Some(match s.to_ascii_uppercase().as_str() {
        "F1"  => 0x70, "F2"  => 0x71, "F3"  => 0x72, "F4"  => 0x73,
        "F5"  => 0x74, "F6"  => 0x75, "F7"  => 0x76, "F8"  => 0x77,
        "F9"  => 0x78, "F10" => 0x79, "F11" => 0x7A, "F12" => 0x7B,
        "SPACE" => 0x20,
        "TAB" => 0x09,
        "ENTER" | "RETURN" => 0x0D,
        "ESC" | "ESCAPE" => 0x1B,
        "BACKSPACE" => 0x08,
        "DELETE" | "DEL" => 0x2E,
        "INSERT" | "INS" => 0x2D,
        "HOME" => 0x24,
        "END" => 0x23,
        "PAGEUP" | "PGUP" => 0x21,
        "PAGEDOWN" | "PGDN" => 0x22,
        "LEFT" => 0x25,
        "UP" => 0x26,
        "RIGHT" => 0x27,
        "DOWN" => 0x28,
        _ => return None,
    })
}

fn oem_punct_vk(c: char) -> Option<u32> {
    Some(match c {
        ';' | ':' => 0xBA,
        '=' | '+' => 0xBB,
        ',' | '<' => 0xBC,
        '-' | '_' => 0xBD,
        '.' | '>' => 0xBE,
        '/' | '?' => 0xBF,
        '`' | '~' => 0xC0,
        '[' | '{' => 0xDB,
        '\\' | '|' => 0xDC,
        ']' | '}' => 0xDD,
        '\'' | '"' => 0xDE,
        _ => return None,
    })
}

/// Returns true if the WM_KEYDOWN event matches the hotkey AND modifiers.
pub fn matches(hk: &Hotkey, vk: u32, ctrl: bool, shift: bool, alt: bool) -> bool {
    hk.vk == vk && hk.ctrl == ctrl && hk.shift == shift && hk.alt == alt
}

/// Format a captured keypress back into the canonical "Ctrl+Shift+R"
/// string used by parse(). Returns None for pure modifier keys (those
/// don't make valid hotkey targets on their own).
pub fn format_keypress(vk: u32, ctrl: bool, shift: bool, alt: bool) -> Option<String> {
    let label = vk_label(vk)?;
    let mut parts: Vec<&str> = Vec::new();
    if ctrl { parts.push("Ctrl"); }
    if shift { parts.push("Shift"); }
    if alt { parts.push("Alt"); }
    let mut out = parts.join("+");
    if !out.is_empty() { out.push('+'); }
    out.push_str(&label);
    Some(out)
}

fn vk_label(vk: u32) -> Option<String> {
    // Pure modifier keys can't be standalone hotkeys.
    const VK_CONTROL: u32 = 0x11;
    const VK_SHIFT: u32 = 0x10;
    const VK_MENU: u32 = 0x12;
    const VK_LCONTROL: u32 = 0xA2;
    const VK_RCONTROL: u32 = 0xA3;
    const VK_LSHIFT: u32 = 0xA0;
    const VK_RSHIFT: u32 = 0xA1;
    const VK_LMENU: u32 = 0xA4;
    const VK_RMENU: u32 = 0xA5;
    match vk {
        VK_CONTROL | VK_SHIFT | VK_MENU
        | VK_LCONTROL | VK_RCONTROL | VK_LSHIFT | VK_RSHIFT | VK_LMENU | VK_RMENU => return None,
        _ => {}
    }
    Some(match vk {
        0x30..=0x39 => char::from(vk as u8).to_string(),       // 0..9
        0x41..=0x5A => char::from(vk as u8).to_string(),       // A..Z
        0x70..=0x7B => format!("F{}", vk - 0x70 + 1),          // F1..F12
        0x20 => "Space".into(),
        0x09 => "Tab".into(),
        0x0D => "Enter".into(),
        0x1B => "Escape".into(),
        0x08 => "Backspace".into(),
        0x2E => "Delete".into(),
        0x2D => "Insert".into(),
        0x24 => "Home".into(),
        0x23 => "End".into(),
        0x21 => "PageUp".into(),
        0x22 => "PageDown".into(),
        // Arrow keys.
        0x25 => "Left".into(),
        0x26 => "Up".into(),
        0x27 => "Right".into(),
        0x28 => "Down".into(),
        // Numpad.
        0x60..=0x69 => format!("Num{}", vk - 0x60),
        0x6A => "NumMultiply".into(),
        0x6B => "NumAdd".into(),
        0x6D => "NumSubtract".into(),
        0x6E => "NumDecimal".into(),
        0x6F => "NumDivide".into(),
        // OEM punctuation (US layout). Conveys the unshifted glyph;
        // modifier flags are kept separately so Shift+` still renders
        // as "Shift+`" rather than "~".
        0xBA => ";".into(),
        0xBB => "=".into(),
        0xBC => ",".into(),
        0xBD => "-".into(),
        0xBE => ".".into(),
        0xBF => "/".into(),
        0xC0 => "`".into(),
        0xDB => "[".into(),
        0xDC => "\\".into(),
        0xDD => "]".into(),
        0xDE => "'".into(),
        // Unknown VK — still capture it so the bind succeeds and the
        // user can see exactly what they pressed (and report if a name
        // is missing).
        other => format!("VK0x{:02X}", other),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ctrl_shift_r() {
        let hk = Hotkey::parse("Ctrl+Shift+R").unwrap();
        assert!(hk.ctrl && hk.shift && !hk.alt);
        assert_eq!(hk.vk, 'R' as u32);
    }

    #[test]
    fn parses_f9() {
        let hk = Hotkey::parse("F9").unwrap();
        assert_eq!(hk.vk, 0x78);
        assert!(!hk.ctrl);
    }

    #[test]
    fn rejects_empty_key() {
        assert!(Hotkey::parse("Ctrl+Shift").is_none());
    }

    #[test]
    fn format_keypress_round_trips_through_parse() {
        let cases = [
            ("Ctrl+Shift+R", 'R' as u32, true, true, false),
            ("Alt+T", 'T' as u32, false, false, true),
            ("F9", 0x78, false, false, false),
            ("Ctrl+Space", 0x20, true, false, false),
        ];
        for (expected, vk, ctrl, shift, alt) in cases {
            let formatted = format_keypress(vk, ctrl, shift, alt).unwrap();
            assert_eq!(formatted, expected);
            let parsed = Hotkey::parse(&formatted).unwrap();
            assert!(matches(&parsed, vk, ctrl, shift, alt));
        }
    }

    #[test]
    fn captures_shift_backtick() {
        // VK_OEM_3 = 0xC0 is the backtick/tilde key. Common bind target;
        // the original `vk_label` table didn't have it and the capture
        // silently rejected it as a pure-modifier press.
        let formatted = format_keypress(0xC0, false, true, false).unwrap();
        assert_eq!(formatted, "Shift+`");
        let parsed = Hotkey::parse(&formatted).unwrap();
        assert_eq!(parsed.vk, 0xC0);
        assert!(parsed.shift);
    }

    #[test]
    fn unknown_vk_round_trips_via_hex_fallback() {
        // An exotic VK still captures so the bind succeeds.
        let formatted = format_keypress(0xFE, true, false, false).unwrap();
        assert_eq!(formatted, "Ctrl+VK0xFE");
        let parsed = Hotkey::parse(&formatted).unwrap();
        assert_eq!(parsed.vk, 0xFE);
        assert!(parsed.ctrl);
    }

    #[test]
    fn format_keypress_rejects_pure_modifiers() {
        // Pure modifier keys shouldn't bind to anything.
        assert!(format_keypress(0x11, false, false, false).is_none()); // Ctrl
        assert!(format_keypress(0x10, false, false, false).is_none()); // Shift
        assert!(format_keypress(0x12, false, false, false).is_none()); // Alt
    }

    #[test]
    fn matches_strictly() {
        let hk = Hotkey::parse("Ctrl+R").unwrap();
        assert!(matches(&hk, 'R' as u32, true, false, false));
        assert!(!matches(&hk, 'R' as u32, true, true, false)); // extra shift held
        assert!(!matches(&hk, 'X' as u32, true, false, false));
    }
}
