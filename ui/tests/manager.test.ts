import { test } from "node:test";
import assert from "node:assert/strict";
import {
  ManagerController,
  buildsOf,
  canRollBack,
  describeCheck,
  describeLocation,
  formatBytes,
  gamesWithBuilds,
  rootProblem,
  slugId,
  variantLabel,
  type BuildRequest,
  type ManagerView,
} from "../src/lib/manager";
import { gogGame, steamGame } from "./fixtures";
import { EMPTY_VIEW, build, fakeManager, root } from "./manager-fake";

const REQ: Omit<BuildRequest, "gameId"> = { variant: "original", baseVersion: "1.0.0", sourceDescription: "my own install" };

test("without a backend the controller loads nothing and says so", async () => {
  const c = new ManagerController(null);
  assert.equal(c.hasBackend, false);
  await c.load();
  assert.equal(c.getSnapshot().loaded, true);
  assert.equal(c.getSnapshot().view, null);
  assert.equal(await c.addRoot("Games", "D:\\x", "builds"), false);
});

test("load reads the saved state; a failure is shown and the page still loads", async () => {
  const fake = fakeManager({ roots: [root("main", "Main")] });
  const c = new ManagerController(fake.api);
  await c.load();
  assert.equal(c.getSnapshot().view?.roots.length, 1);
  assert.equal(c.getSnapshot().error, null);

  const broken = fakeManager();
  broken.failNext("view", "the saved state is damaged");
  const d = new ManagerController(broken.api);
  await d.load();
  assert.equal(d.getSnapshot().loaded, true);
  assert.equal(d.getSnapshot().view, null);
  assert.equal(d.getSnapshot().error, "the saved state is damaged");
});

test("adding a folder checks the name and the path before it asks the backend, and sends a tidy id", async () => {
  const fake = fakeManager();
  const c = new ManagerController(fake.api);
  assert.equal(await c.addRoot("   ", "D:\\x", "builds"), false);
  assert.match(c.getSnapshot().error ?? "", /name/);
  assert.equal(await c.addRoot("Big Disk", "  ", "builds"), false);
  assert.match(c.getSnapshot().error ?? "", /path/);
  assert.equal(fake.calls.length, 0);

  assert.equal(await c.addRoot("  Big Disk! #2 ", " D:\\SUL\\Big ", "mods"), true);
  assert.deepEqual(fake.calls, [{ op: "addRoot", args: ["big-disk-2", "Big Disk! #2", "D:\\SUL\\Big", "mods"] }]);
  assert.equal(c.getSnapshot().error, null);
  assert.equal(c.getSnapshot().view?.roots[0]?.id, "big-disk-2");
});

test("pointing a folder at a new place needs a path, and sends it trimmed", async () => {
  const fake = fakeManager();
  const c = new ManagerController(fake.api);
  assert.equal(await c.relinkRoot("main", "   "), false);
  assert.match(c.getSnapshot().error ?? "", /path/);
  assert.equal(fake.calls.length, 0);
  assert.equal(await c.relinkRoot("main", "  E:\\SUL\\Main "), true);
  assert.deepEqual(fake.calls, [{ op: "relinkRoot", args: ["main", "E:\\SUL\\Main"] }]);
});

test("a refusal from the core is shown as it came and leaves the list as it was", async () => {
  const fake = fakeManager({ roots: [root("main", "Main")] });
  const c = new ManagerController(fake.api);
  await c.load();
  assert.equal(await c.addRoot("Main", "D:\\other", "builds"), false);
  assert.match(c.getSnapshot().error ?? "", /already exists/);
  assert.equal(c.getSnapshot().busy, null);
  assert.equal(c.getSnapshot().view?.roots.length, 1);
  // The next action clears the old error.
  assert.equal(await c.addRoot("Second", "D:\\second", "builds"), true);
  assert.equal(c.getSnapshot().error, null);
});

test("only one operation runs at a time; a second request while one runs is ignored, not queued", async () => {
  const fake = fakeManager({ roots: [root("main", "Main")] });
  const c = new ManagerController(fake.api);
  await c.load();
  const release = fake.hold();
  const first = c.importBuild(steamGame, REQ, root("main", "Main"));
  assert.match(c.getSnapshot().busy ?? "", /import/);
  assert.equal(await c.importBuild(gogGame, REQ, root("main", "Main")), false);
  assert.equal(await c.verify("x"), false);
  release();
  assert.equal(await first, true);
  assert.equal(c.getSnapshot().busy, null);
  assert.equal(fake.changes().length, 1);
  assert.equal(fake.calls.filter((x) => x.op === "importBuild").length, 1);
});

test("an import sends the library game's id and the chosen folder, and the notice says the original was only read", async () => {
  const fake = fakeManager({ roots: [root("main", "Main")] });
  const c = new ManagerController(fake.api);
  await c.load();
  assert.equal(await c.importBuild(steamGame, { variant: { custom: "tuned" }, baseVersion: "7.1.1", sourceDescription: "Steam" }, root("main", "Main")), true);
  assert.deepEqual(fake.changes(), [
    { op: "importBuild", args: [{ variant: { custom: "tuned" }, baseVersion: "7.1.1", sourceDescription: "Steam", gameId: steamGame.id }, "main"] },
  ]);
  assert.match(c.getSnapshot().notice ?? "", /Helldivers 2.*only read/);
  assert.equal(c.getSnapshot().view?.builds.length, 1);
});

test("recording in place copies nothing and says so", async () => {
  const fake = fakeManager();
  const c = new ManagerController(fake.api);
  assert.equal(await c.registerInPlace(steamGame, REQ), true);
  assert.equal(fake.changes()[0]?.op, "registerInPlace");
  assert.match(c.getSnapshot().notice ?? "", /Nothing was copied/);
  assert.equal(c.getSnapshot().view?.builds[0]?.location.kind, "external_install");
});

test("a check is remembered per build and reported in plain words, including a mismatch", async () => {
  const fake = fakeManager({ builds: [build("a", "g", "original"), build("b", "g", "optimized")] });
  fake.drift("b");
  const c = new ManagerController(fake.api);
  await c.verify("a");
  assert.match(c.getSnapshot().notice ?? "", /^Matches its manifest/);
  await c.verify("b");
  assert.match(c.getSnapshot().notice ?? "", /Does not match its manifest: 0 missing, 1 changed/);
  assert.equal(c.getSnapshot().checks["a"]?.state, "ready");
  assert.equal(c.getSnapshot().checks["b"]?.state, "failed");
});

test("make active and roll back go through the backend; a rollback with nothing earlier is refused by it", async () => {
  const fake = fakeManager({ builds: [build("a", "g", "original"), build("b", "g", "original")] });
  const c = new ManagerController(fake.api);
  await c.activate("a");
  await c.activate("b");
  assert.equal(canRollBack(c.getSnapshot().view!, "g", "original"), true);
  assert.equal(await c.rollback("g", "original"), true);
  assert.equal(c.getSnapshot().view?.builds.find((b) => b.id === "a")?.active, true);
  assert.equal(await c.rollback("g", "original"), false);
  assert.match(c.getSnapshot().error ?? "", /no earlier build/);
});

test("listeners hear every change and can leave", async () => {
  const fake = fakeManager();
  const c = new ManagerController(fake.api);
  let heard = 0;
  const off = c.subscribe(() => (heard += 1));
  await c.load();
  assert.ok(heard >= 1);
  const before = heard;
  off();
  await c.addRoot("Games", "D:\\g", "builds");
  assert.equal(heard, before);
});

test("slugId keeps letters and digits, joins the rest with single dashes, and never ends or starts with one", () => {
  assert.equal(slugId("Big Disk"), "big-disk");
  assert.equal(slugId("  --D:\\SUL (2)--  "), "d-sul-2");
  assert.equal(slugId("!!!"), "");
  assert.equal(slugId("x".repeat(40)).length, 32);
  assert.ok(!slugId("a".repeat(31) + " b").endsWith("-"));
});

test("formatBytes uses decimal units and says unknown for an unknown size", () => {
  assert.equal(formatBytes(null), "unknown");
  assert.equal(formatBytes(0), "0 B");
  assert.equal(formatBytes(999), "999 B");
  assert.equal(formatBytes(1_500), "1.5 KB");
  assert.equal(formatBytes(500_000_000_000), "500 GB");
  assert.equal(formatBytes(12_300_000_000), "12 GB");
  // The edges of a unit: 1000 is the first KB, and a size that would print as "1000 KB" is shown as 1.0 MB.
  assert.equal(formatBytes(1_000), "1.0 KB");
  assert.equal(formatBytes(999_499), "999 KB");
  assert.equal(formatBytes(999_999), "1.0 MB");
});

test("a folder that is gone, or has a different folder in its place, is described as a problem; a good one is not", () => {
  assert.equal(rootProblem(root("a", "A")), null);
  assert.match(rootProblem(root("a", "A", "builds", { present: false, marker_ok: false })) ?? "", /not there/);
  assert.match(rootProblem(root("a", "A", "builds", { marker_ok: false })) ?? "", /different folder or drive/);
});

test("labels: variants, locations and checks", () => {
  assert.equal(variantLabel("original"), "Original");
  assert.equal(variantLabel({ custom: "tuned" }), "Custom: tuned");
  const roots = [root("main", "Main")];
  assert.equal(describeLocation({ kind: "managed", root_id: "main", dir: "x" }, roots), "A copy in Main");
  assert.equal(describeLocation({ kind: "managed", root_id: "gone", dir: "x" }, roots), "A copy in gone");
  assert.match(describeLocation({ kind: "external_install", path: "p" }, roots), /read only/);
  assert.equal(describeCheck({ state: "ready", report: { missing: [], modified: [], extra: [] } }), "Matches its manifest.");
  assert.equal(describeCheck({ state: "ready", report: { missing: [], modified: [], extra: ["a"] } }), "Matches its manifest (1 extra file not listed).");
  assert.equal(describeCheck({ state: "ready", report: { missing: [], modified: [], extra: ["a", "b"] } }), "Matches its manifest (2 extra files not listed).");
  assert.equal(describeCheck({ state: "failed", report: { missing: ["a", "b"], modified: ["c"], extra: [] } }), "Does not match its manifest: 2 missing, 1 changed.");
});

test("builds are listed per game with the original slot first; games without a name show their id", () => {
  const view: ManagerView = {
    ...EMPTY_VIEW,
    builds: [build("z", "g1", "legacy"), build("y", "g1", "tuned"), build("x", "g1", "original"), build("w", "g2", "original")],
    slots: [{ game_id: "g1", slot: "original", build_id: "x", history: ["q"] }],
  };
  assert.deepEqual(buildsOf(view, "g1").map((b) => b.id), ["x", "z", "y"]);
  assert.equal(canRollBack(view, "g1", "original"), true);
  assert.equal(canRollBack(view, "g1", "legacy"), false);
  assert.equal(canRollBack(view, "g2", "original"), false);
  assert.deepEqual(gamesWithBuilds(view, [{ ...steamGame, id: "g1", name: "Zed" }]), [
    { gameId: "g2", name: "g2" },
    { gameId: "g1", name: "Zed" },
  ]);
});
