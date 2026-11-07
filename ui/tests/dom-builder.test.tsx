import { absent, act, choose, click, installTauri, mount, one, resetBrowser, settle, type, withText } from "./dom";
import assert from "node:assert/strict";
import { beforeEach, describe, test } from "node:test";
import { BuilderPanel } from "../src/components/BuilderPanel";
import { tauriBuilderApi } from "../src/lib/builder";
import ModsView from "../src/views/ModsView";
import type { LibraryApi } from "../src/lib/library";
import { gogGame, steamGame } from "./fixtures";
import { HEX64, aPlan, aRecipe, fakeBuilder, mod, modRoot, recipeView } from "./builder-fake";
import { fakeMods } from "./mods-fake";

beforeEach(() => resetBrowser());

const GAMES = [steamGame, gogGame];
const HD2 = steamGame.id;
const panel = (host: HTMLElement) => one(host, "section[aria-label='Mod builder and patches']");
const editor = (host: HTMLElement) => one(host, "div[aria-label='Write a recipe']");
const recipeCard = (host: HTMLElement, name = "Loud") => one(host, `li[aria-label='Recipe ${name}']`);
const stepCard = (host: HTMLElement, n: number) => one(editor(host), `li[aria-label='Step ${n}']`);
/** The input, select or textarea of the label whose own text (not its options') is `label`. */
const field = (f: HTMLElement, label: string) => {
  const found = [...f.querySelectorAll("label")].filter((l) => l.firstChild?.textContent?.trim() === label);
  if (found.length !== 1) throw new Error(`expected exactly one label ${JSON.stringify(label)}, found ${found.length}`);
  return found[0]!.querySelector("input, select, textarea") as HTMLInputElement & HTMLSelectElement & HTMLTextAreaElement;
};
/** `type` from the helpers sets an input's value; a textarea has its own setter, so it needs its own few lines. */
async function typeInto(area: HTMLTextAreaElement, value: string) {
  await act(async () => {
    const setter = Object.getOwnPropertyDescriptor(window.HTMLTextAreaElement.prototype, "value")?.set;
    if (!setter) throw new Error("no value setter on HTMLTextAreaElement");
    setter.call(area, value);
    area.dispatchEvent(new window.Event("input", { bubbles: true }));
  });
}
const dialog = (host: HTMLElement) => one(host, "[role=dialog]");
const noDialog = (host: HTMLElement) => absent(host.querySelector("[role=dialog]"), "no question should be open");
const alertText = (host: HTMLElement) => [...host.querySelectorAll("[role=alert]")].map((e) => e.textContent).join(" | ");
const status = (host: HTMLElement) => one(panel(host), "[role=status]").textContent ?? "";
const louder = () => mod(HD2, "louder", "1.0", { name: "Louder" });

/** One game with an adapter, one mod, a library folder and one saved recipe. */
function world(extra: Parameters<typeof fakeBuilder>[0] = {}) {
  return {
    recipes: [recipeView(aRecipe(HD2, "loud", { name: "Loud" }))],
    mods: [louder()],
    mod_roots: [modRoot()],
    deployable_games: [HD2],
    ...extra,
  };
}

async function start(initial: Parameters<typeof fakeBuilder>[0] = world(), games = GAMES) {
  const fake = fakeBuilder(initial);
  const m = await mount(<BuilderPanel api={fake.api} games={games} />);
  await settle();
  return { fake, ...m };
}

/** Fill the editor with a game, a name and one "layer a mod" step. */
async function writeLayerRecipe(host: HTMLElement, name = "Quiet Mix") {
  await choose(field(editor(host), "Game") as HTMLSelectElement, HD2);
  await type(field(editor(host), "Recipe name"), name);
  await click(withText(editor(host), "button", "Add: layer a mod"));
  await choose(field(stepCard(host, 1), "Layer this mod") as HTMLSelectElement, `${HD2}/louder@1.0`);
}

describe("the builder panel, clicked through", () => {
  test("in a plain browser it says it needs the desktop app and offers nothing to press", async () => {
    const { host, unmount } = await mount(<BuilderPanel api={null} games={GAMES} />);
    assert.match(panel(host).textContent ?? "", /Browser preview: the mod builder needs the desktop app/);
    absent(host.querySelector("button, input, select"), "nothing to operate without a backend");
    assert.equal(one(panel(host), ".badge").textContent, "Experimental");
    unmount();
  });

  test("it is labelled Experimental and says what it is, what it never touches, and that every real game is refused today", async () => {
    const { host, unmount } = await start();
    const badge = one(panel(host), ".badge");
    assert.equal(badge.textContent, "Experimental");
    assert.ok(badge.classList.contains("badge-experimental"));
    const text = panel(host).textContent ?? "";
    assert.match(text, /tested on made-up folders and files only/);
    assert.match(text, /Nothing here changes a game, and a patch only ever writes a new file/);
    assert.match(text, /no adapter for a real game exists yet, so previews and runs are refused for every real game today/);
    assert.match(text, /changes game code while the game runs can be written down but SUL cannot run it/);
    unmount();
  });

  test("a saved recipe lists its steps in the owner's words", async () => {
    const recipe = aRecipe(HD2, "loud", {
      name: "Loud",
      description: "for the quiet mission",
      steps: [
        { op: "layer_mod", mod_id: "louder", version: "1.0", into: "data" },
        { op: "patch_file", path: "data/a.bin", patch: { id: "p", description: "d", expect_sha256: HEX64, hunks: [{ offset: 1, old_hex: "00", new_hex: "01" }] } },
      ],
    });
    const { host, unmount } = await start(world({ recipes: [recipeView(recipe)] }));
    const card = recipeCard(host);
    assert.match(card.textContent ?? "", /Loud.*2 steps/);
    assert.match(card.textContent ?? "", /Helldivers 2.*for the quiet mission/);
    const steps = [...one(card, "ol[aria-label='Steps of Loud']").querySelectorAll("li")].map((l) => l.textContent);
    assert.deepEqual(steps, ["Layer mod louder 1.0 into data", `Patch data/a.bin: 1 change (only for the file whose SHA-256 starts ${HEX64.slice(0, 12)}…)`]);
    unmount();
  });

  test("a recipe for a game with no adapter can be kept but not previewed, and says so", async () => {
    const { host, unmount } = await start(world({ recipes: [recipeView(aRecipe(gogGame.id, "loud", { name: "Loud" }))] }));
    const card = recipeCard(host);
    assert.match(card.textContent ?? "", /This game has no adapter yet, so the recipe can be saved but not previewed or run/);
    assert.equal(withText<HTMLButtonElement>(card, "button", "Preview run…").disabled, true);
    unmount();
  });

  test("a recipe that changes game code is flagged and cannot be previewed, however the page is pressed", async () => {
    const hook = aRecipe(HD2, "hook", { name: "Hook", steps: [{ op: "runtime_code", description: "hook the audio loader" }] });
    const { fake, host, unmount } = await start(world({ recipes: [recipeView(hook)] }));
    const card = recipeCard(host, "Hook");
    assert.match(card.textContent ?? "", /Changes game code: SUL cannot run it/);
    assert.match(card.textContent ?? "", /SUL does not change game code, so this recipe cannot be previewed or run/);
    const preview = withText<HTMLButtonElement>(card, "button", "Preview run…");
    assert.equal(preview.disabled, true);
    await click(preview);
    assert.deepEqual(fake.calls.map((c) => c.op), ["view"], "nothing was asked of the backend");
    unmount();
  });

  test("runs that could not be read are shown as problems rather than hidden", async () => {
    const { host, unmount } = await start(world({ run_errors: ["run old-1 of “loud” has a damaged record"] }));
    assert.match(alertText(host), /run old-1 of “loud” has a damaged record/);
    unmount();
  });
});

describe("writing a recipe", () => {
  test("typing and adding steps sends nothing; saving sends the recipe, new, with no recipe to replace", async () => {
    const { fake, host, unmount } = await start(world({ recipes: [] }));
    await writeLayerRecipe(host);
    await type(field(editor(host), "What it is for"), "for me");
    await type(field(stepCard(host, 1), "Into this folder of the workspace (blank for the top)"), "data");
    assert.deepEqual(fake.changes(), [], "nothing is saved until the person says so");
    await click(withText(editor(host), "button", "Save the recipe"));
    assert.deepEqual(fake.changes(), [
      {
        op: "saveRecipe",
        args: [{ schema: 1, id: "quiet-mix", name: "Quiet Mix", game_id: HD2, description: "for me", steps: [{ op: "layer_mod", mod_id: "louder", version: "1.0", into: "data" }] }, null],
      },
    ]);
    assert.match(status(host), /Saved the recipe “Quiet Mix”\. Nothing was built, and no mod or game file was read\./);
    assert.match(recipeCard(host, "Quiet Mix").textContent ?? "", /quiet-mix/);
    assert.equal(field(editor(host), "Recipe name").value, "", "the editor is empty again");
    unmount();
  });

  test("a draft with a mistake is refused with the reason and nothing is sent; the draft is kept", async () => {
    const { fake, host, unmount } = await start(world({ recipes: [] }));
    await click(withText(editor(host), "button", "Save the recipe"));
    assert.match(alertText(host), /Give the recipe a name\./);
    await type(field(editor(host), "Recipe name"), "Quiet Mix");
    await click(withText(editor(host), "button", "Save the recipe"));
    assert.match(alertText(host), /Pick a game first/);
    await choose(field(editor(host), "Game") as HTMLSelectElement, HD2);
    await click(withText(editor(host), "button", "Save the recipe"));
    assert.match(alertText(host), /Add at least one step/);
    await click(withText(editor(host), "button", "Add: layer a mod"));
    await click(withText(editor(host), "button", "Save the recipe"));
    assert.match(alertText(host), /Step 1: pick the mod to layer/);
    assert.deepEqual(fake.changes(), []);
    assert.equal(field(editor(host), "Recipe name").value, "Quiet Mix", "what was typed is still there");
    unmount();
  });

  test("a patch step: look up a mod's file, use it, write the changes, and the saved patch carries that file's hash and the hunks", async () => {
    const { fake, host, unmount } = await start(world({ recipes: [] }));
    fake.setFiles([
      { path: "data/a.bin", sha256: HEX64, size: 4 },
      { path: "data/b.txt", sha256: "cd".repeat(32), size: 2 },
    ]);
    await choose(field(editor(host), "Game") as HTMLSelectElement, HD2);
    await type(field(editor(host), "Recipe name"), "Patched");
    await click(withText(editor(host), "button", "Add: patch a file"));
    const step = stepCard(host, 1);
    await type(field(step, "Words in the file's path"), "bin");
    await click(one(step, "button[aria-label='List files of Louder 1.0 for step 1']"));
    assert.deepEqual(fake.calls.at(-1), { op: "modFiles", args: [HD2, "louder", "1.0", "bin"] });
    assert.match(step.textContent ?? "", /1 matching \(the mod has 2\)/);
    assert.match(one(step, "ul[aria-label='Files of the mod']").textContent ?? "", /data\/a\.bin.*abababababab…/);
    await click(one(step, "button[aria-label='Use data/a.bin in step 1']"));
    assert.equal(field(step, "File to patch (path in the workspace)").value, "data/a.bin");
    assert.equal(field(step, "The file's SHA-256").value, HEX64);
    await type(field(step, "Patch name"), "Louder UI");
    await type(field(step, "What it does"), "makes it louder");
    await typeInto(field(step, "Changes: one per line, as offset, old bytes, new bytes (hexadecimal, same length)") as HTMLTextAreaElement, "0x10 00FF 01fe");
    await click(withText(editor(host), "button", "Save the recipe"));
    assert.deepEqual(fake.changes(), [
      {
        op: "saveRecipe",
        args: [
          {
            schema: 1,
            id: "patched",
            name: "Patched",
            game_id: HD2,
            description: "",
            steps: [{ op: "patch_file", path: "data/a.bin", patch: { id: "louder-ui", description: "makes it louder", expect_sha256: HEX64, hunks: [{ offset: 16, old_hex: "00ff", new_hex: "01fe" }] } }],
          },
          null,
        ],
      },
    ]);
    unmount();
  });

  test("a bad hunk is refused with its line number and nothing is sent", async () => {
    const { fake, host, unmount } = await start(world({ recipes: [] }));
    await choose(field(editor(host), "Game") as HTMLSelectElement, HD2);
    await type(field(editor(host), "Recipe name"), "Patched");
    await click(withText(editor(host), "button", "Add: patch a file"));
    const step = stepCard(host, 1);
    await type(field(step, "File to patch (path in the workspace)"), "data/a.bin");
    await type(field(step, "Patch name"), "p");
    await type(field(step, "The file's SHA-256"), HEX64);
    await typeInto(field(step, "Changes: one per line, as offset, old bytes, new bytes (hexadecimal, same length)") as HTMLTextAreaElement, "1 00 01\n2 0 01");
    await click(withText(editor(host), "button", "Save the recipe"));
    assert.match(alertText(host), /Step 1: Line 2: the old bytes “0” must be hexadecimal/);
    assert.deepEqual(fake.changes(), []);
    unmount();
  });

  test("steps can be moved and removed, and a file lookup is dropped when the steps change places so it cannot sit under another step", async () => {
    const { fake, host, unmount } = await start(world({ recipes: [] }));
    fake.setFiles([{ path: "data/a.bin", sha256: HEX64, size: 4 }]);
    await choose(field(editor(host), "Game") as HTMLSelectElement, HD2);
    await click(withText(editor(host), "button", "Add: patch a file"));
    await click(withText(editor(host), "button", "Add: patch a file"));
    await type(field(stepCard(host, 1), "File to patch (path in the workspace)"), "first");
    await type(field(stepCard(host, 2), "File to patch (path in the workspace)"), "second");
    await click(one(stepCard(host, 1), "button[aria-label='List files of Louder 1.0 for step 1']"));
    assert.ok(stepCard(host, 1).querySelector("ul[aria-label='Files of the mod']"));
    await click(one(editor(host), "button[aria-label='Move step 1 down']"));
    assert.equal(field(stepCard(host, 1), "File to patch (path in the workspace)").value, "second");
    assert.equal(field(stepCard(host, 2), "File to patch (path in the workspace)").value, "first");
    absent(editor(host).querySelector("ul[aria-label='Files of the mod']"), "the lookup belonged to the step that moved");
    assert.equal(one<HTMLButtonElement>(editor(host), "button[aria-label='Move step 1 up']").disabled, true);
    assert.equal(one<HTMLButtonElement>(editor(host), "button[aria-label='Move step 2 down']").disabled, true);
    await click(one(editor(host), "button[aria-label='Remove step 1']"));
    assert.equal(editor(host).querySelectorAll("ol[aria-label='Steps of the recipe being written'] > li").length, 1);
    assert.equal(field(stepCard(host, 1), "File to patch (path in the workspace)").value, "first");
    assert.deepEqual(fake.changes(), []);
    unmount();
  });

  test("Change… opens a saved recipe; saving names the recipe it started from, and the game and the name stay as they were", async () => {
    const saved = recipeView(aRecipe(HD2, "loud", { name: "Loud", description: "old" }));
    const { fake, host, unmount } = await start(world({ recipes: [saved] }));
    await click(one(host, "button[aria-label='Change Loud']"));
    assert.match(editor(host).textContent ?? "", /Change the recipe “Loud”/);
    assert.equal(field(editor(host), "Game").disabled, true);
    assert.equal(field(editor(host), "Recipe name").disabled, true);
    assert.equal(field(editor(host), "What it is for").value, "old");
    await type(field(editor(host), "What it is for"), "new");
    await click(withText(editor(host), "button", "Save the change"));
    assert.equal(fake.changes().length, 1);
    const [recipe, replaces] = fake.changes()[0]!.args as [{ id: string; description: string }, string];
    assert.equal(recipe.id, "loud");
    assert.equal(recipe.description, "new");
    assert.equal(replaces, saved.sha256);
    assert.match(recipeCard(host).textContent ?? "", /new/);
    assert.match(editor(host).textContent ?? "", /Write a recipe/, "the editor is empty again");
    unmount();
  });

  test("a change the core refuses as stale shows the reason and keeps the draft, so nothing typed is lost", async () => {
    const { fake, host, unmount } = await start();
    await click(one(host, "button[aria-label='Change Loud']"));
    await type(field(editor(host), "What it is for"), "my change");
    fake.failNext("saveRecipe", "recipe “loud” changed since it was opened; open it again and redo the change");
    await click(withText(editor(host), "button", "Save the change"));
    assert.match(alertText(host), /changed since it was opened/);
    assert.equal(field(editor(host), "What it is for").value, "my change");
    assert.match(editor(host).textContent ?? "", /Change the recipe “Loud”/);
    unmount();
  });

  test("Start over drops the draft without sending anything", async () => {
    const { fake, host, unmount } = await start(world({ recipes: [] }));
    await writeLayerRecipe(host);
    await click(withText(editor(host), "button", "Start over"));
    assert.equal(field(editor(host), "Recipe name").value, "");
    assert.equal(editor(host).querySelectorAll("ol[aria-label='Steps of the recipe being written'] > li").length, 0);
    assert.deepEqual(fake.changes(), []);
    unmount();
  });
});

describe("previewing and building a run", () => {
  const previewButton = (host: HTMLElement) => one<HTMLButtonElement>(recipeCard(host), "button[aria-label='Preview a run of Loud']");

  test("Preview run… only plans: it shows what would be built, and builds nothing", async () => {
    const { fake, host, unmount } = await start();
    fake.setPlan(
      aPlan({
        patches: [{ step: 1, path: "data/a.bin", outcome: { patch_id: "louder-ui", before_sha256: HEX64, after_sha256: "cd".repeat(32), hunks: 2, bytes_changed: 3 } }],
        files: [{ path: "data/a.bin", sha256: "cd".repeat(32), size: 4, origin: "step 2: patch louder-ui", replaced: ["step 1: louder 1.0"] }],
        warnings: ["the mod louder says it works with 7.1.1 only"],
      }),
    );
    await click(previewButton(host));
    assert.deepEqual(fake.calls.map((c) => c.op), ["view", "planRecipe"]);
    assert.deepEqual(fake.calls[1]!.args, ["loud"]);
    const d = dialog(host);
    assert.match(d.textContent ?? "", /Build “Loud”\?/);
    assert.match(d.textContent ?? "", /new workspace folder in “Mod library” for Helldivers 2\. No game file is touched, no mod is changed and nothing is deleted/);
    assert.match(d.textContent ?? "", /1 file, 4 B, from 1 mod and 1 patch\./);
    assert.match(one(d, "ul[aria-label='Mods layered']").textContent ?? "", /louder 1\.0 · from made by hand · licence note \(your words, not checked\): mine/);
    assert.match(one(d, "ul[aria-label='Patches applied']").textContent ?? "", /Step 2: louder-ui on data\/a\.bin · 2 changes, 3 bytes/);
    assert.match(one(d, "ul[aria-label='Files the run builds']").textContent ?? "", /data\/a\.bin · 4 B · from step 2: patch louder-ui · replaces the same path from step 1: louder 1\.0/);
    assert.match(one(d, "ul[aria-label='Things to check before this run']").textContent ?? "", /works with 7\.1\.1 only/);
    assert.deepEqual(fake.changes(), []);
    unmount();
  });

  test("a preview of a very large run shows the first 200 files and says how many more there are", async () => {
    const { fake, host, unmount } = await start();
    const files = Array.from({ length: 205 }, (_, i) => ({ path: `data/f${String(i).padStart(3, "0")}.bin`, sha256: "cd".repeat(32), size: 1, origin: "step 1: louder 1.0", replaced: [] as string[] }));
    fake.setPlan(aPlan({ files, total_bytes: 205 }));
    await click(previewButton(host));
    const d = dialog(host);
    assert.equal(one(d, "ul[aria-label='Files the run builds']").querySelectorAll("li").length, 200);
    assert.match(d.textContent ?? "", /…and 5 more files\. SUL still builds and records all of them\./);
    assert.match(d.textContent ?? "", /205 files, 205 B/);
    unmount();
  });

  test("Cancel and Escape close the preview and build nothing", async () => {
    const { fake, host, unmount } = await start();
    await click(previewButton(host));
    await click(withText(dialog(host), "button", "Cancel"));
    noDialog(host);
    await click(previewButton(host));
    await act(async () => {
      window.dispatchEvent(new window.KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    });
    noDialog(host);
    assert.deepEqual(fake.changes(), []);
    unmount();
  });

  test("confirming builds the previewed recipe into the chosen library, and the new run appears under the recipe", async () => {
    const { fake, host, unmount } = await start(world({ mod_roots: [modRoot("a", "Library A"), modRoot("b", "Library B")] }));
    await choose(one<HTMLSelectElement>(one(host, "div[aria-label='Recipes']"), "select"), "b");
    await click(previewButton(host));
    assert.match(dialog(host).textContent ?? "", /new workspace folder in “Library B”/);
    await click(withText(dialog(host), "button", "Build into the workspace"));
    assert.deepEqual(fake.changes(), [{ op: "runRecipe", args: ["loud", "b"] }]);
    noDialog(host);
    assert.match(status(host), /Built 1 file into the workspace of “Library B” as run run-1\. No game was changed and nothing was deleted\./);
    assert.match(one(recipeCard(host), "ul[aria-label='Runs of Loud']").textContent ?? "", /Run run-1 in “Library B”/);
    unmount();
  });

  test("a blocked plan lists its problems and the Build button cannot be pressed", async () => {
    const { fake, host, unmount } = await start();
    fake.setPlan(aPlan({ problems: ["the patch does not fit data/a.bin: the bytes there are not the old bytes"] }));
    await click(previewButton(host));
    assert.match(one(dialog(host), "ul[aria-label='Problems that block this run']").textContent ?? "", /does not fit data\/a\.bin/);
    const build = withText<HTMLButtonElement>(dialog(host), "button", "Build into the workspace");
    assert.equal(build.disabled, true);
    await click(build);
    assert.deepEqual(fake.changes(), []);
    unmount();
  });

  test("a refusal from the backend is shown as text and no question opens", async () => {
    const { fake, host, unmount } = await start();
    fake.failNext("planRecipe", "the mod louder 1.0 is not stored");
    await click(previewButton(host));
    assert.match(alertText(host), /the mod louder 1\.0 is not stored/);
    noDialog(host);
    unmount();
  });

  test("a run that fails when confirmed says why, keeps the question open, and changes nothing on the page", async () => {
    const { fake, host, unmount } = await start();
    await click(previewButton(host));
    fake.failNext("runRecipe", "not enough free space in the mod library");
    await click(withText(dialog(host), "button", "Build into the workspace"));
    assert.match(alertText(host), /not enough free space in the mod library/);
    assert.ok(dialog(host));
    assert.equal(recipeCard(host).querySelector("ul[aria-label='Runs of Loud']"), null);
    unmount();
  });

  test("with no library folder the preview cannot be started and the page says what to add", async () => {
    const { fake, host, unmount } = await start(world({ mod_roots: [] }));
    assert.equal(previewButton(host).disabled, true);
    await click(previewButton(host));
    assert.deepEqual(fake.calls.map((c) => c.op), ["view"]);
    unmount();
  });

  test("while an operation runs the buttons are disabled and a second press sends nothing", async () => {
    const { fake, host, unmount } = await start();
    const release = fake.hold();
    await click(previewButton(host));
    assert.match(status(host), /Working… building a large run can take a while\./);
    assert.equal(previewButton(host).disabled, true);
    await click(previewButton(host));
    release();
    await settle();
    assert.equal(fake.calls.filter((c) => c.op === "planRecipe").length, 1);
    unmount();
  });
});

describe("finished runs", () => {
  const run = { root_id: "modlib", run_id: "run-1" };
  const withRun = () => world({ recipes: [recipeView(aRecipe(HD2, "loud", { name: "Loud" }), { runs: [run] })] });

  test("Check files compares the run with its record and shows the answer", async () => {
    const { fake, host, unmount } = await start(withRun());
    await click(one(recipeCard(host), "button[aria-label='Check the files of run run-1']"));
    assert.deepEqual(fake.calls.at(-1), { op: "verifyRun", args: ["loud", "modlib", "run-1"] });
    assert.match(recipeCard(host).textContent ?? "", /Last check: Matches its record\./);
    assert.deepEqual(fake.changes(), []);
    unmount();
  });

  test("a record of a very large run lists the first 200 files and says how many more there are", async () => {
    const { fake, host, unmount } = await start(withRun());
    fake.setPlan(aPlan({ files: Array.from({ length: 203 }, (_, i) => ({ path: `data/f${i}.bin`, sha256: "cd".repeat(32), size: 1, origin: "step 1", replaced: [] as string[] })) }));
    await click(one(recipeCard(host), "button[aria-label='Show the record of run run-1']"));
    const d = dialog(host);
    assert.equal(one(d, "ul[aria-label='Files of the run']").querySelectorAll("li").length, 200);
    assert.match(d.textContent ?? "", /…and 3 more files\./);
    unmount();
  });

  test("Show record opens what the run used and built, with the owner's notes marked as unchecked, and Close closes it", async () => {
    const { fake, host, unmount } = await start(withRun());
    await click(one(recipeCard(host), "button[aria-label='Show the record of run run-1']"));
    const d = dialog(host);
    assert.match(d.textContent ?? "", /Run run-1/);
    assert.match(d.textContent ?? "", /Recipe “Loud”, built 2023-11-14 22:13 UTC from 1 mod and 0 patches: 1 file\./);
    assert.match(one(d, "ul[aria-label='Mods the run used']").textContent ?? "", /licence note \(your words, not checked\): mine/);
    assert.match(one(d, "ul[aria-label='Files of the run']").textContent ?? "", /data\/a\.bin · 4 B/);
    await click(withText(d, "button", "Close"));
    noDialog(host);
    assert.deepEqual(fake.changes(), []);
    unmount();
  });
});

describe("what the panel sends to the real invoke", () => {
  const view = () => ({ recipes: [], mods: [], mod_roots: [], deployable_games: [], run_errors: [] });
  const record = () => ({ schema: 1, run_id: "r", started_unix: 1, recipe: aRecipe("g"), recipe_sha256: HEX64, inputs: [], patches: [], files: [], output_sha256: HEX64, warnings: [] });

  function answer(cmd: string): unknown {
    switch (cmd) {
      case "builder_plan_recipe":
        return aPlan();
      case "builder_run_recipe":
        return { record: record(), view: view() };
      case "builder_read_run":
        return record();
      case "builder_verify_run":
        return { missing: [], modified: [], extra: [] };
      case "builder_mod_files":
        return { files: [], matching: 0, total: 0 };
      default:
        return view();
    }
  }

  test("every command name and argument is the one the Rust commands take (camelCase top-level arguments, snake_case objects)", async () => {
    const tauri = installTauri(answer);
    const api = tauriBuilderApi();
    assert.ok(api, "inside the webview there is a backend");
    const recipe = aRecipe("g", "r");
    await api.view();
    await api.saveRecipe(recipe, null);
    await api.saveRecipe(recipe, HEX64);
    await api.planRecipe("r");
    await api.runRecipe("r", "root");
    await api.readRun("r", "root", "run-1");
    await api.verifyRun("r", "root", "run-1");
    await api.modFiles("g", "m", "1.0", "bin");
    assert.deepEqual(tauri.calls, [
      { cmd: "builder_view", args: {} },
      { cmd: "builder_save_recipe", args: { recipe, replaces: null } },
      { cmd: "builder_save_recipe", args: { recipe, replaces: HEX64 } },
      { cmd: "builder_plan_recipe", args: { recipeId: "r" } },
      { cmd: "builder_run_recipe", args: { recipeId: "r", rootId: "root" } },
      { cmd: "builder_read_run", args: { recipeId: "r", rootId: "root", runId: "run-1" } },
      { cmd: "builder_verify_run", args: { recipeId: "r", rootId: "root", runId: "run-1" } },
      { cmd: "builder_mod_files", args: { gameId: "g", modId: "m", version: "1.0", filter: "bin" } },
    ]);
    tauri.remove();
  });

  test("in a plain browser there is no backend", () => {
    assert.equal(tauriBuilderApi(), null);
  });

  test("data in the wrong shape is refused with a plain message instead of reaching the page", async () => {
    for (const junk of [[], null, "text", { recipes: [] }, { recipes: [], mods: [], mod_roots: [], deployable_games: [] }]) {
      const tauri = installTauri(() => junk);
      const api = tauriBuilderApi()!;
      await assert.rejects(api.view(), /does not understand/);
      await assert.rejects(api.saveRecipe(aRecipe("g"), null), /does not understand/);
      await assert.rejects(api.planRecipe("r"), /does not understand/);
      await assert.rejects(api.runRecipe("r", "root"), /does not understand/);
      await assert.rejects(api.readRun("r", "root", "x"), /does not understand/);
      await assert.rejects(api.verifyRun("r", "root", "x"), /does not understand/);
      await assert.rejects(api.modFiles("g", "m", "1", ""), /does not understand/);
      tauri.remove();
    }
  });

  test("a run answer needs both the record and a view, and each must be in shape", async () => {
    for (const junk of [{ record: record() }, { record: record(), view: {} }, { record: {}, view: view() }, { view: view() }]) {
      const tauri = installTauri(() => junk);
      await assert.rejects(tauriBuilderApi()!.runRecipe("r", "root"), /does not understand/);
      tauri.remove();
    }
  });

  test("a failing command reaches the page as its own text", async () => {
    const tauri = installTauri(() => {
      throw "recipe “r” changed since it was opened; open it again and redo the change";
    });
    const api = tauriBuilderApi()!;
    await assert.rejects(api.saveRecipe(aRecipe("g", "r"), HEX64), /changed since it was opened/);
    tauri.remove();
  });
});

describe("the Mods page", () => {
  const libraryApi = (): LibraryApi => ({
    list: async () => GAMES,
    scan: async () => ({ games: GAMES, added: 0, updated: 0, returned: 0, missing: 0 }),
    matrix: async () => {
      throw "not used here";
    },
    tick: async () => {
      throw "not used here";
    },
    play: async () => {
      throw "not used here";
    },
  });

  test("shows the builder panel after the mods panel, and the not-built note no longer names the builder, patches or external tools", async () => {
    const builder = fakeBuilder(world());
    const mods = fakeMods();
    const { host, unmount } = await mount(<ModsView api={mods.api} builderApi={builder.api} libraryApi={libraryApi()} />);
    await settle();
    const text = host.textContent ?? "";
    assert.match(text, /mod profiles, deploy\/purge, the builder, patches and external tools are built and Experimental/);
    assert.ok(text.indexOf("Mods and profiles") < text.indexOf("Mod builder and patches"));
    assert.ok(text.indexOf("Mod builder and patches") < text.indexOf("Not built yet"));
    assert.match(one(panel(host), "li[aria-label='Recipe Loud']").textContent ?? "", /Helldivers 2/, "the library's game names are used");
    assert.doesNotMatch(text, /external tools are not built/);
    assert.doesNotMatch(text, /mod builder, patches and external tools are not built yet/);
    unmount();
  });

  test("opened in a plain browser it shows the preview note for the builder and nothing to press in it", async () => {
    const { host, unmount } = await mount(<ModsView api={null} builderApi={null} libraryApi={null} />);
    await settle();
    assert.match(panel(host).textContent ?? "", /Browser preview: the mod builder needs the desktop app/);
    absent(panel(host).querySelector("button, input, select"), "nothing to operate without a backend");
    unmount();
  });
});
