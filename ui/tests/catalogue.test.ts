import { test } from "node:test";
import assert from "node:assert/strict";
import {
  CatalogueController,
  applyCategory,
  categoriesOf,
  filterSummary,
  findCategory,
  isCatalogueView,
  isGameDetail,
  plainPath,
  sameCategory,
  type CatalogueView,
} from "../src/lib/catalogue";
import { aDetail, fakeCatalogue } from "./catalogue-fake";
import { gogGame, steamGame } from "./fixtures";

const GAMES = [steamGame, gogGame];
const STEAM = steamGame.id;
const GOG = gogGame.id;

async function loaded(initial: Record<string, string[]> = {}) {
  const fake = fakeCatalogue(GAMES, initial);
  const c = new CatalogueController(fake.api);
  await c.load();
  return { fake, c };
}

/** Loaded, with the details of one game open. */
async function opened(id = GOG, initial: Record<string, string[]> = {}) {
  const r = await loaded(initial);
  await r.c.open(id);
  return r;
}

// ---------------------------------------------------------------------------
// Rules the page applies
// ---------------------------------------------------------------------------

test("two categories are the same when they differ only in ASCII case, as in the core", () => {
  assert.equal(sameCategory("RPG", "rpg"), true);
  assert.equal(sameCategory("Co-op", "CO-OP"), true);
  assert.equal(sameCategory("RPG", "RPGs"), false);
  // The core compares with `eq_ignore_ascii_case`, so an accented capital is not its lower case there, and is not here.
  assert.equal(sameCategory("É", "é"), false);
});

test("a category is found under the view's own spelling, and a name nobody carries is not found", () => {
  const view: CatalogueView = { categories: [{ name: "RPG", count: 1, game_ids: [GOG] }], entries: [] };
  assert.equal(findCategory(view, "rpg")?.name, "RPG");
  assert.equal(findCategory(view, "Racing"), null);
  assert.equal(findCategory(view, null), null);
  assert.equal(findCategory(null, "RPG"), null);
});

test("the category filter keeps the games that carry it, in the order given, and no choice keeps them all", () => {
  const view: CatalogueView = { categories: [{ name: "RPG", count: 1, game_ids: [GOG] }], entries: [] };
  assert.deepEqual(applyCategory(GAMES, view, null), GAMES);
  assert.deepEqual(applyCategory(GAMES, view, "rpg"), [gogGame]);
  assert.deepEqual(applyCategory([gogGame, steamGame], { categories: [{ name: "X", count: 2, game_ids: [STEAM, GOG] }], entries: [] }, "X"), [gogGame, steamGame], "the order of the games given, not of the ids");
  // A choice no game carries (it cannot be made from the page) filters nothing out.
  assert.deepEqual(applyCategory(GAMES, view, "Racing"), GAMES);
  assert.deepEqual(applyCategory(GAMES, null, "RPG"), GAMES);
});

test("a game's categories come from the view's entries", () => {
  const view: CatalogueView = { categories: [], entries: [{ game_id: GOG, categories: ["RPG", "Co-op"] }] };
  assert.deepEqual(categoriesOf(view, GOG), ["RPG", "Co-op"]);
  assert.deepEqual(categoriesOf(view, STEAM), []);
  assert.deepEqual(categoriesOf(null, GOG), []);
});

test("the sentence above the grid says what filter is on, and is nothing when none is", () => {
  assert.equal(filterSummary("", null, 2, 2), null);
  assert.equal(filterSummary("   ", null, 2, 2), null);
  // The wording for a search alone is the one the page always had.
  assert.equal(filterSummary(" alp ", null, 1, 2), "Showing 1 of 2 games matching “alp”.");
  assert.equal(filterSummary("zzz", null, 0, 2), "No games match “zzz”.");
  assert.equal(filterSummary("", "RPG", 1, 2), "Showing 1 of 2 games in “RPG”.");
  assert.equal(filterSummary("", "RPG", 0, 2), "No games are in “RPG”.");
  assert.equal(filterSummary("alp", "RPG", 1, 2), "Showing 1 of 2 games matching “alp” in “RPG”.");
  assert.equal(filterSummary("alp", "RPG", 0, 2), "No games match “alp” in “RPG”.");
});

test("a resolved Windows path is shown the way it was typed, and other paths are left alone", () => {
  assert.equal(plainPath("\\\\?\\C:\\Users\\me\\cover.png"), "C:\\Users\\me\\cover.png");
  assert.equal(plainPath("\\\\?\\UNC\\server\\share\\cover.png"), "\\\\?\\UNC\\server\\share\\cover.png");
  assert.equal(plainPath("D:\\Art\\cover.png"), "D:\\Art\\cover.png");
  assert.equal(plainPath("/home/me/cover.png"), "/home/me/cover.png");
});

test("data in the wrong shape is not accepted as a view or a detail", () => {
  const fake = fakeCatalogue(GAMES, { [GOG]: ["RPG"] });
  assert.equal(isCatalogueView(fake.view()), true);
  assert.equal(isGameDetail(fake.detail(GOG)), true);
  assert.equal(isGameDetail(aDetail(gogGame, { artwork: "a.png", adapter: { id: "gog-1", version: "1" } })), true);
  for (const bad of [null, 3, {}, { categories: [] }, { entries: [] }, { categories: [{ name: "x", count: "1", game_ids: [] }], entries: [] }, { categories: [{ name: "x", count: 1, game_ids: [3] }], entries: [] }, { categories: [], entries: [{ game_id: "x", categories: "RPG" }] }]) {
    assert.equal(isCatalogueView(bad), false, JSON.stringify(bad));
  }
  const good = aDetail(gogGame);
  for (const bad of [null, {}, { ...good, notes: undefined }, { ...good, artwork: undefined }, { ...good, categories: "RPG" }, { ...good, launch: {} }, { ...good, builds: [{}] }, { ...good, adapter: {} }, { ...good, capability_label: 3 }]) {
    assert.equal(isGameDetail(bad), false, JSON.stringify(bad));
  }
});

// ---------------------------------------------------------------------------
// The controller
// ---------------------------------------------------------------------------

test("with no backend nothing is asked, nothing is shown and nothing can be changed", async () => {
  const c = new CatalogueController(null);
  assert.equal(c.hasBackend, false);
  await c.load();
  assert.equal(c.getSnapshot().loaded, true);
  assert.equal(c.getSnapshot().view, null);
  assert.equal(c.getSnapshot().error, null);
  await c.open(GOG);
  assert.equal(c.getSnapshot().openId, null);
  assert.equal(await c.addCategory("RPG"), false);
  assert.equal(await c.setArtwork("a.png"), false);
});

test("loading gets the categories, and a failed load is shown without a view", async () => {
  const { c } = await loaded({ [GOG]: ["RPG"] });
  assert.equal(c.getSnapshot().loaded, true);
  assert.deepEqual(c.getSnapshot().view?.categories.map((x) => x.name), ["RPG"]);

  const fake = fakeCatalogue(GAMES);
  fake.failNext("view", "the build manager could not be opened: nope");
  const bad = new CatalogueController(fake.api);
  await bad.load();
  assert.equal(bad.getSnapshot().loaded, true);
  assert.equal(bad.getSnapshot().view, null);
  assert.match(bad.getSnapshot().error ?? "", /could not be opened: nope/);
  await bad.load();
  assert.equal(bad.getSnapshot().error, null, "the next load clears the error");
});

test("choosing a category filters by it, choosing none clears it, and a name nobody carries selects nothing", async () => {
  const { c } = await loaded({ [GOG]: ["RPG"] });
  c.select("rpg");
  assert.equal(c.getSnapshot().selected, "RPG", "the view's spelling is kept");
  c.select(null);
  assert.equal(c.getSnapshot().selected, null);
  c.select("Racing");
  assert.equal(c.getSnapshot().selected, null);
});

test("a chosen category stays chosen when its spelling changes, and is dropped when no game carries it any more", async () => {
  const { fake, c } = await loaded({ [GOG]: ["RPG"], [STEAM]: ["rpg"] });
  c.select("RPG");
  await c.open(GOG);
  await c.removeCategory("RPG");
  // Only the other game carries it now, under its own spelling.
  assert.equal(c.getSnapshot().selected, "rpg");
  assert.deepEqual(c.getSnapshot().view?.categories.map((x) => x.name), ["rpg"]);
  fake.give(STEAM, []);
  await c.load();
  assert.equal(c.getSnapshot().selected, null);
});

test("opening a game asks for its details, which arrive after the page says they are loading", async () => {
  const { fake, c } = await loaded();
  const release = fake.hold();
  const opening = c.open(GOG);
  assert.equal(c.getSnapshot().openId, GOG);
  assert.equal(c.getSnapshot().detail, null);
  release();
  await opening;
  assert.equal(c.getSnapshot().detail?.game_id, GOG);
  assert.equal(c.getSnapshot().detail?.display_name, "Alpha");
  assert.deepEqual(fake.calls.filter((x) => x.op === "detail").map((x) => x.args), [[GOG]]);
});

test("opening a second game replaces the first, and a late answer for the first is not shown", async () => {
  const { fake, c } = await loaded();
  const releaseFirst = fake.holdDetail(GOG);
  const first = c.open(GOG);
  await c.open(STEAM);
  assert.equal(c.getSnapshot().detail?.game_id, STEAM);
  releaseFirst();
  await first;
  assert.equal(c.getSnapshot().openId, STEAM);
  assert.equal(c.getSnapshot().detail?.game_id, STEAM, "the answer for the game that was left is dropped");
});

test("a late failure for a game that was left is not shown either", async () => {
  const { fake, c } = await loaded();
  fake.failNext("detail", "slow and broken");
  const releaseFirst = fake.holdDetail(GOG);
  const first = c.open(GOG);
  await c.open(STEAM);
  releaseFirst();
  await first;
  assert.equal(c.getSnapshot().detailError, null);
  assert.equal(c.getSnapshot().detail?.game_id, STEAM);
});

test("opening the open game again closes it, and closing forgets its details", async () => {
  const { c } = await opened();
  await c.open(GOG);
  assert.equal(c.getSnapshot().openId, null);
  assert.equal(c.getSnapshot().detail, null);
  await c.open(GOG);
  c.close();
  assert.equal(c.getSnapshot().openId, null);
  assert.equal(c.getSnapshot().detail, null);
});

test("details that cannot be read are shown as an error and refreshing tries again", async () => {
  const { fake, c } = await loaded();
  fake.failNext("detail", "the library has no game gog-1");
  await c.open(GOG);
  assert.equal(c.getSnapshot().openId, GOG);
  assert.equal(c.getSnapshot().detail, null);
  assert.match(c.getSnapshot().detailError ?? "", /the library has no game/);
  await c.refreshDetail();
  assert.equal(c.getSnapshot().detailError, null);
  assert.equal(c.getSnapshot().detail?.game_id, GOG);
});

test("refreshing picks up what changed behind the page, and is skipped while a change is being saved", async () => {
  const { fake, c } = await opened();
  assert.deepEqual(c.getSnapshot().detail?.launch, { state: "not_running" });
  fake.setDetail(GOG, { launch: { state: "running", pid: 7, build_id: "gog-1-experimental-1", since_unix: 1_700_000_000 } });
  await c.refreshDetail();
  assert.equal(c.getSnapshot().detail?.launch.state, "running");

  const before = fake.calls.filter((x) => x.op === "detail").length;
  const release = fake.hold();
  const saving = c.addCategory("RPG");
  await c.refreshDetail();
  assert.equal(fake.calls.filter((x) => x.op === "detail").length, before, "no second read while a change is in flight");
  release();
  await saving;
});

test("adding a category sends the game's list with the new one last, and shows the detail and the view after it", async () => {
  const { fake, c } = await opened(GOG, { [GOG]: ["RPG"] });
  assert.equal(await c.addCategory("  Co-op "), true);
  assert.deepEqual(fake.edits(), [{ op: "setCategories", args: [GOG, ["RPG", "Co-op"]] }]);
  const s = c.getSnapshot();
  assert.deepEqual(s.detail?.categories, ["RPG", "Co-op"]);
  assert.deepEqual(s.view?.categories.map((x) => x.name), ["Co-op", "RPG"]);
  assert.equal(s.notice, "Added “Co-op”.");
  assert.equal(s.busy, null);
  assert.equal(s.detailError, null);
});

test("a blank name, or one the game already has, is turned away on the page and nothing is sent", async () => {
  const { fake, c } = await opened(GOG, { [GOG]: ["RPG"] });
  assert.equal(await c.addCategory("   "), false);
  assert.equal(c.getSnapshot().detailError, "Type a category name first.");
  assert.equal(await c.addCategory(" rpg "), false);
  assert.equal(c.getSnapshot().detailError, null);
  assert.equal(c.getSnapshot().notice, "Alpha already has “rpg”.");
  assert.deepEqual(fake.edits(), []);
});

test("a name the core refuses is shown in the core's words and the details stay as they were", async () => {
  const { fake, c } = await opened(GOG, { [GOG]: ["RPG"] });
  assert.equal(await c.addCategory("x".repeat(41)), false);
  assert.match(c.getSnapshot().detailError ?? "", /a category must be 1 to 40 characters/);
  assert.deepEqual(c.getSnapshot().detail?.categories, ["RPG"]);
  assert.equal(c.getSnapshot().busy, null);
  assert.equal(fake.edits().length, 1, "it was sent, and refused by the core");
});

test("removing a category sends the list without it, ignoring ASCII case, and removing the last sends an empty list", async () => {
  const { fake, c } = await opened(GOG, { [GOG]: ["RPG", "Co-op"] });
  assert.equal(await c.removeCategory("rpg"), true);
  assert.deepEqual(fake.edits().at(-1), { op: "setCategories", args: [GOG, ["Co-op"]] });
  assert.equal(c.getSnapshot().notice, "Removed “rpg”.");
  assert.equal(await c.removeCategory("Co-op"), true);
  assert.deepEqual(fake.edits().at(-1), { op: "setCategories", args: [GOG, []] });
  assert.deepEqual(c.getSnapshot().detail?.categories, []);
  assert.deepEqual(c.getSnapshot().view?.categories, []);
});

test("without an open game there is nothing to change", async () => {
  const { fake, c } = await loaded({ [GOG]: ["RPG"] });
  assert.equal(await c.removeCategory("RPG"), false);
  assert.equal(await c.addCategory("Co-op"), false);
  assert.equal(await c.setArtwork("C:\\a.png"), false);
  assert.equal(await c.clearArtwork(), false);
  assert.deepEqual(fake.edits(), []);
});

test("an artwork path is trimmed and sent, and the reference shown comes back from the core", async () => {
  const { fake, c } = await opened();
  assert.equal(await c.setArtwork("  C:\\Art\\cover.png "), true);
  assert.deepEqual(fake.edits(), [{ op: "setArtwork", args: [GOG, "C:\\Art\\cover.png"] }]);
  assert.equal(c.getSnapshot().detail?.artwork, "\\\\?\\C:\\Art\\cover.png");
  assert.equal(c.getSnapshot().notice, "Artwork reference saved. The image is not copied.");
});

test("a blank artwork path is turned away on the page and clearing sends null", async () => {
  const { fake, c } = await opened();
  assert.equal(await c.setArtwork("   "), false);
  assert.equal(c.getSnapshot().detailError, "Type the path of an image file first.");
  assert.deepEqual(fake.edits(), []);
  await c.setArtwork("a.png");
  assert.equal(await c.clearArtwork(), true);
  assert.deepEqual(fake.edits().at(-1), { op: "setArtwork", args: [GOG, null] });
  assert.equal(c.getSnapshot().detail?.artwork, null);
  assert.equal(c.getSnapshot().notice, "Artwork reference cleared.");
});

test("a file the core will not use as artwork is shown in the core's words and the reference stays", async () => {
  const { c } = await opened();
  await c.setArtwork("good.png");
  assert.equal(await c.setArtwork("notes.txt"), false);
  assert.match(c.getSnapshot().detailError ?? "", /artwork must be a \.png, \.jpg, \.jpeg or \.webp file: notes\.txt/);
  assert.equal(c.getSnapshot().detail?.artwork, "good.png");
});

test("only one change is made at a time", async () => {
  const { fake, c } = await opened(GOG, { [GOG]: ["RPG"] });
  const release = fake.hold();
  const first = c.addCategory("Co-op");
  assert.equal(c.getSnapshot().busy, "categories");
  assert.equal(await c.removeCategory("RPG"), false);
  assert.equal(await c.setArtwork("a.png"), false);
  assert.equal(fake.edits().length, 1);
  release();
  assert.equal(await first, true);
  assert.equal(c.getSnapshot().busy, null);
});

test("an error is cleared by the next change, and a notice by the next thing done", async () => {
  const { fake, c } = await opened();
  fake.failNext("setCategories", "could not save");
  assert.equal(await c.addCategory("RPG"), false);
  assert.equal(c.getSnapshot().detailError, "could not save");
  assert.equal(await c.addCategory("RPG"), true);
  assert.equal(c.getSnapshot().detailError, null);
  assert.equal(c.getSnapshot().notice, "Added “RPG”.");
  await c.refreshDetail();
  assert.equal(c.getSnapshot().notice, null);
});

test("a change that finishes after the details were closed updates the lists but does not bring the details back", async () => {
  const { fake, c } = await opened();
  const release = fake.hold();
  const saving = c.addCategory("RPG");
  c.close();
  release();
  assert.equal(await saving, true);
  assert.equal(c.getSnapshot().openId, null);
  assert.equal(c.getSnapshot().detail, null);
  assert.deepEqual(c.getSnapshot().view?.categories.map((x) => x.name), ["RPG"], "the chips still learn what was saved");
});

test("listeners hear about every change and stop hearing when they unsubscribe", async () => {
  const { c } = await loaded();
  let heard = 0;
  const off = c.subscribe(() => heard++);
  c.select(null);
  assert.equal(heard, 1);
  off();
  c.select(null);
  assert.equal(heard, 1);
});
