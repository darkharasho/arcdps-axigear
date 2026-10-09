//! API key field logic, host-testable: when an edit is saved, and what the
//! status line under the field says. The key itself is never formatted.

use axigear_core::driver::Command;

/// Commands for the key field: save when the trimmed edit differs from the
/// saved key, then test when asked.
pub fn commit_commands(edit: &str, saved: &str, test: bool) -> Vec<Command> {
    let mut out = Vec::new();
    if edit.trim() != saved.trim() {
        out.push(Command::SetApiKey(edit.to_string()));
    }
    if test {
        out.push(Command::TestKey);
    }
    out
}

/// The line under the field: a test result wins; else "Saved" when the
/// field matches a non-empty saved key. `true` = ok tone.
pub fn status(edit: &str, saved: &str, key_test: Option<&str>) -> Option<(String, bool)> {
    if let Some(t) = key_test {
        return Some((t.to_string(), t.starts_with("key ok")));
    }
    (!saved.trim().is_empty() && edit.trim() == saved.trim()).then(|| ("Saved".to_string(), true))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn is_set(c: &Command, k: &str) -> bool { matches!(c, Command::SetApiKey(x) if x == k) }

    #[test]
    fn leaving_the_field_saves_only_a_changed_key() {
        assert!(commit_commands(" NEW ", "OLD", false).iter().any(|c| is_set(c, " NEW ")));
        assert!(commit_commands("OLD", "OLD", false).is_empty());
        assert!(commit_commands(" OLD ", "OLD", false).is_empty(), "trim-equal is unchanged");
    }

    #[test]
    fn test_saves_first_then_tests() {
        let cmds = commit_commands("NEW", "OLD", true);
        assert_eq!(cmds.len(), 2);
        assert!(is_set(&cmds[0], "NEW"));
        assert!(matches!(cmds[1], Command::TestKey));
        assert!(matches!(commit_commands("OLD", "OLD", true).as_slice(), [Command::TestKey]));
    }

    #[test]
    fn status_line() {
        assert_eq!(status("", "", None), None);
        assert_eq!(status("K", "K", None), Some(("Saved".into(), true)));
        assert_eq!(status("K2", "K", None), None, "unsaved edit: say nothing yet");
        assert_eq!(status("K", "K", Some("key ok (main)")), Some(("key ok (main)".into(), true)));
        assert_eq!(status("K", "K", Some("HTTP 401")), Some(("HTTP 401".into(), false)));
    }
}
