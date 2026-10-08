//! Test-only helpers. Grows in later tasks (World builder in Task 14).

use crate::axicode::decode_comp_code;
use crate::model::{Build, Comp};

pub fn fixture(name: &str) -> String {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/");
    std::fs::read_to_string(format!("{path}{name}"))
        .unwrap_or_else(|e| panic!("fixture {name}: {e}"))
        .trim()
        .to_string()
}

/// The Tuesday comp: builds [Firebrand, Berserker, core Necro];
/// party 1 = [Firebrand, Berserker, DPS tag(Berserker, Necro)], party 2 = [Firebrand, Necro].
pub fn comp() -> Comp {
    decode_comp_code(&fixture("comp-tuesday.txt")).unwrap()
}

pub fn firebrand() -> Build {
    comp().builds[0].clone()
}

#[allow(dead_code)]
pub fn berserker() -> Build {
    comp().builds[1].clone()
}

#[allow(dead_code)]
pub fn necro() -> Build {
    comp().builds[2].clone()
}
