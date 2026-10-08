//! Small text helpers shared by checks and UI strings.

/// Case- and punctuation-insensitive key: "Berserker's" == "berserkers".
pub fn norm(s: &str) -> String {
    s.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

/// "18× +9 Concentration, 2× +9 Mighty", most common first, ties by name.
pub fn summarize(items: impl IntoIterator<Item = String>) -> String {
    let mut counts: std::collections::BTreeMap<String, usize> = Default::default();
    for item in items {
        *counts.entry(item).or_default() += 1;
    }
    if counts.is_empty() {
        return "none".into();
    }
    let mut rows: Vec<(String, usize)> = counts.into_iter().collect();
    rows.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    rows.iter().map(|(name, n)| format!("{n}× {name}")).collect::<Vec<_>>().join(", ")
}

/// "just now", "45s ago", "3m ago", "2h ago".
pub fn ago(secs: u64) -> String {
    match secs {
        0..=9 => "just now".into(),
        10..=59 => format!("{secs}s ago"),
        60..=3599 => format!("{}m ago", secs / 60),
        _ => format!("{}h ago", secs / 3600),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn norm_drops_case_and_punctuation() {
        assert_eq!(norm("Berserker's"), "berserkers");
        assert_eq!(norm("Peppercorn-Crusted Sous-Vide Steak"), "peppercorncrustedsousvidesteak");
    }

    #[test]
    fn summarize_counts_most_common_first() {
        let items = ["b", "a", "b", "c", "b", "a"].map(String::from);
        assert_eq!(summarize(items), "3× b, 2× a, 1× c");
        assert_eq!(summarize(Vec::<String>::new()), "none");
    }

    #[test]
    fn ago_buckets() {
        assert_eq!(ago(3), "just now");
        assert_eq!(ago(45), "45s ago");
        assert_eq!(ago(180), "3m ago");
        assert_eq!(ago(7300), "2h ago");
    }
}
