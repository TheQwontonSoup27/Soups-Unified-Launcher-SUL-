import { test } from "node:test";
import assert from "node:assert/strict";
import {
  ModsController,
  canDeployGame,
  describeDeploy,
  describePurge,
  describeVerify,
  errorsOf,
  formatSize,
  latestOfEachMod,
  leavesFile,
  modKey,
  modsOfGame,
  parseRequires,
  parseVersions,
  versionsOf,
  warningsOf,
  type DeployPlan,
  type ModSource,
  type PurgeItem,
} from "../src/lib/mods";
import { buildMods, fakeMods, mod, modRoot, problem, profile } from "./mods-fake";

const GAME = "gog-1";
const SRC: ModSource = {
  game_id: GAME,
  mod_id: "ui",
  name: "UI mod",
  version: "1.0",
  requires: [],
  compatible_versions: [],
  source: "mine",
  license_note: "mine",
  path: "D:\\mods\\ui",
};

/** A view with one deployable game, one build, one mod and a profile that uses it. */
function world(extra: Parameters<typeof fakeMods>[0] = {}) {
  return fakeMods({
    mods: [mod(GAME, "ui", "1.0")],
    profiles: [profile("main", GAME, "b1", [{ mod_id: "ui", version: "1.0", enabled: true }])],
    builds: [buildMods("b1", GAME, "experimental")],
    mod_roots: [modRoot()],
    deployable_games: [GAME],
    ...extra,
  });
}

const item = (path: string, action: PurgeItem["action"], detail = ""): PurgeItem => ({ path, mod_id: "ui", action, detail });

test("without a backend the controller loads nothing and every action says no", async () => {
  const c = new ModsController(null);
  assert.equal(c.hasBackend, false);
  await c.load();
  assert.equal(c.getSnapshot().loaded, true);
  assert.equal(c.getSnapshot().view, null);
  assert.equal(await c.importMod(SRC, "modlib"), false);
  assert.equal(await c.previewDeploy("main"), false);
  assert.equal(c.getSnapshot().preview, null);
});

test("load reads the saved state; a failure is shown and the page still loads", async () => {
  const c = new ModsController(world().api);
  await c.load();
  assert.equal(c.getSnapshot().view?.mods.length, 1);
  const broken = world();
  broken.failNext("view", "the mod index is damaged");
  const d = new ModsController(broken.api);
  await d.load();
  assert.equal(d.getSnapshot().loaded, true);
  assert.equal(d.getSnapshot().view, null);
  assert.equal(d.getSnapshot().error, "the mod index is damaged");
});

test("an import checks the form before it asks the backend and trims what it sends", async () => {
  const fake = world({ mods: [] });
  const c = new ModsController(fake.api);
  await c.load();
  assert.equal(await c.importMod({ ...SRC, mod_id: "", name: "  " }, "modlib"), false);
  assert.match(c.getSnapshot().error ?? "", /name/);
  assert.equal(await c.importMod({ ...SRC, version: " " }, "modlib"), false);
  assert.match(c.getSnapshot().error ?? "", /version/);
  assert.equal(await c.importMod({ ...SRC, path: "  " }, "modlib"), false);
  assert.match(c.getSnapshot().error ?? "", /path/);
  assert.equal(await c.importMod(SRC, ""), false);
  assert.match(c.getSnapshot().error ?? "", /library folder for mods/);
  assert.deepEqual(fake.changes(), [], "nothing reached the backend");

  assert.equal(await c.importMod({ ...SRC, path: "  D:\\mods\\ui  ", version: " 1.0 " }, "modlib"), true);
  assert.deepEqual(fake.changes().map((x) => x.op), ["importMod"]);
  assert.equal((fake.changes()[0]!.args[0] as ModSource).path, "D:\\mods\\ui");
  assert.equal((fake.changes()[0]!.args[0] as ModSource).version, "1.0");
  assert.equal(c.getSnapshot().view?.mods.length, 1);
  assert.match(c.getSnapshot().notice ?? "", /only read/);
});

test("a refused import shows the core's words and leaves the list as it was", async () => {
  const fake = world();
  const c = new ModsController(fake.api);
  await c.load();
  assert.equal(await c.importMod(SRC, "modlib"), false);
  assert.match(c.getSnapshot().error ?? "", /already in the store/);
  assert.equal(c.getSnapshot().view?.mods.length, 1);
  assert.equal(c.getSnapshot().busy, null);
});

test("a check records its result per mod and says what it found", async () => {
  const fake = world();
  const c = new ModsController(fake.api);
  await c.load();
  const m = c.getSnapshot().view!.mods[0]!;
  assert.equal(await c.verifyMod(m), true);
  assert.deepEqual(c.getSnapshot().checks[modKey(m)], { missing: [], modified: [], extra: [] });
  assert.equal(c.getSnapshot().notice, "Matches what was imported.");
});

test("a profile needs a name, a game and a build, and gets a tidy id", async () => {
  const fake = world({ profiles: [] });
  const c = new ModsController(fake.api);
  await c.load();
  assert.equal(await c.createProfile("  ", GAME, "b1"), false);
  assert.equal(await c.createProfile("Main set", "", "b1"), false);
  assert.equal(await c.createProfile("Main set", GAME, ""), false);
  assert.deepEqual(fake.changes(), []);
  assert.equal(await c.createProfile("  Main set ", GAME, "b1"), true);
  assert.deepEqual(fake.changes()[0]!.args, ["main-set", "Main set", GAME, "b1"]);
  assert.match(c.getSnapshot().notice ?? "", /Nothing was deployed/);
});

test("a profile edit says when the build keeps what was deployed", async () => {
  const fake = world({ builds: [buildMods("b1", GAME, "experimental", { deployment: { id: "d", profile_id: "main", profile_name: "MAIN", status: "deployed", started_unix: 1, finished_unix: 2, files: 2 } })] });
  const c = new ModsController(fake.api);
  await c.load();
  assert.equal(await c.editProfile("main", { action: "set_enabled", mod_id: "ui", enabled: false }), true);
  assert.match(c.getSnapshot().notice ?? "", /applies at the next deploy/);
  assert.equal(c.getSnapshot().view?.profiles[0]?.mods[0]?.enabled, false);
  assert.equal(await c.editProfile("main", { action: "remove_mod", mod_id: "ghost" }), false);
  assert.match(c.getSnapshot().error ?? "", /not in this profile/);
});

test("a deploy preview changes nothing, and only a confirmation runs the deploy", async () => {
  const fake = world();
  const c = new ModsController(fake.api);
  await c.load();
  assert.equal(await c.previewDeploy("main"), true);
  const p = c.getSnapshot().preview;
  assert.equal(p?.kind, "deploy");
  assert.deepEqual(fake.changes(), [], "a preview is read only");
  assert.equal(c.getSnapshot().view?.builds[0]?.deployment, null);

  c.closePreview();
  assert.equal(c.getSnapshot().preview, null);
  assert.deepEqual(fake.changes(), [], "cancelling does nothing");
  assert.equal(await c.confirm(), false, "there is nothing to confirm");

  await c.previewDeploy("main");
  assert.equal(await c.confirm(), true);
  assert.deepEqual(fake.changes().map((x) => x.op), ["deploy"]);
  assert.equal(c.getSnapshot().preview, null);
  assert.equal(c.getSnapshot().view?.builds[0]?.deployment?.profile_id, "main");
  assert.match(c.getSnapshot().notice ?? "", /Deployed 1 file: 1 added, 0 replaced/);
});

test("a game with no adapter gets a refusal from the preview and no deploy is ever sent", async () => {
  const fake = world({ deployable_games: [] });
  const c = new ModsController(fake.api);
  await c.load();
  assert.equal(await c.previewDeploy("main"), false);
  assert.match(c.getSnapshot().error ?? "", /no adapter/);
  assert.equal(c.getSnapshot().preview, null);
  assert.equal(await c.confirm(), false);
  assert.deepEqual(fake.changes(), []);
});

test("a blocked deploy plan is never run, even if it is confirmed", async () => {
  const fake = world();
  const blocked: DeployPlan = { profile_id: "main", build_id: "b1", files: [], problems: [problem("data/anticheat/x lands in a folder mods may never change")], total_bytes: 0 };
  fake.setPlan(blocked);
  const c = new ModsController(fake.api);
  await c.load();
  await c.previewDeploy("main");
  assert.equal(await c.confirm(), false);
  assert.deepEqual(fake.changes(), [], "no deploy was sent");
  assert.match(c.getSnapshot().error ?? "", /blocked/);
});

test("a purge preview lists what would happen, and the purge runs only after the confirmation", async () => {
  const dep = { id: "d", profile_id: "main", profile_name: "MAIN", status: "deployed" as const, started_unix: 1, finished_unix: 2, files: 2 };
  const fake = world({ builds: [buildMods("b1", GAME, "experimental", { deployment: dep })] });
  fake.setPurgeItems([item("data/a.bin", "remove_mod_file_and_restore_base"), item("mods/ui.txt", "remove_mod_file")]);
  const c = new ModsController(fake.api);
  await c.load();
  assert.equal(await c.previewPurge("b1"), true);
  const p = c.getSnapshot().preview;
  assert.equal(p?.kind === "purge" && p.plan.items.length, 2);
  assert.deepEqual(fake.changes(), []);
  assert.equal(await c.confirm(), true);
  assert.deepEqual(fake.changes().map((x) => x.op), ["purge"]);
  const after = c.getSnapshot();
  assert.equal(after.view?.builds[0]?.deployment, null);
  assert.equal(after.view?.builds[0]?.undoable_purge?.files, 2);
  assert.match(after.notice ?? "", /Purged: 2 files moved to the recovery folder/);
});

test("a purge that leaves a file says the build is still partly modded", async () => {
  const dep = { id: "d", profile_id: "main", profile_name: "MAIN", status: "deployed" as const, started_unix: 1, finished_unix: 2, files: 2 };
  const fake = world({ builds: [buildMods("b1", GAME, "experimental", { deployment: dep })] });
  fake.setPurgeItems([item("data/a.bin", "remove_mod_file"), item("mods/ui.txt", "leave_changed_since_deploy", "the file changed after it was deployed")]);
  const c = new ModsController(fake.api);
  await c.load();
  await c.previewPurge("b1");
  await c.confirm();
  assert.match(c.getSnapshot().notice ?? "", /1 file left in place, so the build is still partly modded: mods\/ui\.txt \(the file changed after it was deployed\)/);
  assert.notEqual(c.getSnapshot().view?.builds[0]?.deployment, null, "the build still shows what is deployed");
});

test("undo asks first, refuses when nothing can be undone, and sends the purge's own id", async () => {
  const fake = world({ builds: [buildMods("b1", GAME, "experimental", { undoable_purge: { id: "purge-7", profile_name: "MAIN", unix: 3, files: 2 } })] });
  const c = new ModsController(fake.api);
  await c.load();
  assert.equal(c.previewUndo("nope"), false);
  assert.match(c.getSnapshot().error ?? "", /no purge/);
  assert.equal(c.previewUndo("b1"), true);
  assert.equal(c.getSnapshot().preview?.kind, "undo");
  assert.deepEqual(fake.changes(), [], "asking changes nothing");
  assert.equal(await c.confirm(), true);
  assert.deepEqual(fake.changes()[0], { op: "undoPurge", args: ["b1", "purge-7"] });
  assert.match(c.getSnapshot().notice ?? "", /Undid the purge: 1 file put back/);
});

test("purge and deploy previews the purge of the profile's own build and runs one operation", async () => {
  const dep = { id: "d", profile_id: "main", profile_name: "MAIN", status: "deployed" as const, started_unix: 1, finished_unix: 2, files: 1 };
  const fake = world({ builds: [buildMods("b1", GAME, "experimental", { deployment: dep })] });
  fake.setPurgeItems([item("data/ui.bin", "remove_mod_file")]);
  const c = new ModsController(fake.api);
  await c.load();
  assert.equal(await c.previewPurgeAndDeploy("ghost"), false);
  assert.equal(await c.previewPurgeAndDeploy("main"), true);
  const p = c.getSnapshot().preview;
  assert.equal(p?.kind === "purge_and_deploy" && p.buildId, "b1");
  assert.deepEqual(fake.changes(), []);
  assert.equal(await c.confirm(), true);
  assert.deepEqual(fake.changes().map((x) => x.op), ["purgeAndDeploy"]);
  assert.match(c.getSnapshot().notice ?? "", /Purged: 1 file moved.*Deployed 1 file/);
});

test("purge and deploy that purges nothing new says the deploy was not attempted", async () => {
  const dep = { id: "d", profile_id: "main", profile_name: "MAIN", status: "deployed" as const, started_unix: 1, finished_unix: 2, files: 1 };
  const fake = world({ builds: [buildMods("b1", GAME, "experimental", { deployment: dep })] });
  fake.setPurgeItems([item("data/ui.bin", "leave_changed_since_deploy", "changed")]);
  const c = new ModsController(fake.api);
  await c.load();
  await c.previewPurgeAndDeploy("main");
  await c.confirm();
  assert.match(c.getSnapshot().notice ?? "", /The deploy was not attempted/);
});

test("only one operation runs at a time", async () => {
  const fake = world();
  const c = new ModsController(fake.api);
  await c.load();
  const release = fake.hold();
  const first = c.editProfile("main", { action: "notes", notes: "a" });
  assert.equal(c.getSnapshot().busy, "edit:main");
  assert.equal(await c.editProfile("main", { action: "notes", notes: "b" }), false, "a second request while one runs is ignored");
  release();
  assert.equal(await first, true);
  assert.equal(fake.changes().length, 1);
  assert.equal(c.getSnapshot().busy, null);
});

test("a failed operation clears the busy mark and keeps the earlier data", async () => {
  const fake = world();
  const c = new ModsController(fake.api);
  await c.load();
  fake.failNext("editProfile", "disk full");
  assert.equal(await c.editProfile("main", { action: "notes", notes: "x" }), false);
  assert.equal(c.getSnapshot().error, "disk full");
  assert.equal(c.getSnapshot().busy, null);
  assert.equal(c.getSnapshot().view?.profiles[0]?.notes, "");
});

test("subscribers hear every change", async () => {
  const c = new ModsController(world().api);
  let heard = 0;
  const off = c.subscribe(() => (heard += 1));
  await c.load();
  assert.ok(heard >= 1);
  const before = heard;
  off();
  await c.load();
  assert.equal(heard, before);
});

test("requirement text is parsed into ids with an optional oldest version, and bad pieces are named", () => {
  assert.deepEqual(parseRequires(""), { requires: [] });
  assert.deepEqual(parseRequires(" base , addon>=1.2 ,"), { requires: [{ mod_id: "base", min_version: null }, { mod_id: "addon", min_version: "1.2" }] });
  for (const bad of ["a b", ">=1", "a>=", "a>=1>=2", "../x", "a,,$"]) {
    assert.ok("bad" in parseRequires(bad), bad);
  }
  assert.deepEqual(parseRequires("ok, ../x"), { bad: "../x" });
  assert.deepEqual(parseVersions(" 1.0.0 ,2.0, "), ["1.0.0", "2.0"]);
  assert.deepEqual(parseVersions(""), []);
});

test("small helpers pick the right mods, versions and games", () => {
  const view = {
    mods: [mod(GAME, "b", "1.0", { imported_unix: 5 }), mod(GAME, "a", "1.0", { imported_unix: 1 }), mod(GAME, "a", "2.0", { imported_unix: 9 }), mod("other", "a", "3.0")],
    profiles: [],
    builds: [],
    mod_roots: [],
    deployable_games: [GAME],
  };
  assert.deepEqual(versionsOf(view, GAME, "a"), ["2.0", "1.0"], "newest import first");
  assert.deepEqual(latestOfEachMod(view, GAME).map((m) => `${m.mod_id}@${m.version}`), ["a@2.0", "b@1.0"].sort((x, y) => `Mod ${x[0]}`.localeCompare(`Mod ${y[0]}`)));
  assert.deepEqual(modsOfGame(view, GAME).map(modKey), [`${GAME}/a@1.0`, `${GAME}/a@2.0`, `${GAME}/b@1.0`]);
  assert.equal(canDeployGame(view, GAME), true);
  assert.equal(canDeployGame(view, "other"), false);
});

test("problems split by how serious they are, and purge actions say whether a file stays", () => {
  const list = [problem("a"), problem("b", "warning"), problem("c")];
  assert.deepEqual(errorsOf(list).map((p) => p.message), ["a", "c"]);
  assert.deepEqual(warningsOf(list).map((p) => p.message), ["b"]);
  assert.equal(leavesFile("leave_changed_since_deploy"), true);
  assert.equal(leavesFile("leave_base_copy_problem"), true);
  for (const a of ["remove_mod_file", "remove_mod_file_and_restore_base", "restore_base_only", "forget"] as const) assert.equal(leavesFile(a), false, a);
});

test("sizes and report sentences read plainly", () => {
  assert.equal(formatSize(0), "0 B");
  assert.equal(formatSize(999), "999 B");
  assert.equal(formatSize(999_999), "1.0 MB", "999,999 bytes must not print as 1000 KB");
  assert.equal(formatSize(1_500_000_000), "1.5 GB");
  assert.equal(describeVerify({ missing: [], modified: [], extra: [] }), "Matches what was imported.");
  assert.equal(describeVerify({ missing: [], modified: [], extra: ["x"] }), "Matches what was imported (1 extra file not listed).");
  assert.equal(describeVerify({ missing: ["a"], modified: ["b", "c"], extra: [] }), "Does not match what was imported: 1 missing, 2 changed.");
  assert.equal(describeDeploy({ deployment_id: "d", added: ["a"], replaced: ["b", "c"], kept: [], warnings: [] }), "Deployed 3 files: 1 added, 2 replaced (the replaced originals are kept).");
  assert.equal(describePurge({ purge_id: "p", removed: ["a"], restored: ["a"], forgotten: [], left: [], recovery_dir: null }), "Purged: 1 file moved to the recovery folder, 1 original file put back.");
});
