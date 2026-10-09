//! Which icon URLs we fetch and where they're cached. Host-testable.

pub const HOSTS: [&str; 2] = ["https://render.guildwars2.com/", "https://wiki.guildwars2.com/"];

pub fn allowed(url: &str) -> bool {
    HOSTS.iter().any(|h| url.starts_with(h) && url.len() > h.len())
}

/// FNV-1a 64: stable across runs and Rust versions (unlike DefaultHasher).
pub fn cache_name(url: &str) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in url.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{h:016x}.img")
}

/// Where the art sits in a specialization background, in texture
/// pixels. The API serves a 1024x256 texture with the 647x136 panel
/// art in its lower-left corner; the rest is padding.
const SPEC_TEX: [f32; 2] = [1024.0, 256.0];
const SPEC_ART_MIN: [f32; 2] = [0.0, 120.0];
const SPEC_ART_MAX: [f32; 2] = [647.0, 256.0];

/// UVs that fill a `card`-sized rect with the art region of a spec
/// background without stretching it: the art is cropped to the card's
/// aspect, keeping its left edge (where the emblem art is) and its
/// bottom edge (where the panel's ground line is).
pub fn spec_background_uv(card: [f32; 2]) -> ([f32; 2], [f32; 2]) {
    let ([x0, y0], [x1, y1]) = (SPEC_ART_MIN, SPEC_ART_MAX);
    let (art_w, art_h) = (x1 - x0, y1 - y0);
    let aspect = if card[1] > 0.0 { card[0] / card[1] } else { art_w / art_h };
    let (w, h) = if aspect < art_w / art_h { (art_h * aspect, art_h) } else { (art_w, art_w / aspect) };
    (
        [x0 / SPEC_TEX[0], (y1 - h) / SPEC_TEX[1]],
        [(x0 + w) / SPEC_TEX[0], y1 / SPEC_TEX[1]],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: [f32; 2], b: [f32; 2]) -> bool { (a[0] - b[0]).abs() < 1e-4 && (a[1] - b[1]).abs() < 1e-4 }

    #[test]
    fn spec_background_uses_only_the_art_region() {
        // A card with the art's own aspect maps to exactly the art rect.
        let (min, max) = spec_background_uv([647.0, 136.0]);
        assert!(near(min, [0.0, 120.0 / 256.0]), "{min:?}");
        assert!(near(max, [647.0 / 1024.0, 1.0]), "{max:?}");
    }

    #[test]
    fn spec_background_crops_to_a_narrower_card_from_the_left() {
        // 500x140 is narrower than the art: full art height, left part of its width.
        let (min, max) = spec_background_uv([500.0, 140.0]);
        assert!(near(min, [0.0, 120.0 / 256.0]), "{min:?}");
        assert!(near(max, [136.0 * 500.0 / 140.0 / 1024.0, 1.0]), "{max:?}");
    }

    #[test]
    fn spec_background_crops_to_a_wider_card_from_the_bottom() {
        // 1000x140 is wider than the art: full art width, bottom part of its height.
        let (min, max) = spec_background_uv([1000.0, 140.0]);
        assert!(near(min, [0.0, (256.0 - 647.0 * 140.0 / 1000.0) / 256.0]), "{min:?}");
        assert!(near(max, [647.0 / 1024.0, 1.0]), "{max:?}");
    }

    #[test]
    fn only_gw2_hosts_are_allowed() {
        assert!(allowed("https://render.guildwars2.com/file/ABC/1.png"));
        assert!(allowed("https://wiki.guildwars2.com/images/b/b5/Bandit_Cleaver.png"));
        assert!(!allowed("http://render.guildwars2.com/file/ABC/1.png"));
        assert!(!allowed("https://render.guildwars2.com/"));
        assert!(!allowed(""));
        assert!(!allowed("https://evil.example/render.guildwars2.com/x.png"));
        assert!(!allowed("https://render.guildwars2.com.evil.example/x.png"));
    }

    #[test]
    fn cache_names_are_stable_and_distinct() {
        assert_eq!(cache_name("a"), "af63dc4c8601ec8c.img");
        assert_ne!(cache_name("https://render.guildwars2.com/1"), cache_name("https://render.guildwars2.com/2"));
    }
}
