// Regenerates fixtures/ from AxiForge's shipping AxiCode encoder/decoder.
// Usage: AXIFORGE=../axiforge node fixtures/gen-fixtures.cjs
"use strict";
const fs = require("fs");
const path = require("path");
const crypto = require("crypto");
const AXIFORGE = path.resolve(process.env.AXIFORGE || path.join(__dirname, "..", "..", "axiforge"));
const code = require(path.join(AXIFORGE, "packages", "axicode", "src", "index.js"));
const OUT = process.env.OUT || __dirname;

// Real trait tiers from the bundled GW2 data, so positions and IDs agree.
const SPECDB = Object.fromEntries(JSON.parse(fs.readFileSync(
  process.env.SPECDB || path.join(__dirname, "..", "crates", "axigear-core", "data", "specializations.json"), "utf8"))
  .map((s) => [s.id, s]));
// positions: 1 = top, 2 = middle, 3 = bottom, per tier.
const spec = (id, positions) => {
  const s = SPECDB[id];
  const pick = (tier) => s.majors[tier][positions[tier] - 1];
  return { id, name: s.name, elite: s.elite, majorChoices: { 1: pick(0), 2: pick(1), 3: pick(2) },
    majorTraitsByTier: { 1: s.majors[0].map((id) => ({ id })), 2: s.majors[1].map((id) => ({ id })), 3: s.majors[2].map((id) => ({ id })) } };
};
const uniformInf = (id, { oh1, set2, oh2 }) => ({
  head: id, shoulders: id, chest: id, hands: id, legs: id, feet: id,
  back: [id, id], ring1: [id, id, id], ring2: [id, id, id], accessory1: id, accessory2: id,
  mainhand1: [id, id], offhand1: oh1 ? [id] : [], mainhand2: set2 ? [id, id] : [], offhand2: oh2 ? [id] : [],
});
const base = { underwaterSkills: { heal: null, utility: [null, null, null], elite: null },
  selectedLegends: ["", ""], selectedUnderwaterLegends: ["", ""],
  selectedPets: { terrestrial1: 0, terrestrial2: 0, aquatic1: 0, aquatic2: 0 },
  activeAttunement: "", activeAttunement2: "", activeKit: 0, activeWeaponSet: 1, allianceTacticsForm: 0,
  antiquaryArtifacts: { f2: 0, f3: 0, f4: 0 } };

const BUILDS = {
  berserker: { ...base, id: "berserker", title: "Berserker DPS", profession: "Warrior", gameMode: "pve",
    specializations: [
      spec(4, [3, 3, 1]), spec(36, [1, 2, 3]), spec(18, [2, 1, 1]),
    ],
    skills: { heal: { id: 14402 }, utility: [{ id: 14404 }, { id: 14410 }, { id: 14405 }], elite: { id: 14355 } },
    equipment: { statPackage: "Berserker's", relic: "Relic of the Thief",
      food: "Bowl of Sweet and Spicy Butternut Squash Soup", utility: "Superior Sharpening Stone", enrichment: "",
      weapons: { mainhand1: "greatsword", offhand1: "", mainhand2: "axe", offhand2: "", aquatic1: "", aquatic2: "" },
      runes: { head: "24836", shoulders: "24836", chest: "24836", hands: "24836", legs: "24836", feet: "24836" },
      sigils: { mainhand1: ["24615", "24868"], offhand1: [], mainhand2: ["24615"], offhand2: [], aquatic1: [], aquatic2: [] },
      infusions: uniformInf("49432", { oh1: false, set2: true, oh2: false }) } },
  firebrand: { ...base, id: "firebrand", title: "Quickbrand", profession: "Guardian", gameMode: "wvw",
    specializations: [spec(42, [2, 2, 1]), spec(46, [1, 3, 2]), spec(62, [3, 1, 1])],
    skills: { heal: { id: 41714 }, utility: [{ id: 40915 }, { id: 9153 }, { id: 9246 }], elite: { id: 43357 } },
    equipment: { statPackage: "", relic: "Relic of the Flock", food: "Plate of Truffle Steak Dinner",
      utility: "Bountiful Maintenance Oil", enrichment: "",
      slots: { head: "Minstrel's", shoulders: "Minstrel's", chest: "Minstrel's", hands: "Minstrel's", legs: "Minstrel's", feet: "Minstrel's",
        back: "Minstrel's", amulet: "Minstrel's", ring1: "Minstrel's", ring2: "Minstrel's", accessory1: "Minstrel's", accessory2: "Minstrel's",
        mainhand1: "Minstrel's", offhand1: "Minstrel's", mainhand2: "Harrier's" },
      weapons: { mainhand1: "mace", offhand1: "shield", mainhand2: "staff", offhand2: "", aquatic1: "", aquatic2: "" },
      runes: { head: "24842", shoulders: "24842", chest: "24842", hands: "24842", legs: "24691", feet: "24691" },
      sigils: { mainhand1: ["24865"], offhand1: ["24612"], mainhand2: ["24865", "24612"], offhand2: [], aquatic1: [], aquatic2: [] },
      infusions: { ...uniformInf("37133", { oh1: true, set2: true, oh2: false }), back: ["37133", "86180"] } } },
  necro: { ...base, id: "necro", title: "Core Necro", profession: "Necromancer", gameMode: "wvw",
    specializations: [spec(19, [1, 1, 1]), spec(50, [1, 1, 1]), spec(53, [1, 1, 1])],
    skills: { heal: { id: 10527 }, utility: [{ id: 10545 }, { id: 0 }, { id: 10685 }], elite: { id: 10646 } },
    equipment: { statPackage: "Viper's", relic: "", food: "", utility: "", enrichment: "",
      weapons: { mainhand1: "staff", offhand1: "", mainhand2: "", offhand2: "", aquatic1: "", aquatic2: "" },
      runes: { head: "0", shoulders: "0", chest: "0", hands: "0", legs: "0", feet: "0" },
      sigils: { mainhand1: ["0", "0"], offhand1: [], mainhand2: [], offhand2: [], aquatic1: [], aquatic2: [] },
      infusions: uniformInf("0", { oh1: false, set2: false, oh2: false }) } },
};

const COMP = { id: "comp1", name: "Tuesday Zerg", gameMode: "wvw",
  partyLines: [
    { id: "l1", capacity: 5, slots: ["firebrand", "berserker", "tag:dps"] },
    { id: "l2", capacity: 5, slots: ["firebrand", "necro"] },
  ],
  categories: [{ id: "dps", name: "DPS", icon: "", buildIds: ["berserker", "necro"] }],
};

const write = (name, data) => fs.writeFileSync(path.join(OUT, name), typeof data === "string" ? data + "\n" : JSON.stringify(data, null, 2) + "\n");

for (const [key, build] of Object.entries(BUILDS)) {
  const share = code.encodeShareCode(build);
  write(`build-${key}.txt`, share);
  write(`build-${key}.decoded.json`, code.decodeShareCode(share));
}
const compCode = code.encodeCompCode(COMP, BUILDS);
write("comp-tuesday.txt", compCode);
write("comp-tuesday.decoded.json", code.decodeCompCode(compCode));

// Published payload: same shape serializeCompForPublish emits (schemaVersion added by the axiforge PR).
const published = { schemaVersion: 1, id: COMP.id, name: COMP.name, notes: "", tags: [], gameMode: COMP.gameMode,
  partyLines: COMP.partyLines, buildColors: {}, images: {}, notesClassIcons: {}, categories: COMP.categories,
  builds: Object.fromEntries(Object.entries(BUILDS).map(([k, b]) => [k, {
    ...b,
    specializations: b.specializations.map(({ majorTraitsByTier, ...s }) => s),
    skills: { heal: { ...b.skills.heal, name: `skill-${b.skills.heal.id}` },
      utility: b.skills.utility.map((u) => (u.id ? { ...u, name: `skill-${u.id}` } : null)),
      elite: { ...b.skills.elite, name: `skill-${b.skills.elite.id}` } },
  }])) };
// Same layout as axiforge buildEncryption.js: base64(iv(12) | ciphertext | tag(16)).
// Fixed key so the Rust tests can decrypt; the iv is random per run.
const KEY = "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8";
const encrypt = (obj) => {
  const iv = crypto.randomBytes(12);
  const cipher = crypto.createCipheriv("aes-256-gcm", Buffer.from(KEY, "base64url"), iv);
  const enc = Buffer.concat([cipher.update(JSON.stringify(obj), "utf8"), cipher.final()]);
  return Buffer.concat([iv, enc, cipher.getAuthTag()]).toString("base64");
};
write("comp-tuesday.enc", encrypt(published));
write("build-firebrand.enc", encrypt(published.builds.firebrand));
write("fixture.key", KEY);
write("comp-tuesday.published.json", published);
console.log("fixtures written to", OUT);
