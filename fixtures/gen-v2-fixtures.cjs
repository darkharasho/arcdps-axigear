// Generates the AxiForge v2 published fixtures with AxiForge's OWN encryptor, so
// the Rust tests prove cross-implementation compatibility. Read-only use of the
// axiforge checkout (it only require()s buildEncryption.js and prints nothing).
//
//   node fixtures/gen-v2-fixtures.cjs [path/to/axiforge]
//
// Inputs : comp-tuesday.published.json (the v1 comp plaintext, with embedded `builds`)
//          and the v1 build-firebrand.enc (decrypted with fixture.key).
// Outputs: build-firebrand.enc.v2          v2 envelope of the v1 build-link plaintext
//          comp-tuesday.v2.enc.v2          v2 comp ("v":2, `members` instead of `builds`)
//          member-<name>.enc.v2 / .enc     member build files (firebrand: v1 envelope,
//                                          berserker + necro: v2), each with its own key
//          comp-tuesday.v2.members.json    { buildId: { fileId, key, owner } } (same as in the comp)
// Member keys are fixed so the output is reviewable; IVs are random, so re-running
// changes the bytes (the tests only depend on the plaintext).
const fs = require("fs");
const path = require("path");
const axiforge = process.argv[2] || path.join(process.env.HOME, "Documents/GitHub/axiforge");
const enc = require(path.join(axiforge, "src/main/buildEncryption"));

const dir = __dirname;
const key = fs.readFileSync(path.join(dir, "fixture.key"), "utf8").trim();
const comp = JSON.parse(fs.readFileSync(path.join(dir, "comp-tuesday.published.json"), "utf8"));
const w = (name, data) => fs.writeFileSync(path.join(dir, name), data);

const link = enc.decryptBuild(fs.readFileSync(path.join(dir, "build-firebrand.enc"), "utf8").trim(), key);
w("build-firebrand.enc.v2", enc.encryptPayload(link, key));

const members = {};
const ids = { firebrand: "aaaa0001", berserker: "bbbb0002", necro: "cccc0003" };
let n = 1;
for (const [buildId, build] of Object.entries(comp.builds)) {
  const mkey = Buffer.alloc(32, n++).toString("base64url");
  members[buildId] = { fileId: ids[buildId], key: mkey, owner: "teammate" };
  if (buildId === "firebrand") w(`member-${buildId}.enc`, enc.encryptBuild(build, mkey));
  else w(`member-${buildId}.enc.v2`, enc.encryptPayload(build, mkey));
}
const { builds, ...rest } = comp;
w("comp-tuesday.v2.enc.v2", enc.encryptPayload({ ...rest, v: 2, members }, key));
w("comp-tuesday.v2.members.json", JSON.stringify(members, null, 2) + "\n");
