//! Bundled food/utility buff IDs (scripts/gen-consumables.py, from Elite
//! Insights) and how AxiForge's labels map onto them.

use std::collections::HashMap;

use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};

use crate::text::norm;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConsumableKind {
    Food,
    Utility,
}

#[derive(Deserialize)]
struct Row {
    id: u32,
    kind: ConsumableKind,
    name: String,
}

pub struct Consumables {
    by_id: HashMap<u32, (ConsumableKind, String)>,
}

/// AxiForge labels whose buff is named differently in game.
const ALIASES: [(&str, &str); 2] = [
    ("Plate of Truffle Steak Dinner", "Plate of Truffle Steak"),
    ("Bowl of Fancy Potato and Leek Soup", "Bowl of Fancy Potatoe and Leek Soup"),
];

static BUNDLED: Lazy<Consumables> = Lazy::new(|| {
    let rows: Vec<Row> = serde_json::from_str(include_str!("../data/consumables.json")).expect("bundled consumables.json");
    Consumables { by_id: rows.into_iter().map(|r| (r.id, (r.kind, r.name))).collect() }
});

impl Consumables {
    pub fn bundled() -> &'static Consumables {
        &BUNDLED
    }

    pub fn kind_of(&self, buff_id: u32) -> Option<ConsumableKind> {
        self.by_id.get(&buff_id).map(|(k, _)| *k)
    }

    pub fn name_of(&self, buff_id: u32) -> Option<&str> {
        self.by_id.get(&buff_id).map(|(_, n)| n.as_str())
    }

    /// Whether buff `buff_id` is the consumable AxiForge calls `label`. The
    /// WvW "Mists-Infused" variant of a food counts as that food.
    pub fn matches(&self, buff_id: u32, label: &str) -> bool {
        let Some(name) = self.name_of(buff_id) else { return false };
        let label = ALIASES.iter().find(|(from, _)| *from == label).map_or(label, |(_, to)| to);
        let (have, want) = (norm(name), norm(label));
        have == want || have == format!("mistsinfused{want}")
    }

    /// Buff ID with exactly this (normalized) name.
    pub fn find(&self, name: &str) -> Option<u32> {
        let want = norm(name);
        self.by_id.iter().find(|(_, (_, n))| norm(n) == want).map(|(id, _)| *id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::axicode::tables::{FOOD, UTILITY};

    #[test]
    fn every_axiforge_label_matches_a_bundled_buff() {
        let c = Consumables::bundled();
        for label in FOOD.iter().chain(UTILITY.iter()).filter(|l| !l.is_empty()) {
            assert!(c.by_id.keys().any(|id| c.matches(*id, label)), "no buff for {label:?}");
        }
    }

    #[test]
    fn kinds_aliases_and_mists_infused() {
        let c = Consumables::bundled();
        let truffle = c.find("Plate of Truffle Steak").unwrap();
        assert_eq!(c.kind_of(truffle), Some(ConsumableKind::Food));
        assert!(c.matches(truffle, "Plate of Truffle Steak Dinner"));
        assert!(!c.matches(truffle, "Plate of Beef Rendang"));

        let mists = c.find("Mists-Infused Peppercorn-Crusted Sous-Vide Steak").unwrap();
        assert!(c.matches(mists, "Peppercorn-Crusted Sous-Vide Steak"));

        let stone = c.find("Superior Sharpening Stone").unwrap();
        assert_eq!(c.kind_of(stone), Some(ConsumableKind::Utility));
        assert_eq!(c.kind_of(1), None);
    }
}
