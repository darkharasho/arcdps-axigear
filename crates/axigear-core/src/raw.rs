//! Equipment in AxiForge's JavaScript shape. Both the AxiCode decoder and
//! published records produce this; `Equipment::from_raw` types it.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OneOrMany {
    One(String),
    Many(Vec<String>),
}

impl OneOrMany {
    pub fn as_slice(&self) -> &[String] {
        match self {
            OneOrMany::One(s) => std::slice::from_ref(s),
            OneOrMany::Many(v) => v,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RawEquipment {
    pub stat_package: String,
    pub slots: BTreeMap<String, String>,
    pub relic: String,
    pub food: String,
    pub utility: String,
    pub weapons: BTreeMap<String, String>,
    pub runes: BTreeMap<String, String>,
    pub sigils: BTreeMap<String, Vec<String>>,
    pub infusions: BTreeMap<String, OneOrMany>,
}
