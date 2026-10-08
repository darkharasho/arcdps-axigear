//! The axi-design contract, enforced against the UI sources as TEXT.
//!
//! Reading source rather than calling it is what lets this run on Linux
//! while the modules it guards are `#[cfg(windows)]`. It is the parity
//! of `tests/renderer/tokens.test.ts` in the desktop app, and it is the
//! only thing that will stop colour literals creeping back one
//! convenience constant at a time.

use std::path::{Path, PathBuf};

/// Files allowed to hold a colour literal, and ONLY a colour literal.
/// `theme.rs` owns chrome; nothing else owns colours. It is still held to
/// the square-corner rule, which is why it is exempted inside the colour
/// test rather than dropped from the walk: `theme::push_form` pushes
/// eight rounding vars and every one of them must be zero.
const ALLOWED: [&str; 1] = ["src/ui/theme.rs"];

/// DEFERRED, NOT EXEMPT: surfaces knowingly left unconverted. Empty here;
/// do not add to it.
const DEFERRED: [&str; 0] = [];

/// Surfaces not yet converted. Each conversion task deletes its own
/// entry; the list reaching empty is the conversion being done. Unlike
/// DEFERRED this is temporary scaffolding — if you are reading this
/// after the branch merged and it is non-empty, something was skipped.
const PENDING: [&str; 0] = [];

/// Roots walked for guardable `.rs` files. Walking rather than listing
/// means a NEW ui file is guarded by default, which is the only way an
/// allowlist stays honest.
const ROOTS: [&str; 1] = ["src/ui"];

/// Individually guarded files outside the walked roots. None here.
const EXTRA_FILES: [&str; 0] = [];

/// Opt-out marker. A line carrying this is skipped, and the text after
/// it is the reason. Never add one without a reason a reviewer can
/// weigh.
const ALLOW_MARKER: &str = "axi-guard: allow";

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn rel(p: &Path) -> String {
    p.strip_prefix(repo_root())
        .unwrap_or(p)
        .to_string_lossy()
        .replace('\\', "/")
}

fn collect_rs(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", dir.display()));
    for entry in entries {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            collect_rs(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// Every file this test is responsible for right now.
fn guarded_files() -> Vec<PathBuf> {
    let root = repo_root();
    let mut found = Vec::new();
    for r in ROOTS {
        collect_rs(&root.join(r), &mut found);
    }
    for f in EXTRA_FILES {
        found.push(root.join(f));
    }
    found.sort();
    found.retain(|p| {
        let r = rel(p);
        !DEFERRED.contains(&r.as_str()) && !PENDING.contains(&r.as_str())
    });
    found
}

/// Is this a colour? Exactly four comma-separated components, at least
/// three of which parse as `f32`.
///
/// Exactly four rules out `[0.0, 4.0]` (a two-component imgui vec) and
/// `[x, y, w, h]` (an all-identifier rect). "At least three" (rather
/// than "all four") is what catches `[0.5, 0.5, 0.5, SOME_ALPHA_CONST]`
/// — a literal RGB triplet with a named alpha component is still a
/// colour literal wearing a disguise.
fn is_colour_literal(body: &str) -> bool {
    // rustfmt routinely leaves a trailing comma on a wrapped array
    // literal (`[\n    0.1, 0.2, 0.3, 1.0,\n]`); strip it before
    // splitting so that does not masquerade as a fifth, unparseable
    // component.
    let trimmed = body.trim().trim_end_matches(',');
    let parts: Vec<&str> = trimmed.split(',').map(str::trim).collect();
    if parts.len() != 4 {
        return false;
    }
    parts.iter().filter(|p| p.parse::<f32>().is_ok()).count() >= 3
}

/// A continuation or closing line of a `/* ... */` block: `*/`, a bare
/// `*`, or `* text`. Deliberately NOT any line merely starting with `*`
/// — that swallowed pointer-deref assignments such as
/// `*slot = [0.1, 0.2, 0.3, 1.0];`, which is real code the scanner must
/// see. A deref is always `*` immediately followed by an identifier,
/// `(`, `*` or `&`; a comment continuation always has whitespace, a `/`,
/// or nothing after its `*`.
fn is_block_comment_continuation(line: &str) -> bool {
    let t = line.trim_start();
    let Some(rest) = t.strip_prefix('*') else { return false };
    rest.is_empty() || rest.starts_with('/') || rest.starts_with(char::is_whitespace)
}

/// Comment and string content confuse every rule here, so lines that
/// are wholly a comment are skipped. A colour literal hiding inside a
/// doc comment is not a colour the plugin draws.
fn is_comment(line: &str) -> bool {
    let t = line.trim_start();
    t.starts_with("//") || t.starts_with("/*") || is_block_comment_continuation(line)
}

/// Blank out whole-line comments and lines carrying the opt-out marker,
/// preserving line count (and therefore line numbers) so downstream
/// scanning can still report an accurate line.
fn strip_ignored_lines(text: &str) -> String {
    text.lines()
        .map(|line| {
            if is_comment(line) || line.contains(ALLOW_MARKER) {
                String::new()
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// EVERY `[...]` body in the whole file at every nesting depth, paired
/// with the line on which its `[` opened. Tracking depth across the
/// entire text (rather than resetting per line) is what catches a colour
/// literal that rustfmt has wrapped across multiple lines, e.g.:
///
/// ```ignore
/// const CANARY: [f32; 4] = [
///     0.11, 0.22, 0.33, 1.0,
/// ];
/// ```
///
/// Nested bodies are emitted in their own right, not just the outermost
/// one. A palette written as an array OF colours —
/// `const PAL: [[f32; 4]; 2] = [[0.1, 0.2, 0.3, 1.0], [0.4, 0.5, 0.6, 1.0]];`
/// — presents its outer body as eight comma-separated parts, which the
/// colour rule (exactly four) rejects; each inner body is the four-part
/// literal that must be flagged. One frame per open bracket is how both
/// are seen.
///
/// An inner bracket's own `[` and `]` stay in the enclosing frame's text,
/// so an outer body never looks like a flattened list of its children's
/// components.
fn bracket_bodies(text: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    // (line the `[` opened on, body so far), innermost last.
    let mut stack: Vec<(usize, String)> = Vec::new();
    let mut line_no = 1usize;
    for ch in text.chars() {
        if ch == '\n' {
            line_no += 1;
        }
        match ch {
            '[' => {
                for frame in stack.iter_mut() {
                    frame.1.push('[');
                }
                stack.push((line_no, String::new()));
            }
            ']' => {
                if let Some((open_line, buf)) = stack.pop() {
                    out.push((open_line, buf));
                }
                for frame in stack.iter_mut() {
                    frame.1.push(']');
                }
            }
            _ => {
                for frame in stack.iter_mut() {
                    frame.1.push(ch);
                }
            }
        }
    }
    out
}

/// The argument text of every `rounding(` / `Rounding(` call on a line.
fn rounding_args(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < line.len() {
        if !line.is_char_boundary(i) {
            // Advancing one byte at a time can land inside a multi-byte
            // UTF-8 character (e.g. an em dash in a nearby string
            // literal); skip forward to the next real boundary rather
            // than slicing mid-character.
            i += 1;
            continue;
        }
        let rest = &line[i..];
        let hit = rest.starts_with("rounding(") || rest.starts_with("Rounding(");
        if !hit {
            i += 1;
            continue;
        }
        let open = i + rest.find('(').expect("just matched");
        let mut depth = 0usize;
        let mut end = None;
        for (off, ch) in line[open..].char_indices() {
            match ch {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        end = Some(open + off);
                        break;
                    }
                }
                _ => {}
            }
        }
        if let Some(close) = end {
            out.push(line[open + 1..close].trim().to_string());
            i = close + 1;
        } else {
            i = open + 1;
        }
    }
    out
}

/// Does this line call the hex-minting helper `rgb(...)`, e.g. via
/// `theme::rgb(...)` or a bare `rgb(...)`? Matched as a whole call name
/// (preceded by a non-identifier character or the start of the line) so
/// it does not fire on `with_alpha(`, which legitimately restates an
/// existing token's alpha rather than minting a new colour, nor on a
/// differently-named function that merely ends in `rgb`, such as
/// `parse_rgb(` or `srgb_to_linear(`'s hypothetical `to_rgb(` — `_` is
/// an identifier character too, so it must count as "still part of the
/// previous word" just like a letter or digit does.
fn contains_rgb_call(line: &str) -> bool {
    for (i, _) in line.match_indices("rgb(") {
        let prev_is_ident = line[..i]
            .chars()
            .next_back()
            .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_');
        if !prev_is_ident {
            return true;
        }
    }
    false
}

/// Scan one file's text for contract violations. Pure: takes source,
/// returns `(line, kind, detail)` per violation. Unit-tested below
/// against inline fixtures, so the scanner's own blind spots are
/// regression-tested rather than rediscovered by hand. `kind` is one of
/// `"colour"`, `"rgb"`, or `"rounding"`; callers decide which kinds
/// apply to a given file (e.g. `ALLOWED` exempts `"colour"` and
/// `"rgb"`, never `"rounding"`).
fn scan(text: &str) -> Vec<(usize, &'static str, String)> {
    let mut violations = Vec::new();

    for (n, line) in text.lines().enumerate() {
        if is_comment(line) || line.contains(ALLOW_MARKER) {
            continue;
        }
        for arg in rounding_args(line) {
            let square = arg == "0.0" || arg == "0" || arg == "0.0_f32";
            if !square {
                violations.push((n + 1, "rounding", format!("rounding({arg})")));
            }
        }
        if contains_rgb_call(line) {
            violations.push((
                n + 1,
                "rgb",
                "mints a colour via theme::rgb; chrome colours belong in theme.rs".to_string(),
            ));
        }
    }

    let stripped = strip_ignored_lines(text);
    for (line_no, body) in bracket_bodies(&stripped) {
        if is_colour_literal(&body) {
            let display = body.split_whitespace().collect::<Vec<_>>().join(" ");
            violations.push((line_no, "colour", format!("[{display}]")));
        }
    }

    violations
}

#[test]
fn no_colour_literal_lives_outside_theme() {
    let mut violations: Vec<String> = Vec::new();
    for path in guarded_files() {
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
        if ALLOWED.contains(&rel(&path).as_str()) {
            continue;
        }
        for (line, kind, detail) in scan(&text) {
            if kind == "colour" || kind == "rgb" {
                violations.push(format!("{}:{}: {}", rel(&path), line, detail));
            }
        }
    }
    assert!(
        violations.is_empty(),
        "colour literals belong in src/ui/theme.rs, nowhere else:\n  {}",
        violations.join("\n  ")
    );
}

#[test]
fn every_corner_is_square() {
    // --axi-radius and --axi-radius-sm are both 0 in the language, and
    // that is a contract rather than a default. A single missed
    // rounding var shows up as one softened widget, which is very hard
    // to spot in a screenshot.
    let mut violations: Vec<String> = Vec::new();
    for path in guarded_files() {
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
        for (line, kind, detail) in scan(&text) {
            if kind == "rounding" {
                violations.push(format!("{}:{}: {}", rel(&path), line, detail));
            }
        }
    }
    assert!(
        violations.is_empty(),
        "axi-design has square corners; rounding must be 0.0:\n  {}",
        violations.join("\n  ")
    );
}

#[test]
fn the_exclusion_lists_still_point_at_real_files() {
    // A path-based exclusion for a file that has been renamed or split
    // silently matches nothing, and the guard keeps passing over a file
    // nobody is guarding. This is the check that makes the lists rot
    // loudly instead of quietly.
    let root = repo_root();
    for group in [&DEFERRED[..], &PENDING[..], &ALLOWED[..]] {
        for p in group {
            assert!(
                root.join(p).is_file(),
                "{p} is listed in the guard's exclusions but does not exist; \
                 the file moved and the list did not"
            );
        }
    }
}

#[test]
fn the_guard_is_actually_guarding_something() {
    // If the walk, the roots, or the allowlists ever conspire to leave
    // nothing guarded, both rules above pass vacuously. This is the
    // canary for that.
    let files = guarded_files();
    assert!(
        files.len() >= 2,
        "only {} file(s) guarded — check ROOTS and the exclusion lists: {:?}",
        files.len(),
        files.iter().map(|p| rel(p)).collect::<Vec<_>>()
    );
}

// --- Scanner self-tests -----------------------------------------------
//
// Each of these is a bypass a reviewer demonstrated live against an
// earlier version of `scan`. They stay here so the scanner's own blind
// spots are regression-tested rather than rediscovered by hand next
// time someone touches it.

#[test]
fn scan_catches_multiline_colour_literal() {
    let src = "const CANARY: [f32; 4] = [\n    0.11, 0.22, 0.33, 1.0,\n];\n";
    let violations = scan(src);
    assert!(
        violations.iter().any(|(_, kind, _)| *kind == "colour"),
        "rustfmt wraps long array literals across lines; a colour split \
         across lines must still be caught: {violations:?}"
    );
}

#[test]
fn scan_catches_nested_colour_literal_in_a_palette_array() {
    // The outer body is eight components, so the "exactly four" rule
    // rejects it; only a per-depth walk ever presents the two inner
    // four-part literals to the colour rule. This was the fourth
    // documented bypass and the likeliest shape a real regression takes:
    // one array of inks rather than one ink.
    let src = "const PAL: [[f32; 4]; 2] = [[0.1, 0.2, 0.3, 1.0], [0.4, 0.5, 0.6, 1.0]];\n";
    let violations = scan(src);
    let colours: Vec<_> = violations.iter().filter(|(_, k, _)| *k == "colour").collect();
    assert_eq!(
        colours.len(),
        2,
        "both inks of a nested palette literal must be flagged: {violations:?}"
    );
}

#[test]
fn scan_scans_a_pointer_deref_assignment() {
    // `is_comment` used to treat any line starting with `*` as a
    // block-comment continuation, which swallowed real code. The two
    // shapes must be told apart, not lumped together.
    let src = "*slot = [0.1, 0.2, 0.3, 1.0];\n";
    let violations = scan(src);
    assert!(
        violations.iter().any(|(_, kind, _)| *kind == "colour"),
        "a deref assignment is code, not a comment continuation: {violations:?}"
    );
}

#[test]
fn scan_still_skips_block_comment_continuation_lines() {
    let src = "/*\n * const C: [f32; 4] = [0.1, 0.2, 0.3, 1.0];\n */\n";
    let violations = scan(src);
    assert!(
        violations.iter().all(|(_, kind, _)| *kind != "colour"),
        "a colour inside a block comment is not a colour the plugin \
         draws: {violations:?}"
    );
}

#[test]
fn scan_catches_rgb_helper_call() {
    let src = "fn sneaky() -> [f32; 4] { crate::ui::theme::rgb(0x12, 0x34, 0x56) }\n";
    let violations = scan(src);
    assert!(
        violations.iter().any(|(_, kind, _)| *kind == "rgb"),
        "theme::rgb mints a colour with no array literal at all and must \
         be caught outside ALLOWED files: {violations:?}"
    );
}

#[test]
fn scan_does_not_flag_a_function_merely_named_with_an_rgb_suffix() {
    let src = "fn parse_rgb(s: &str) -> [f32; 4] { parse_rgb(s) }\n\
               fn sneaky() -> [f32; 4] { rgb(0x12, 0x34, 0x56) }\n";
    let violations = scan(src);
    assert_eq!(
        violations
            .iter()
            .filter(|(_, kind, _)| *kind == "rgb")
            .count(),
        1,
        "parse_rgb's definition and call both end in `rgb(` but are not \
         the hex-minting helper — `_` counts as an identifier character \
         just like a letter or digit, so only the bare rgb(...) call on \
         the second line should be flagged: {violations:?}"
    );
}

#[test]
fn scan_does_not_flag_with_alpha() {
    let src = "let c = theme::ACCENT.with_alpha(0.5);\n";
    let violations = scan(src);
    assert!(
        violations.iter().all(|(_, kind, _)| *kind != "rgb"),
        "with_alpha restates an existing token's alpha and must not be \
         treated as colour-minting: {violations:?}"
    );
}

#[test]
fn scan_catches_mixed_named_alpha_colour() {
    let src = "const C: [f32; 4] = [0.5, 0.5, 0.5, SOME_ALPHA_CONST];\n";
    let violations = scan(src);
    assert!(
        violations.iter().any(|(_, kind, _)| *kind == "colour"),
        "a literal RGB triplet with a named alpha component is still a \
         colour literal: {violations:?}"
    );
}

#[test]
fn scan_does_not_flag_non_colour_arrays() {
    let src = "let rect = [x, y, w, h];\nlet point = [0.0, 4.0];\n";
    let violations = scan(src);
    assert!(
        violations.iter().all(|(_, kind, _)| *kind != "colour"),
        "an all-identifier rect and a two-component imgui vec are not \
         colours: {violations:?}"
    );
}

#[test]
fn scan_flags_nonzero_rounding_and_allows_zero() {
    let src = "a.rounding(4.0).build();\nb.rounding(0.0).build();\n";
    let violations = scan(src);
    let rounding: Vec<_> = violations.iter().filter(|(_, k, _)| *k == "rounding").collect();
    assert_eq!(
        rounding.len(),
        1,
        "expected exactly one non-zero rounding violation: {violations:?}"
    );
}

#[test]
fn rounding_args_does_not_panic_on_multibyte_characters_before_the_call() {
    // "€" is 3 bytes (U+20AC) and "🎉" is 4 bytes (U+1F389). The old
    // implementation walked the line one *byte* at a time and re-sliced
    // from that raw index on every step, so it would land inside one of
    // these characters' byte sequences and panic long before ever
    // reaching the real `rounding(` call further down the line.
    let line = "€🎉 a.rounding(4.0).build();";
    let args = rounding_args(line);
    assert_eq!(
        args,
        vec!["4.0".to_string()],
        "multi-byte characters ahead of the call must not disrupt \
         detection of the call's own argument: {args:?}"
    );
}

#[test]
fn scan_still_flags_a_genuine_rounding_violation_on_a_line_with_multibyte_text() {
    // Same hazard as above, but through the public `scan` entry point,
    // and pinning that a real violation is still detected — not just
    // that the line is scanned without panicking.
    let src = "ui.text(\"€🎉\"); a.rounding(5.0).build();\n";
    let violations = scan(src);
    assert!(
        violations
            .iter()
            .any(|(_, kind, detail)| *kind == "rounding" && detail == "rounding(5.0)"),
        "a multi-byte string literal earlier on the line must not mask a \
         genuine non-zero rounding violation later on it: {violations:?}"
    );
}

#[test]
fn the_conversion_is_complete() {
    // PENDING is scaffolding for the conversion branch, not a
    // permanent exemption list. If this fails, a surface was added to
    // it and never converted — the guard is passing over a file
    // nobody is guarding, which is the exact failure DEFERRED's
    // comment warns about.
    assert!(
        PENDING.is_empty(),
        "unconverted surfaces still in PENDING: {PENDING:?}"
    );
    // The converted surfaces are actually being walked.
    let guarded: Vec<String> = guarded_files().iter().map(|p| rel(p)).collect();
    for surface in [
        "src/ui/badge.rs",
        "src/ui/checklist.rs",
        "src/ui/icons.rs",
    ] {
        assert!(
            guarded.contains(&surface.to_string()),
            "{surface} is not being guarded; guarded set is {guarded:?}"
        );
    }
}

// --- Latin-1 string literals ------------------------------------------
//
// arcdps's ImGui font only carries Latin-1 glyphs; anything above U+00FF
// in a drawn string renders as `?`. Status icons are drawn, never glyphs.

/// `(line, char)` for each char above U+00FF inside a `"..."` literal.
/// Skips `//` comments (including doc comments) and `/* */` blocks; handles
/// escapes and char literals well enough for plain UI sources. Raw strings
/// are scanned as ordinary strings, which only over-reports.
/// Decode a `u{XXXX}` escape body (the text right after a backslash).
/// Returns the char and how many chars the body spans, or `None` if
/// this is not a well-formed unicode escape.
fn unicode_escape(rest: &[char]) -> Option<(char, usize)> {
    if rest.first() != Some(&'u') || rest.get(1) != Some(&'{') {
        return None;
    }
    let close = rest.iter().take(12).position(|&c| c == '}')?;
    let hex: String = rest[2..close].iter().filter(|&&c| c != '_').collect();
    let ch = char::from_u32(u32::from_str_radix(&hex, 16).ok()?)?;
    Some((ch, close + 1))
}

fn non_latin1_in_strings(text: &str) -> Vec<(usize, char)> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let (mut i, mut line) = (0usize, 1usize);
    let mut in_str = false;
    let mut in_block = false;
    while i < chars.len() {
        let c = chars[i];
        if c == '\n' {
            line += 1;
        }
        if in_block {
            if c == '*' && chars.get(i + 1) == Some(&'/') {
                in_block = false;
                i += 1;
            }
        } else if in_str {
            if c == '\\' {
                if chars.get(i + 1) == Some(&'\n') {
                    line += 1;
                }
                if let Some((esc, len)) = unicode_escape(&chars[i + 1..]) {
                    if (esc as u32) > 0xFF {
                        out.push((line, esc));
                    }
                    i += len - 1; // the shared `i += 1` below lands on the `}`
                }
                i += 1;
            } else if c == '"' {
                in_str = false;
            } else if (c as u32) > 0xFF {
                out.push((line, c));
            }
        } else if c == '/' && chars.get(i + 1) == Some(&'/') {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
            continue;
        } else if c == '/' && chars.get(i + 1) == Some(&'*') {
            in_block = true;
            i += 1;
        } else if c == '\'' && chars.get(i + 2) == Some(&'\'') {
            i += 2; // 'x' char literal, so a '"' inside it cannot open a string
        } else if c == '\'' && chars.get(i + 1) == Some(&'\\') {
            while i + 1 < chars.len() && chars[i + 1] != '\'' {
                i += 1;
            }
            i += 1;
        } else if c == '"' {
            in_str = true;
        }
        i += 1;
    }
    out
}

#[test]
fn ui_strings_are_latin_1() {
    let mut violations = Vec::new();
    for path in guarded_files() {
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
        for (line, ch) in non_latin1_in_strings(&text) {
            violations.push(format!("{}:{}: U+{:04X} {ch}", rel(&path), line, ch as u32));
        }
    }
    assert!(
        violations.is_empty(),
        "arcdps fonts lack glyphs above U+00FF; draw icons via ui::icons:\n  {}",
        violations.join("\n  ")
    );
}

#[test]
fn latin_1_scanner_flags_strings_and_ignores_comments() {
    let bad = "ui.text(\"ok \\\" \u{2713} done\");\nlet x = 1;\nui.text(\"\u{2014}\");\n";
    assert_eq!(non_latin1_in_strings(bad), vec![(1, '\u{2713}'), (3, '\u{2014}')]);
    let fine = "// \u{2713} comment\n/// doc \u{2014}\n/* \u{26A0} */\nlet a = \"caf\u{e9} \u{b7} 6\u{d7}\"; let q = '\"'; // \u{2717}\n";
    assert!(non_latin1_in_strings(fine).is_empty());
}

#[test]
fn latin_1_scanner_decodes_unicode_escapes() {
    // The old truncate_to_width body: an escaped U+2026 in a format string.
    let old = "let c = format!(\"{}\\u{2026}\", base.trim_end());\n";
    assert_eq!(non_latin1_in_strings(old), vec![(1, '\u{2026}')]);
    let bad = "ui.text(\"a\\u{1F600}\");\nui.text(\"\\u{20_14}x\\u{2713}\");\n";
    assert_eq!(
        non_latin1_in_strings(bad),
        vec![(1, '\u{1F600}'), (2, '\u{2014}'), (2, '\u{2713}')]
    );
    let fine =
        "// \\u{2026}\nlet a = \"\\u{00b7} \\u{d7} \\\\u{2026} \\n\"; let c = '\\u{2026}';\n";
    assert!(non_latin1_in_strings(fine).is_empty());
}
