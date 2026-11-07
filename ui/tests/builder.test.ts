import { test } from "node:test";
import assert from "node:assert/strict";
import {
  BuilderController,
  EMPTY_DRAFT,
  canPlanGame,
  describePlan,
  describeStep,
  describeVerifyRun,
  draftOf,
  hunksToText,
  isBuilderView,
  parseHunks,
  recipeOf,
  runKey,
  shortHash,
  type Draft,
  type DraftStep,
  type Step,
} from "../src/lib/builder";
import { HEX64, aPlan, aRecipe, fakeBuilder, mod, modRoot, recipeView } from "./builder-fake";

const GAME = "gog-1";

/** A view with one deployable game, one mod and a library folder. */
function world(extra: Parameters<typeof fakeBuilder>[0] = {}) {
  return fakeBuilder({ mods: [mod(GAME, "louder", "1.0")], mod_roots: [modRoot()], deployable_games: [GAME], ...extra });
}

async function loaded(fake: ReturnType<typeof fakeBuilder>) {
  const c = new BuilderController(fake.api);
  await c.load();
  return c;
}

const layer: DraftStep = { op: "layer_mod", mod_id: "louder", version: "1.0", into: "" };
const patchStep = (extra: Partial<Extract<DraftStep, { op: "patch_file" }>> = {}): DraftStep => ({
  op: "patch_file",
  path: "data/a.bin",
  patch_name: "Louder UI",
  description: "makes it louder",
  expect_sha256: HEX64,
  hunks: "0x10 00ff 01fe",
  ...extra,
});
const draft = (steps: DraftStep[], extra: Partial<Draft> = {}): Draft => ({ ...EMPTY_DRAFT, name: "Loud", game_id: GAME, steps, ...extra });

// ---------------------------------------------------------------------------
// Small rules
// ---------------------------------------------------------------------------

test("hunks are typed one per line, the offset in decimal or hexadecimal, and the bytes come back lower case", () => {
  assert.deepEqual(parseHunks("16 00FF 01fe\n\n  0x20   aa   bb  "), {
    hunks: [
      { offset: 16, old_hex: "00ff", new_hex: "01fe" },
      { offset: 32, old_hex: "aa", new_hex: "bb" },
    ],
  });
});

test("a hunk that cannot be understood says which line and why, and an empty list is not accepted", () => {
  const bad = (text: string) => {
    const r = parseHunks(text);
    assert.ok("bad" in r, `expected ${JSON.stringify(text)} to be refused`);
    return r.bad;
  };
  assert.match(bad(""), /at least one change/);
  assert.match(bad("  \n "), /at least one change/);
  assert.match(bad("1 00"), /Line 1: write the offset, the old bytes and the new bytes/);
  assert.match(bad("1 00 01 02"), /Line 1: write the offset/);
  assert.match(bad("1 00 01\nzz 00 01"), /Line 2: the offset “zz” is not a number/);
  assert.match(bad("-1 00 01"), /offset “-1” is not a number/);
  assert.match(bad("0x 00 01"), /offset “0x” is not a number/);
  assert.match(bad("1 0 01"), /the old bytes “0” must be hexadecimal, two digits per byte/);
  assert.match(bad("1 00 0g"), /the new bytes “0g” must be hexadecimal/);
  assert.match(bad("99999999999999999999 00 01"), /too large/);
});

test("hunks written back as text read back as the same hunks", () => {
  const hunks = [
    { offset: 4, old_hex: "00", new_hex: "01" },
    { offset: 4096, old_hex: "aabb", new_hex: "ccdd" },
  ];
  assert.deepEqual(parseHunks(hunksToText(hunks)), { hunks });
});

test("each step is described in the owner's own words, and a runtime-code step says SUL cannot run it", () => {
  const steps: Step[] = [
    { op: "layer_mod", mod_id: "louder", version: "1.0", into: "" },
    { op: "layer_mod", mod_id: "louder", version: "1.0", into: "data" },
    { op: "patch_file", path: "data/a.bin", patch: { id: "p", description: "d", expect_sha256: HEX64, hunks: [{ offset: 1, old_hex: "00", new_hex: "01" }] } },
    { op: "runtime_code", description: "hook the audio loader" },
  ];
  const [top, into, patch, runtime] = steps.map(describeStep);
  assert.equal(top, "Layer mod louder 1.0 at the top of the workspace");
  assert.equal(into, "Layer mod louder 1.0 into data");
  assert.match(patch!, /^Patch data\/a\.bin: 1 change \(only for the file whose SHA-256 starts abababababab…\)$/);
  assert.match(runtime!, /cannot be run by SUL.*hook the audio loader/);
});

test("short hashes, run keys, plan words and the game check", () => {
  assert.equal(shortHash("abc"), "abc");
  assert.equal(shortHash(HEX64), "abababababab…");
  assert.notEqual(runKey("r", { root_id: "a", run_id: "1" }), runKey("r", { root_id: "a1", run_id: "" }), "a key cannot be forged by moving a piece across");
  assert.equal(describePlan(aPlan()), "1 file, 4 B, from 1 mod and 0 patches.");
  assert.equal(describeVerifyRun({ missing: [], modified: [], extra: [] }), "Matches its record.");
  assert.match(describeVerifyRun({ missing: ["a"], modified: ["b", "c"], extra: [] }), /Does not match its record: 1 missing, 2 changed, 0 not in the record/);
  const v = { recipes: [], mods: [], mod_roots: [], deployable_games: [GAME], run_errors: [] };
  assert.equal(canPlanGame(v, GAME), true);
  assert.equal(canPlanGame(v, "other"), false);
});

test("a view is accepted only if it has the lists the page reads", () => {
  assert.equal(isBuilderView({ recipes: [], mods: [], mod_roots: [], deployable_games: [], run_errors: [] }), true);
  for (const junk of [null, [], "x", {}, { recipes: [], mods: [], mod_roots: [], deployable_games: [] }, { recipes: {}, mods: [], mod_roots: [], deployable_games: [], run_errors: [] }]) {
    assert.equal(isBuilderView(junk), false);
  }
});

// ---------------------------------------------------------------------------
// A draft becomes a recipe, or says what is wrong
// ---------------------------------------------------------------------------

test("a draft becomes the recipe it describes, with the id made from its name and the hash lower-cased", () => {
  const r = recipeOf(draft([layer, patchStep({ expect_sha256: ` ${HEX64.toUpperCase()} `, path: " data/a.bin " })], { name: " Loud Mix! ", description: " for me " }));
  assert.ok("recipe" in r, JSON.stringify(r));
  assert.deepEqual(r.recipe, {
    schema: 1,
    id: "loud-mix",
    name: "Loud Mix!",
    game_id: GAME,
    description: "for me",
    steps: [
      { op: "layer_mod", mod_id: "louder", version: "1.0", into: "" },
      { op: "patch_file", path: "data/a.bin", patch: { id: "louder-ui", description: "makes it louder", expect_sha256: HEX64, hunks: [{ offset: 16, old_hex: "00ff", new_hex: "01fe" }] } },
    ],
  });
});

test("every kind of mistake in a draft is named, with the step it is in", () => {
  const bad = (d: Draft) => {
    const r = recipeOf(d);
    assert.ok("bad" in r, "expected the draft to be refused");
    return r.bad;
  };
  assert.match(bad(draft([layer], { name: "  " })), /Give the recipe a name\./);
  assert.match(bad(draft([layer], { name: "!!!" })), /some letters or digits/);
  assert.match(bad(draft([layer], { game_id: "" })), /Pick a game first/);
  assert.match(bad(draft([])), /Add at least one step/);
  assert.match(bad(draft([layer, { ...layer, mod_id: "", version: "" }])), /Step 2: pick the mod to layer/);
  assert.match(bad(draft([patchStep({ path: " " })])), /Step 1: write the path of the file to patch/);
  assert.match(bad(draft([patchStep({ patch_name: "--" })])), /Step 1: give the patch a name/);
  assert.match(bad(draft([patchStep({ expect_sha256: "abc" })])), /Step 1: the file's SHA-256 must be 64 hexadecimal digits/);
  assert.match(bad(draft([patchStep({ expect_sha256: "g".repeat(64) })])), /64 hexadecimal digits/);
  assert.match(bad(draft([layer, patchStep({ hunks: "1 00" })])), /Step 2: Line 1: write the offset/);
  assert.match(bad(draft([{ op: "runtime_code", description: " " }])), /Step 1: say what the runtime-code change is/);
});

test("a saved recipe becomes a draft that keeps its id and its hash, and reads back as the same recipe", () => {
  const recipe = aRecipe(GAME, "loud", {
    name: "Loud",
    description: "d",
    steps: [
      { op: "layer_mod", mod_id: "louder", version: "1.0", into: "data" },
      { op: "patch_file", path: "data/a.bin", patch: { id: "louder-ui", description: "x", expect_sha256: HEX64, hunks: [{ offset: 16, old_hex: "00ff", new_hex: "01fe" }] } },
      { op: "runtime_code", description: "hook" },
    ],
  });
  const rv = recipeView(recipe);
  const d = draftOf(rv);
  assert.deepEqual(d.editing, { id: "loud", sha256: rv.sha256 });
  assert.deepEqual(recipeOf(d), { recipe });
  // The name of a recipe being changed does not change its id: the saved recipe is found by the id.
  assert.deepEqual(recipeOf({ ...d, name: "Renamed" }), { recipe: { ...recipe, name: "Renamed" } });
});

// ---------------------------------------------------------------------------
// The controller
// ---------------------------------------------------------------------------

test("with no backend the controller is loaded, has no view, and every action is refused without sending anything", async () => {
  const c = new BuilderController(null);
  await c.load();
  assert.equal(c.hasBackend, false);
  assert.equal(c.getSnapshot().loaded, true);
  assert.equal(c.getSnapshot().view, null);
  assert.equal(await c.saveRecipe(aRecipe(GAME), null), false);
  assert.equal(await c.previewRun("loud", "modlib"), false);
  assert.equal(await c.confirmRun(), false);
});

test("a view that cannot be read leaves the page loaded with the reason", async () => {
  const fake = world();
  fake.failNext("view", "the recipe list is damaged");
  const c = await loaded(fake);
  assert.equal(c.getSnapshot().loaded, true);
  assert.equal(c.getSnapshot().view, null);
  assert.equal(c.getSnapshot().error, "the recipe list is damaged");
});

test("a draft with a mistake is refused before anything is sent", async () => {
  const fake = world();
  const c = await loaded(fake);
  assert.equal(await c.saveDraft(draft([patchStep({ expect_sha256: "nope" })])), false);
  assert.match(c.getSnapshot().error ?? "", /64 hexadecimal digits/);
  assert.equal(await c.saveRecipe({ ...aRecipe(GAME), id: "" }, null), false);
  assert.equal(await c.saveRecipe({ ...aRecipe(GAME), name: " " }, null), false);
  assert.equal(await c.saveRecipe({ ...aRecipe(GAME), game_id: "" }, null), false);
  assert.equal(await c.saveRecipe({ ...aRecipe(GAME), steps: [] }, null), false);
  assert.deepEqual(fake.changes(), []);
});

test("a new recipe is saved without naming a recipe to replace, and a change names the hash it started from", async () => {
  const fake = world();
  const c = await loaded(fake);
  assert.equal(await c.saveDraft(draft([layer])), true);
  assert.equal(fake.changes().length, 1);
  assert.equal(fake.changes()[0]!.op, "saveRecipe");
  assert.equal((fake.changes()[0]!.args as unknown[])[1], null);
  assert.match(c.getSnapshot().notice ?? "", /Saved the recipe “Loud”.*Nothing was built/);
  const saved = c.getSnapshot().view!.recipes[0]!;

  assert.equal(await c.saveDraft({ ...draftOf(saved), description: "changed" }), true);
  assert.equal((fake.changes()[1]!.args as unknown[])[1], saved.sha256, "the change names the recipe it started from");
  assert.equal(c.getSnapshot().view!.recipes[0]!.recipe.description, "changed");
});

test("a change from a stale copy is refused by the core, the page keeps what it had and says why", async () => {
  const fake = world();
  const c = await loaded(fake);
  await c.saveDraft(draft([layer]));
  const stale = draftOf(c.getSnapshot().view!.recipes[0]!);
  await c.saveDraft({ ...stale, description: "first" }); // someone else changed it meanwhile
  const before = c.getSnapshot().view;
  assert.equal(await c.saveDraft({ ...stale, description: "second" }), false);
  assert.match(c.getSnapshot().error ?? "", /changed since it was opened/);
  assert.equal(c.getSnapshot().view, before, "the failed change did not touch the data");
  assert.equal(c.getSnapshot().view!.recipes[0]!.recipe.description, "first");
});

test("a new recipe cannot take the id of a saved one: the refusal reaches the page", async () => {
  const fake = world({ recipes: [recipeView(aRecipe(GAME, "loud"))] });
  const c = await loaded(fake);
  assert.equal(await c.saveDraft(draft([layer], { name: "Loud" })), false);
  assert.match(c.getSnapshot().error ?? "", /already exists/);
  assert.equal(c.getSnapshot().view!.recipes.length, 1);
});

test("a preview only plans: nothing is built until it is confirmed, and cancelling builds nothing", async () => {
  const fake = world({ recipes: [recipeView(aRecipe(GAME, "loud"))] });
  const c = await loaded(fake);
  assert.equal(await c.previewRun("loud", "modlib"), true);
  assert.deepEqual(fake.calls.map((x) => x.op), ["view", "planRecipe"]);
  assert.equal(c.getSnapshot().preview?.recipeId, "loud");
  assert.equal(c.getSnapshot().preview?.rootId, "modlib");
  c.closePreview();
  assert.equal(c.getSnapshot().preview, null);
  assert.deepEqual(fake.changes(), []);
  assert.equal(await c.confirmRun(), false, "nothing to confirm once the preview is closed");
  assert.deepEqual(fake.changes(), []);
});

test("confirming builds the run in the root that was previewed, and the new run shows up in the view", async () => {
  const fake = world({ recipes: [recipeView(aRecipe(GAME, "loud"))], mod_roots: [modRoot("a", "Library A"), modRoot("b", "Library B")] });
  const c = await loaded(fake);
  await c.previewRun("loud", "b");
  assert.equal(await c.confirmRun(), true);
  assert.deepEqual(fake.changes(), [{ op: "runRecipe", args: ["loud", "b"] }]);
  assert.equal(c.getSnapshot().preview, null);
  assert.deepEqual(c.getSnapshot().view!.recipes[0]!.runs, [{ root_id: "b", run_id: "run-1" }]);
  assert.match(c.getSnapshot().notice ?? "", /Built 1 file into the workspace of “Library B” as run run-1\. No game was changed and nothing was deleted\./);
});

test("a blocked plan cannot be confirmed: nothing is sent, and the preview stays so the problems can be read", async () => {
  const fake = world({ recipes: [recipeView(aRecipe(GAME, "loud"))] });
  fake.setPlan(aPlan({ problems: ["the patch does not fit data/a.bin"] }));
  const c = await loaded(fake);
  await c.previewRun("loud", "modlib");
  assert.equal(await c.confirmRun(), false);
  assert.match(c.getSnapshot().error ?? "", /blocked, so nothing was built/);
  assert.deepEqual(fake.changes(), []);
  assert.deepEqual(c.getSnapshot().preview?.plan.problems, ["the patch does not fit data/a.bin"]);
});

test("a run is refused for a game with no adapter and for a recipe that changes game code, and the refusal is text", async () => {
  const fake = world({
    recipes: [recipeView(aRecipe("other-game", "loud")), recipeView(aRecipe(GAME, "hook", { steps: [{ op: "runtime_code", description: "hook" }] }))],
  });
  const c = await loaded(fake);
  assert.equal(await c.previewRun("loud", "modlib"), false);
  assert.match(c.getSnapshot().error ?? "", /no adapter for “other-game”/);
  assert.equal(await c.previewRun("hook", "modlib"), false);
  assert.match(c.getSnapshot().error ?? "", /changes game code at run time/);
  assert.equal(c.getSnapshot().preview, null);
  assert.deepEqual(fake.changes(), []);
});

test("a preview with no library folder is refused with what to do, without asking the backend", async () => {
  const fake = world({ recipes: [recipeView(aRecipe(GAME, "loud"))], mod_roots: [] });
  const c = await loaded(fake);
  assert.equal(await c.previewRun("loud", ""), false);
  assert.match(c.getSnapshot().error ?? "", /Add a library folder for mods first/);
  assert.deepEqual(fake.calls.map((x) => x.op), ["view"]);
});

test("only one operation runs at a time", async () => {
  const fake = world({ recipes: [recipeView(aRecipe(GAME, "loud"))] });
  const c = await loaded(fake);
  const release = fake.hold();
  const first = c.previewRun("loud", "modlib");
  assert.equal(c.getSnapshot().busy, "preview");
  assert.equal(await c.previewRun("loud", "modlib"), false, "a second operation is not started while one runs");
  release();
  assert.equal(await first, true);
  assert.equal(c.getSnapshot().busy, null);
  assert.equal(fake.calls.filter((x) => x.op === "planRecipe").length, 1);
});

test("a failed run says why and leaves the preview open and the data as it was", async () => {
  const fake = world({ recipes: [recipeView(aRecipe(GAME, "loud"))] });
  const c = await loaded(fake);
  await c.previewRun("loud", "modlib");
  const before = c.getSnapshot().view;
  fake.failNext("runRecipe", "the mod louder 1.0 is no longer stored");
  assert.equal(await c.confirmRun(), false);
  assert.equal(c.getSnapshot().error, "the mod louder 1.0 is no longer stored");
  assert.equal(c.getSnapshot().view, before);
  assert.notEqual(c.getSnapshot().preview, null);
  assert.equal(c.getSnapshot().busy, null);
});

test("a run's record can be opened and closed, and its files checked against the record", async () => {
  const run = { root_id: "modlib", run_id: "run-1" };
  const fake = world({ recipes: [recipeView(aRecipe(GAME, "loud"), { runs: [run] })] });
  const c = await loaded(fake);
  assert.equal(await c.showRun("loud", run), true);
  assert.equal(c.getSnapshot().openRun?.record.run_id, "run-1");
  c.closeRun();
  assert.equal(c.getSnapshot().openRun, null);
  assert.equal(await c.verifyRun("loud", run), true);
  assert.deepEqual(c.getSnapshot().checks[runKey("loud", run)], { missing: [], modified: [], extra: [] });
  assert.deepEqual(fake.changes(), [], "looking at a run changes nothing");
});

test("a record that is not there is a plain refusal", async () => {
  const fake = world({ recipes: [recipeView(aRecipe(GAME, "loud"))] });
  const c = await loaded(fake);
  assert.equal(await c.showRun("loud", { root_id: "modlib", run_id: "gone" }), false);
  assert.match(c.getSnapshot().error ?? "", /there is no run “gone”/);
});

test("looking up a mod's files passes the filter and keeps how many matched out of how many there are", async () => {
  const fake = world();
  fake.setFiles([
    { path: "data/a.bin", sha256: HEX64, size: 4 },
    { path: "data/b.txt", sha256: "cd".repeat(32), size: 2 },
  ]);
  const c = await loaded(fake);
  assert.equal(await c.lookUpFiles({ game_id: GAME, mod_id: "louder", version: "1.0" }, ".bin"), true);
  assert.deepEqual(fake.calls.at(-1), { op: "modFiles", args: [GAME, "louder", "1.0", ".bin"] });
  assert.equal(c.getSnapshot().files?.matching, 1);
  assert.equal(c.getSnapshot().files?.total, 2);
  c.clearFiles();
  assert.equal(c.getSnapshot().files, null);
});

test("listeners hear every change and stop hearing when they unsubscribe", async () => {
  const fake = world();
  const c = new BuilderController(fake.api);
  let heard = 0;
  const off = c.subscribe(() => (heard += 1));
  await c.load();
  assert.ok(heard >= 1);
  const before = heard;
  off();
  c.closePreview();
  assert.equal(heard, before);
});
