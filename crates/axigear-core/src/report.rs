//! What a check run produces, and how it rolls up into the badge.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::model::{GearSlot, SlotRef};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Category {
    Spec,
    Specializations,
    Traits,
    SkillBar,
    SkillsSeen,
    Weapons,
    Stats,
    Runes,
    Sigils,
    Relic,
    Infusions,
    Food,
    Utility,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Severity {
    Required,
    Advisory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SeveritySetting {
    Required,
    Advisory,
    Off,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Status {
    Pass,
    Fail,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum SlotKey {
    Gear(GearSlot),
    Rune(GearSlot),
    Sigil(GearSlot, u8),
    Infusions,
    /// One wanted infusion item (Equipment tab chips); marked only when short.
    Infusion(u32),
    Skill(u8),               // 0 heal, 1..=3 utilities, 4 elite
    Trait { line: u8, tier: u8 }, // line = index into Build.specs (0..3); tier 0..3
    Spec(u8),                // line 0..3
    Relic,
    Food,
    Utility,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Tab {
    #[default]
    Build,
    Equipment,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlotMark {
    pub key: SlotKey,
    pub status: Status,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Source {
    Live,
    Api,
}

/// Colour role; the UI maps it to theme tokens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Neutral,
    Ok,
    Warn,
    Danger,
}

impl Category {
    pub const ALL: [Category; 13] = [
        Category::Spec, Category::Specializations, Category::Traits, Category::SkillBar,
        Category::SkillsSeen, Category::Weapons, Category::Stats, Category::Runes,
        Category::Sigils, Category::Relic, Category::Infusions, Category::Food, Category::Utility,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Category::Spec => "Spec",
            Category::Specializations => "Specializations",
            Category::Traits => "Traits",
            Category::SkillBar => "Skill bar",
            Category::SkillsSeen => "Skills seen",
            Category::Weapons => "Weapons",
            Category::Stats => "Stats",
            Category::Runes => "Runes",
            Category::Sigils => "Sigils",
            Category::Relic => "Relic",
            Category::Infusions => "Infusions",
            Category::Food => "Food",
            Category::Utility => "Utility",
        }
    }

    pub fn source(self) -> Source {
        match self {
            Category::Spec | Category::SkillsSeen | Category::Food | Category::Utility => Source::Live,
            _ => Source::Api,
        }
    }

    pub fn default_setting(self) -> SeveritySetting {
        match self {
            Category::SkillsSeen | Category::Infusions => SeveritySetting::Advisory,
            _ => SeveritySetting::Required,
        }
    }
}

impl SeveritySetting {
    pub const ALL: [SeveritySetting; 3] = [SeveritySetting::Required, SeveritySetting::Advisory, SeveritySetting::Off];

    pub fn label(self) -> &'static str {
        match self {
            SeveritySetting::Required => "Required",
            SeveritySetting::Advisory => "Advisory",
            SeveritySetting::Off => "Off",
        }
    }

    pub fn severity(self) -> Option<Severity> {
        match self {
            SeveritySetting::Required => Some(Severity::Required),
            SeveritySetting::Advisory => Some(Severity::Advisory),
            SeveritySetting::Off => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckResult {
    pub id: String,
    pub category: Category,
    /// Row title, e.g. "Sigils A" or "Traits: Zeal".
    pub label: String,
    pub severity: Severity,
    pub status: Status,
    pub expected: String,
    pub actual: Option<String>,
    /// Why the result is Unknown.
    pub reason: Option<String>,
    pub source: Source,
    /// Age of the data behind an API result.
    pub age_secs: Option<u64>,
    /// Shown as ⚠ whatever the category's severity (wrong food/utility).
    #[serde(skip)]
    pub forced_advisory: bool,
    /// Per-slot marks for this result.
    #[serde(default)]
    pub marks: Vec<SlotMark>,
}

impl CheckResult {
    pub fn new(category: Category, id: impl Into<String>, label: impl Into<String>, status: Status, expected: impl Into<String>) -> Self {
        CheckResult {
            id: id.into(),
            category,
            label: label.into(),
            severity: Severity::Required,
            status,
            expected: expected.into(),
            actual: None,
            reason: None,
            source: category.source(),
            age_secs: None,
            forced_advisory: false,
            marks: vec![],
        }
    }

    pub fn with_actual(mut self, actual: impl Into<String>) -> Self {
        self.actual = Some(actual.into());
        self
    }

    pub fn with_reason(mut self, reason: impl Into<String>) -> Self {
        self.reason = Some(reason.into());
        self
    }

    pub fn force_advisory(mut self) -> Self {
        self.forced_advisory = true;
        self
    }

    pub fn tone(&self) -> Tone {
        match (self.status, self.severity) {
            (Status::Pass, _) => Tone::Ok,
            (Status::Fail, Severity::Required) => Tone::Danger,
            (Status::Fail, Severity::Advisory) => Tone::Warn,
            (Status::Unknown, _) => Tone::Neutral,
        }
    }

    pub fn mark(mut self, key: SlotKey, status: Status, detail: Option<String>) -> Self {
        self.marks.push(SlotMark { key, status, detail });
        self
    }

    pub fn mark_tone(&self, m: &SlotMark) -> Tone {
        match (m.status, self.severity) {
            (Status::Pass, _) => Tone::Ok,
            (Status::Fail, Severity::Required) => Tone::Danger,
            (Status::Fail, Severity::Advisory) => Tone::Warn,
            (Status::Unknown, _) => Tone::Neutral,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Summary {
    pub pass: usize,
    pub fail_required: usize,
    pub fail_advisory: usize,
    pub unknown: usize,
}

impl Summary {
    pub fn of(results: &[CheckResult]) -> Summary {
        let mut s = Summary::default();
        for r in results {
            match (r.status, r.severity) {
                (Status::Pass, _) => s.pass += 1,
                (Status::Fail, Severity::Required) => s.fail_required += 1,
                (Status::Fail, Severity::Advisory) => s.fail_advisory += 1,
                (Status::Unknown, _) => s.unknown += 1,
            }
        }
        s
    }

    pub fn fails(&self) -> usize {
        self.fail_required + self.fail_advisory
    }

    /// Checks with an answer; Unknowns are neither pass nor fail.
    pub fn decided(&self) -> usize {
        self.pass + self.fails()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckReport {
    pub comp_key: String,
    pub comp_name: String,
    pub slot: SlotRef,
    pub slot_label: String,
    pub results: Vec<CheckResult>,
}

impl CheckReport {
    pub fn summary(&self) -> Summary {
        Summary::of(&self.results)
    }

    pub fn marks_for(&self, key: SlotKey) -> Vec<(&CheckResult, &SlotMark)> {
        self.results.iter().flat_map(|r| r.marks.iter().filter(move |m| m.key == key).map(move |m| (r, m))).collect()
    }

    pub fn worst(&self, key: SlotKey, skip: &[Category]) -> Option<Tone> {
        self.marks_for(key).into_iter().filter(|(r, _)| !skip.contains(&r.category)).map(|(r, m)| r.mark_tone(m)).min_by_key(|t| t.rank())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Badge {
    NoComp,
    PickSlot,
    Checked(Summary),
    Error,
}

impl Badge {
    pub fn tone(&self) -> Tone {
        match self {
            Badge::NoComp => Tone::Neutral,
            Badge::PickSlot => Tone::Warn,
            Badge::Error => Tone::Danger,
            Badge::Checked(s) if s.fail_required > 0 => Tone::Danger,
            Badge::Checked(s) if s.fail_advisory > 0 => Tone::Warn,
            Badge::Checked(s) if s.decided() == 0 => Tone::Neutral,
            Badge::Checked(_) => Tone::Ok,
        }
    }

    /// Latin-1 only; the UI draws the ✓/⚠/✗ icon next to it from `tone()`.
    pub fn text(&self) -> String {
        match self {
            Badge::NoComp => "axigear: no comp".into(),
            Badge::PickSlot => "axigear: pick slot".into(),
            Badge::Error => "axigear: error (see log)".into(),
            Badge::Checked(s) if s.fails() > 0 => format!("axigear {}", if s.fail_required > 0 { s.fails() } else { s.fail_advisory }),
            Badge::Checked(s) if s.decided() == 0 => "axigear: checking".into(),
            Badge::Checked(s) => format!("axigear {}/{}", s.pass, s.decided()),
        }
    }

    pub fn unknown_suffix(&self) -> Option<String> {
        match self {
            Badge::Checked(s) if s.unknown > 0 => Some(format!(" · {}?", s.unknown)),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Severities(pub BTreeMap<Category, SeveritySetting>);

impl Default for Severities {
    fn default() -> Self {
        Severities(Category::ALL.iter().map(|c| (*c, c.default_setting())).collect())
    }
}

impl Severities {
    pub fn get(&self, c: Category) -> SeveritySetting {
        self.0.get(&c).copied().unwrap_or(c.default_setting())
    }

    pub fn set(&mut self, c: Category, s: SeveritySetting) {
        self.0.insert(c, s);
    }
}

impl SlotKey {
    pub fn tab(self) -> Tab {
        match self {
            SlotKey::Skill(_) | SlotKey::Trait { .. } | SlotKey::Spec(_) => Tab::Build,
            _ => Tab::Equipment,
        }
    }
}

impl Tone {
    /// Sort key: worst first.
    pub fn rank(self) -> u8 {
        match self {
            Tone::Danger => 0,
            Tone::Warn => 1,
            Tone::Neutral => 2,
            Tone::Ok => 3,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Group<'a> {
    pub category: Category,
    /// Worst tone in the group.
    pub tone: Tone,
    pub results: Vec<&'a CheckResult>,
}

impl CheckReport {
    /// Results by category, failures first (fail, then warn, then unknown, then passing).
    pub fn groups(&self) -> Vec<Group<'_>> {
        let mut groups: Vec<Group> = Category::ALL
            .iter()
            .filter_map(|c| {
                let results: Vec<&CheckResult> = self.results.iter().filter(|r| r.category == *c).collect();
                let tone = results.iter().map(|r| r.tone()).min_by_key(|t| t.rank())?;
                Some(Group { category: *c, tone, results })
            })
            .collect();
        groups.sort_by_key(|g| g.tone.rank()); // stable: catalog order within a tone
        groups
    }
}

impl CheckResult {
    /// The row text after the label: "expected X · actual Y" or "X · why unknown".
    pub fn detail(&self) -> String {
        match (&self.status, &self.reason, &self.actual) {
            (Status::Unknown, Some(reason), _) if self.expected.is_empty() => reason.clone(),
            (Status::Unknown, Some(reason), _) => format!("{} · {reason}", self.expected),
            (_, _, Some(actual)) => format!("expected {} · actual {actual}", self.expected),
            _ => format!("expected {}", self.expected),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(status: Status, severity: Severity) -> CheckResult {
        CheckResult { severity, ..CheckResult::new(Category::Runes, "x", "x", status, "") }
    }

    #[test]
    fn badge_states() {
        let pass = Summary { pass: 14, ..Default::default() };
        assert_eq!((Badge::Checked(pass).text(), Badge::Checked(pass).tone()), ("axigear 14/14".into(), Tone::Ok));
        let warn = Summary { pass: 12, fail_advisory: 2, unknown: 2, ..Default::default() };
        assert_eq!((Badge::Checked(warn).text(), Badge::Checked(warn).tone()), ("axigear 2".into(), Tone::Warn));
        assert_eq!(Badge::Checked(warn).unknown_suffix().as_deref(), Some(" · 2?"));
        let fail = Summary { pass: 10, fail_required: 2, fail_advisory: 1, ..Default::default() };
        assert_eq!((Badge::Checked(fail).text(), Badge::Checked(fail).tone()), ("axigear 3".into(), Tone::Danger));
        let none = Summary { unknown: 5, ..Default::default() };
        assert_eq!((Badge::Checked(none).text(), Badge::Checked(none).tone()), ("axigear: checking".into(), Tone::Neutral));
        assert_eq!(Badge::NoComp.text(), "axigear: no comp");
        assert_eq!(Badge::PickSlot.tone(), Tone::Warn);
        assert_eq!(Badge::Error.text(), "axigear: error (see log)");
    }

    #[test]
    fn summary_counts_and_tones() {
        let results = [
            r(Status::Pass, Severity::Required),
            r(Status::Fail, Severity::Required),
            r(Status::Fail, Severity::Advisory),
            r(Status::Unknown, Severity::Required),
        ];
        let s = Summary::of(&results);
        assert_eq!((s.pass, s.fail_required, s.fail_advisory, s.unknown, s.decided()), (1, 1, 1, 1, 3));
        assert_eq!(results.map(|r| r.tone()), [Tone::Ok, Tone::Danger, Tone::Warn, Tone::Neutral]);
    }

    #[test]
    fn severity_defaults_and_serde() {
        let s = Severities::default();
        assert_eq!(s.get(Category::Runes), SeveritySetting::Required);
        assert_eq!(s.get(Category::SkillsSeen), SeveritySetting::Advisory);
        assert_eq!(s.get(Category::Infusions), SeveritySetting::Advisory);
        let partial: Severities = serde_json::from_str(r#"{"Food":"Off"}"#).unwrap();
        assert_eq!(partial.get(Category::Food), SeveritySetting::Off);
        assert_eq!(partial.get(Category::Runes), SeveritySetting::Required);
    }

    #[test]
    fn report_round_trips_for_v2_commander_reporting() {
        let report = CheckReport {
            comp_key: "link:e4369a53".into(),
            comp_name: "Tuesday Zerg".into(),
            slot: SlotRef { line: 0, slot: 0, build: 0 },
            slot_label: "Party 1 · Quickbrand".into(),
            results: vec![r(Status::Fail, Severity::Required).with_actual("4/6")],
        };
        let json = serde_json::to_string(&report).unwrap();
        assert_eq!(serde_json::from_str::<CheckReport>(&json).unwrap(), report);
    }

    #[test]
    fn groups_put_failures_first() {
        let mk = |cat, status, severity| CheckResult { severity, ..CheckResult::new(cat, format!("{cat:?}"), "x", status, "e") };
        let report = CheckReport {
            comp_key: "k".into(),
            comp_name: "c".into(),
            slot: SlotRef { line: 0, slot: 0, build: 0 },
            slot_label: "s".into(),
            results: vec![
                mk(Category::Spec, Status::Pass, Severity::Required),
                mk(Category::SkillsSeen, Status::Unknown, Severity::Advisory),
                mk(Category::Infusions, Status::Fail, Severity::Advisory),
                mk(Category::Runes, Status::Fail, Severity::Required),
                mk(Category::Runes, Status::Pass, Severity::Required),
            ],
        };
        let order: Vec<(Category, Tone, usize)> = report.groups().iter().map(|g| (g.category, g.tone, g.results.len())).collect();
        assert_eq!(
            order,
            vec![
                (Category::Runes, Tone::Danger, 2),
                (Category::Infusions, Tone::Warn, 1),
                (Category::SkillsSeen, Tone::Neutral, 1),
                (Category::Spec, Tone::Ok, 1),
            ]
        );
    }

    #[test]
    fn detail_text() {
        let pass = CheckResult::new(Category::Runes, "runes", "Runes", Status::Pass, "6× Monk").with_actual("6× Monk");
        assert_eq!(pass.detail(), "expected 6× Monk · actual 6× Monk");
        let unknown = CheckResult::new(Category::Relic, "relic", "Relic", Status::Unknown, "Relic of the Flock").with_reason("the API doesn't report the relic");
        assert_eq!(unknown.detail(), "Relic of the Flock · the API doesn't report the relic");
        let gated = CheckResult::new(Category::Stats, "api.Stats", "Stats", Status::Unknown, "").with_reason("needs API key (characters, builds)");
        assert_eq!(gated.detail(), "needs API key (characters, builds)");
    }

    fn report_with(results: Vec<CheckResult>) -> CheckReport {
        CheckReport { comp_key: "k".into(), comp_name: "c".into(), slot: SlotRef { line: 0, slot: 0, build: 0 }, slot_label: "s".into(), results }
    }

    #[test]
    fn marks_follow_status_and_severity() {
        let mut r = CheckResult::new(Category::Runes, "runes", "Runes", Status::Fail, "x")
            .mark(SlotKey::Rune(GearSlot::Head), Status::Pass, None)
            .mark(SlotKey::Rune(GearSlot::Feet), Status::Fail, Some("Scholar".into()));
        assert_eq!(r.mark_tone(&r.marks[1]), Tone::Danger);
        r.severity = Severity::Advisory;
        assert_eq!(r.mark_tone(&r.marks[1]), Tone::Warn);
        assert_eq!(r.mark_tone(&r.marks[0]), Tone::Ok);
    }

    #[test]
    fn worst_mark_wins_and_skip_filters_categories() {
        let k = SlotKey::Gear(GearSlot::WeaponA1);
        let a = CheckResult::new(Category::Weapons, "weapons.A", "Weapons A", Status::Pass, "x").mark(k, Status::Pass, None);
        let b = CheckResult::new(Category::Stats, "stats", "Stats", Status::Fail, "x").mark(k, Status::Fail, Some("Rampager's".into()));
        let rep = report_with(vec![a, b]);
        assert_eq!(rep.worst(k, &[]), Some(Tone::Danger));
        assert_eq!(rep.worst(k, &[Category::Stats]), Some(Tone::Ok));
        assert_eq!(rep.marks_for(k).len(), 2);
        assert_eq!(rep.worst(SlotKey::Relic, &[]), None);
    }

    #[test]
    fn keys_know_their_tab() {
        assert_eq!(SlotKey::Skill(0).tab(), Tab::Build);
        assert_eq!(SlotKey::Trait { line: 1, tier: 2 }.tab(), Tab::Build);
        assert_eq!(SlotKey::Spec(2).tab(), Tab::Build);
        assert_eq!(SlotKey::Rune(GearSlot::Head).tab(), Tab::Equipment);
        assert_eq!(SlotKey::Food.tab(), Tab::Equipment);
    }
}
