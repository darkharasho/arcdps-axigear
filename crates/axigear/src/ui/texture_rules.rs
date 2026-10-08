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

#[cfg(test)]
mod tests {
    use super::*;

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
