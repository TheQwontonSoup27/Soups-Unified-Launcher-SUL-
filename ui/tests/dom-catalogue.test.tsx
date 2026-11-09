import { absent, click, installTauri, mount, one, resetBrowser, settle, submit, type, waitFor, withText } from "./dom";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { beforeEach, describe, test } from "node:test";
import { fileURLToPath } from "node:url";
import { tauriCatalogueApi, type CatalogueView, type GameDetail } from "../src/lib/catalogue";
import type { GameView, LibraryApi } from "../src/lib/library";
import GameLibrary from "../src/views/GameLibrary";
import { aDetail, fakeCatalogue } from "./catalogue-fake";
import { gogGame, steamGame } from "./fixtures";

beforeEach(() => resetBrowser());

const GAMES: GameView[] = [steamGame, gogGame];
const HD = steamGame.name; // "Helldivers 2"
const ALPHA = gogGame.name; // "Alpha"

/** A library backend that only lists. The page under test is the catalogue half of the Game Library page. */
function library(games: GameView[] = GAMES): LibraryApi {
  return {
    list: async () => games,
    scan: async () => ({ games, added: 0, updated: 0, returned: 0, missing: 0 }),
    matrix: async () => {
      throw "not used here";
    },
    tick: async () => {
      throw "not used here";
    },
    play: async () => {},
  };
}

async function page(initial: Record<string, string[]> = {}, query = "") {
  const fake = fakeCatalogue(GAMES, initial);
  const m = await mount(<GameLibrary api={library()} managerApi={null} playApi={null} catalogueApi={fake.api} query={query} />);
  await settle();
  return { fake, ...m };
}

/** The cards in the grid, by name, in the order shown. */
const shown = (host: HTMLElement) => [...(host.querySelector("[aria-label='Your games']")?.querySelectorAll("article h3") ?? [])].map((h) => h.textContent);
const card = (host: HTMLElement, name: string) => withText(one(host, "[aria-label='Your games']"), "article h3", name).closest("article") as HTMLElement;
const chips = (host: HTMLElement) => one(host, "section[aria-label='Filter by category']");
const chip = (host: HTMLElement, text: string) => withText<HTMLButtonElement>(chips(host), "button", text);
const detailsButton = (host: HTMLElement, name: string) => one<HTMLButtonElement>(card(host, name), `button[aria-label$='details of ${name}'], button[aria-label='Details of ${name}']`);
const panel = (host: HTMLElement, name: string) => one(host, `section[aria-label='Details of ${name}']`);
const alertText = (host: HTMLElement) => [...host.querySelectorAll("[role=alert]")].map((e) => e.textContent).join(" | ");
const summary = (host: HTMLElement) => one(host, "[aria-label=Scan] p[role=status]").textContent ?? "";
const statusOf = (host: HTMLElement, name: string) => one(panel(host, name), "[role=status]").textContent ?? "";

async function openDetails(host: HTMLElement, name: string) {
  await click(detailsButton(host, name));
  await waitFor(() => !/Loading details/.test(panel(host, name).textContent ?? ""), `details of ${name}`);
}

async function addCategory(host: HTMLElement, name: string, text: string) {
  const group = one(panel(host, name), "div[aria-label='Categories of this game']");
  await type(one<HTMLInputElement>(group, "input[type=text]"), text);
  await submit(one<HTMLFormElement>(group, "form"));
}

describe("categories on the Game Library page, clicked through", () => {
  test("with no backend there are no category buttons and no Details button, and the games are still listed", async () => {
    const { host, unmount } = await mount(<GameLibrary api={library()} managerApi={null} playApi={null} catalogueApi={null} />);
    await settle();
    assert.deepEqual(shown(host), [HD, ALPHA]);
    absent(host.querySelector("section[aria-label='Filter by category']"));
    absent(host.querySelector("button[aria-label^='Details of']"));
    unmount();
  });

  test("a backend handed to the page without a catalogue backend does not reach for the real one", async () => {
    // `api` is a test backend and `catalogueApi` is left out: the catalogue gets none, so nothing is asked of the app.
    const tauri = installTauri(() => {
      throw "the real backend must not be used";
    });
    const { host, unmount } = await mount(<GameLibrary api={library()} managerApi={null} playApi={null} />);
    await settle();
    assert.deepEqual(tauri.calls, []);
    absent(host.querySelector("button[aria-label^='Details of']"));
    unmount();
    tauri.remove();
  });

  test("while no game has a category there is no filter row, but every card offers Details", async () => {
    const { host, unmount } = await page();
    absent(host.querySelector("section[aria-label='Filter by category']"));
    assert.equal(detailsButton(host, HD).textContent, "Details");
    assert.equal(detailsButton(host, ALPHA).textContent, "Details");
    unmount();
  });

  test("each category in use is a button with its count, plus All games; the cards show their own categories", async () => {
    const { host, unmount } = await page({ [gogGame.id]: ["Co-op", "RPG"], [steamGame.id]: ["Co-op"] });
    const labels = [...chips(host).querySelectorAll("button")].map((b) => b.textContent);
    assert.deepEqual(labels, ["All games (2)", "Co-op (2)", "RPG (1)"]);
    assert.equal(chip(host, "All games (2)").getAttribute("aria-pressed"), "true");
    assert.equal(chip(host, "RPG (1)").getAttribute("aria-pressed"), "false");
    assert.deepEqual(
      [...card(host, ALPHA).querySelectorAll(".tag[title='A category you gave this game']")].map((t) => t.textContent),
      ["Co-op", "RPG"],
    );
    assert.deepEqual(
      [...card(host, HD).querySelectorAll(".tag[title='A category you gave this game']")].map((t) => t.textContent),
      ["Co-op"],
    );
    unmount();
  });

  test("pressing a category shows only the games that carry it and says so; All games brings every game back", async () => {
    const { host, unmount } = await page({ [gogGame.id]: ["RPG"], [steamGame.id]: ["Co-op"] });
    await click(chip(host, "RPG (1)"));
    assert.deepEqual(shown(host), [ALPHA]);
    assert.equal(chip(host, "RPG (1)").getAttribute("aria-pressed"), "true");
    assert.equal(chip(host, "All games (2)").getAttribute("aria-pressed"), "false");
    assert.equal(summary(host), "Showing 1 of 2 games in “RPG”.");

    await click(chip(host, "All games (2)"));
    assert.deepEqual(shown(host), [HD, ALPHA]);
    assert.notEqual(summary(host), "Showing 1 of 2 games in “RPG”.");
    unmount();
  });

  test("the category and the sidebar search combine: a game must match both", async () => {
    // Both games are in "Co-op"; only Alpha matches the search "alpha".
    const both = await page({ [gogGame.id]: ["Co-op"], [steamGame.id]: ["Co-op"] }, "alpha");
    assert.deepEqual(shown(both.host), [ALPHA]);
    assert.equal(summary(both.host), "Showing 1 of 2 games matching “alpha”.");
    await click(chip(both.host, "Co-op (2)"));
    assert.deepEqual(shown(both.host), [ALPHA]);
    assert.equal(summary(both.host), "Showing 1 of 2 games matching “alpha” in “Co-op”.");
    both.unmount();

    // Alpha is not in "RPG"; the search matches Alpha only, so nothing is left and the sentence names both.
    const none = await page({ [steamGame.id]: ["RPG"], [gogGame.id]: ["Co-op"] }, "alpha");
    await click(chip(none.host, "RPG (1)"));
    assert.deepEqual(shown(none.host), []);
    assert.equal(summary(none.host), "No games match “alpha” in “RPG”.");
    none.unmount();
  });

  test("the category lists are asked for again when the library's set of games changes (after a scan), and not otherwise", async () => {
    let listed: GameView[] = [];
    const lib: LibraryApi = {
      ...library(),
      list: async () => listed,
      scan: async () => {
        listed = GAMES;
        return { games: GAMES, added: 2, updated: 0, returned: 0, missing: 0 };
      },
    };
    const fake = fakeCatalogue(GAMES, { [gogGame.id]: ["RPG"] });
    const { host, unmount } = await mount(<GameLibrary api={lib} managerApi={null} playApi={null} catalogueApi={fake.api} />);
    await settle();
    const asked = () => fake.calls.filter((c) => c.op === "view").length;
    const before = asked();
    absent(host.querySelector("[aria-label='Your games']"), "no games yet");

    await click(withText(host, "button", "Scan for games"));
    await waitFor(() => asked() === before + 1, "one new request for the categories");
    assert.deepEqual(shown(host), [HD, ALPHA]);
    assert.deepEqual([...chips(host).querySelectorAll("button")].map((b) => b.textContent), ["All games (2)", "RPG (1)"]);

    // Scanning again finds the same games: the set did not change, so nothing is asked again.
    await click(withText(host, "button", "Scan for games"));
    await settle();
    assert.equal(asked(), before + 1);
    unmount();
  });

  test("the search-only wording stays as it was when no category is chosen", async () => {
    const { host, unmount } = await page({ [gogGame.id]: ["RPG"] }, "zzz");
    assert.equal(summary(host), "No games match “zzz”.");
    unmount();
  });
});

describe("a game's details, opened from its card", () => {
  test("Details opens inside that game's card only, says it is loading, then shows what SUL knows", async () => {
    const { fake, host, unmount } = await page();
    const release = fake.holdDetail(gogGame.id);
    await click(detailsButton(host, ALPHA));
    assert.match(statusOf(host, ALPHA), /Loading details/);
    assert.equal(detailsButton(host, ALPHA).getAttribute("aria-expanded"), "true");
    assert.equal(detailsButton(host, ALPHA).textContent, "Hide details");
    assert.ok(card(host, ALPHA).contains(panel(host, ALPHA)), "the details are inside the card");
    absent(host.querySelector(`section[aria-label='Details of ${HD}']`));
    release();
    await waitFor(() => !/Loading details/.test(panel(host, ALPHA).textContent ?? ""), "the details");

    const text = panel(host, ALPHA).textContent ?? "";
    assert.match(text, /Adapter:\s*none registered/);
    assert.match(text, /Now:\s*Not running/);
    assert.match(text, /No adapter is registered for this game/);
    assert.match(text, /SUL has no build of this game yet/);
    assert.equal(one(panel(host, ALPHA), ".badge").textContent, "Not investigated");
    unmount();
  });

  test("builds, the adapter, the capability label and what is running come from the core's detail, in words", async () => {
    const { fake, host, unmount } = await page();
    const detail: Partial<GameDetail> = {
      builds: [
        { build_id: "gog-1-stable-1", slot: "stable", state: "ready", managed: false, active: true },
        { build_id: "gog-1-experimental-1", slot: "experimental", state: "failed", managed: true, active: false },
      ],
      adapter: { id: "made-up", version: "0.1" },
      capability: "experimental",
      capability_label: "Experimental",
      launch: { state: "running", pid: 4242, build_id: "gog-1-stable-1", since_unix: 1_700_000_000 },
      notes: ["Only made-up programs were tested."],
    };
    fake.setDetail(gogGame.id, detail);
    await openDetails(host, ALPHA);
    const p = panel(host, ALPHA);
    assert.equal(one(p, ".badge").textContent, "Experimental");
    assert.match(p.textContent ?? "", /Adapter:\s*made-up 0\.1/);
    assert.match(p.textContent ?? "", /Now:\s*Running \(process 4242\)/);
    const builds = [...one(p, "div[aria-label='Builds of this game']").querySelectorAll("li")].map((li) => li.textContent ?? "");
    assert.equal(builds.length, 2);
    assert.match(builds[0]!, /gog-1-stable-1.*stable.*install stays in place.*active/);
    assert.match(builds[1]!, /gog-1-experimental-1.*experimental.*managed copy.*Does not match its manifest/);
    assert.doesNotMatch(builds[1]!, /active/);
    assert.match(one(p, "ul[aria-label=Notes]").textContent ?? "", /Only made-up programs were tested\./);
    unmount();
  });

  test("pressing Details again, or Close, hides the details; opening a second game replaces the first", async () => {
    const { host, unmount } = await page();
    await openDetails(host, ALPHA);
    await click(detailsButton(host, ALPHA));
    absent(host.querySelector(`section[aria-label='Details of ${ALPHA}']`));
    assert.equal(detailsButton(host, ALPHA).getAttribute("aria-expanded"), "false");

    await openDetails(host, ALPHA);
    await openDetails(host, HD);
    absent(host.querySelector(`section[aria-label='Details of ${ALPHA}']`));
    assert.ok(panel(host, HD));

    await click(one(panel(host, HD), "button[aria-label='Close details']"));
    absent(host.querySelector("section[aria-label^='Details of']"));
    unmount();
  });

  test("details that cannot be read are shown as an error, and Refresh tries again", async () => {
    const { fake, host, unmount } = await page();
    fake.failNext("detail", "the catalogue file could not be read");
    await click(detailsButton(host, ALPHA));
    await waitFor(() => /could not be read/.test(alertText(host)), "the error");
    absent(panel(host, ALPHA).querySelector(".badge"));
    assert.doesNotMatch(statusOf(host, ALPHA), /Loading details/);

    await click(one(panel(host, ALPHA), "button[aria-label='Refresh details']"));
    await waitFor(() => panel(host, ALPHA).querySelector(".badge") !== null, "the details after refreshing");
    assert.equal(alertText(host), "");
    unmount();
  });

  test("Refresh picks up what changed behind the page (a build became active, a game started)", async () => {
    const { fake, host, unmount } = await page();
    await openDetails(host, ALPHA);
    assert.match(panel(host, ALPHA).textContent ?? "", /Now:\s*Not running/);
    fake.setDetail(gogGame.id, { launch: { state: "running", pid: 99, build_id: "gog-1-stable-1", since_unix: 1_700_000_000 } });
    await click(one(panel(host, ALPHA), "button[aria-label='Refresh details']"));
    await waitFor(() => /Running \(process 99\)/.test(panel(host, ALPHA).textContent ?? ""), "the new state");
    unmount();
  });
});

describe("a game's categories, edited on the page", () => {
  test("adding a category sends the game's whole list, clears the field, shows it on the card and in the filter row", async () => {
    const { fake, host, unmount } = await page({ [gogGame.id]: ["RPG"] });
    await openDetails(host, ALPHA);
    await addCategory(host, ALPHA, "  Co-op ");
    await waitFor(() => /Added “Co-op”\./.test(statusOf(host, ALPHA)), "the notice");
    assert.deepEqual(fake.edits(), [{ op: "setCategories", args: [gogGame.id, ["RPG", "Co-op"]] }]);
    const input = one<HTMLInputElement>(panel(host, ALPHA), "div[aria-label='Categories of this game'] input[type=text]");
    assert.equal(input.value, "", "the field is emptied after a save");
    assert.deepEqual(
      [...card(host, ALPHA).querySelectorAll(".tag[title='A category you gave this game']")].map((t) => t.textContent),
      ["RPG", "Co-op"],
    );
    assert.deepEqual([...chips(host).querySelectorAll("button")].map((b) => b.textContent), ["All games (2)", "Co-op (1)", "RPG (1)"]);
    unmount();
  });

  test("a blank name is turned away on the page and nothing is sent", async () => {
    const { fake, host, unmount } = await page();
    await openDetails(host, ALPHA);
    await addCategory(host, ALPHA, "   ");
    assert.match(alertText(host), /Type a category name first\./);
    assert.deepEqual(fake.edits(), []);
    unmount();
  });

  test("a name the game already has (any ASCII case) is turned away with a notice and nothing is sent", async () => {
    const { fake, host, unmount } = await page({ [gogGame.id]: ["RPG"] });
    await openDetails(host, ALPHA);
    await addCategory(host, ALPHA, "rpg");
    assert.match(statusOf(host, ALPHA), /Alpha already has “rpg”\./);
    assert.deepEqual(fake.edits(), []);
    unmount();
  });

  test("a name the core refuses is shown in the core's words, the typed text stays, and the game's list is unchanged", async () => {
    const { fake, host, unmount } = await page({ [gogGame.id]: ["RPG"] });
    await openDetails(host, ALPHA);
    await addCategory(host, ALPHA, "x".repeat(41));
    await waitFor(() => /1 to 40 characters/.test(alertText(host)), "the refusal");
    const input = one<HTMLInputElement>(panel(host, ALPHA), "div[aria-label='Categories of this game'] input[type=text]");
    assert.equal(input.value, "x".repeat(41), "what was typed is kept so it can be fixed");
    assert.deepEqual(fake.view().entries, [{ game_id: gogGame.id, categories: ["RPG"] }]);
    unmount();
  });

  test("Remove sends the list without that category; removing the last one empties the filter row", async () => {
    const { fake, host, unmount } = await page({ [gogGame.id]: ["RPG"] });
    await openDetails(host, ALPHA);
    await click(one(panel(host, ALPHA), "button[aria-label='Remove category RPG']"));
    await waitFor(() => /Removed “RPG”\./.test(statusOf(host, ALPHA)), "the notice");
    assert.deepEqual(fake.edits(), [{ op: "setCategories", args: [gogGame.id, []] }]);
    absent(host.querySelector("section[aria-label='Filter by category']"));
    assert.match(one(panel(host, ALPHA), "div[aria-label='Categories of this game']").textContent ?? "", /None\. Your own labels, saved on this computer; they change no game file\./);
    unmount();
  });

  test("removing the category that is chosen as the filter drops the filter, so the grid does not go empty", async () => {
    const { host, unmount } = await page({ [gogGame.id]: ["RPG"] });
    await click(chip(host, "RPG (1)"));
    assert.deepEqual(shown(host), [ALPHA]);
    await openDetails(host, ALPHA);
    await click(one(panel(host, ALPHA), "button[aria-label='Remove category RPG']"));
    await waitFor(() => /Removed “RPG”\./.test(statusOf(host, ALPHA)), "the notice");
    assert.deepEqual(shown(host), [HD, ALPHA]);
    unmount();
  });

  test("while a change is being saved the buttons are held and the page says so", async () => {
    const { fake, host, unmount } = await page({ [gogGame.id]: ["RPG"] });
    await openDetails(host, ALPHA);
    const release = fake.hold();
    await click(one(panel(host, ALPHA), "button[aria-label='Remove category RPG']"));
    assert.equal(statusOf(host, ALPHA), "Saving…");
    for (const label of ["Add category", "Use this image", "Refresh details", "Remove category RPG"]) {
      assert.equal(one<HTMLButtonElement>(panel(host, ALPHA), `button[aria-label='${label}']`).disabled, true, label);
    }
    release();
    await waitFor(() => statusOf(host, ALPHA) !== "Saving…", "the save to finish");
    unmount();
  });
});

describe("a game's artwork reference", () => {
  test("it says what is and is not done: only the place of the file is kept, nothing is copied, and no image is shown", async () => {
    const { host, unmount } = await page();
    await openDetails(host, ALPHA);
    const art = one(panel(host, ALPHA), "div[aria-label='Artwork of this game']");
    assert.match(art.textContent ?? "", /No image chosen\./);
    assert.match(art.textContent ?? "", /SUL only remembers where the image is/);
    assert.match(art.textContent ?? "", /it does not copy it/);
    assert.match(art.textContent ?? "", /cannot be shown here yet/);
    absent(panel(host, ALPHA).querySelector("img"), "no image element: the app has no permission to show a local file");
    absent(art.querySelector("button[aria-label='Clear artwork']"), "nothing to clear yet");
    unmount();
  });

  test("a path is sent trimmed, shown the way it was typed (without the \\\\?\\ prefix), and can be cleared", async () => {
    const { fake, host, unmount } = await page();
    await openDetails(host, ALPHA);
    const art = () => one(panel(host, ALPHA), "div[aria-label='Artwork of this game']");
    await type(one<HTMLInputElement>(art(), "input[type=text]"), "  C:\\Pictures\\alpha.png ");
    await submit(one<HTMLFormElement>(art(), "form"));
    await waitFor(() => /Artwork reference saved\. The image is not copied\./.test(statusOf(host, ALPHA)), "the notice");
    assert.deepEqual(fake.edits(), [{ op: "setArtwork", args: [gogGame.id, "C:\\Pictures\\alpha.png"] }]);
    assert.equal(one(art(), "code").textContent, "C:\\Pictures\\alpha.png");
    assert.equal(one<HTMLInputElement>(art(), "input[type=text]").value, "");
    absent(panel(host, ALPHA).querySelector("img"));

    await click(one(art(), "button[aria-label='Clear artwork']"));
    await waitFor(() => /Artwork reference cleared\./.test(statusOf(host, ALPHA)), "the notice");
    assert.deepEqual(fake.edits()[1], { op: "setArtwork", args: [gogGame.id, null] });
    assert.match(art().textContent ?? "", /No image chosen\./);
    unmount();
  });

  test("a blank path is turned away on the page; a file the core will not use is shown in the core's words and the typed path stays", async () => {
    const { fake, host, unmount } = await page();
    await openDetails(host, ALPHA);
    const art = () => one(panel(host, ALPHA), "div[aria-label='Artwork of this game']");
    await submit(one<HTMLFormElement>(art(), "form"));
    assert.match(alertText(host), /Type the path of an image file first\./);
    assert.deepEqual(fake.edits(), []);

    await type(one<HTMLInputElement>(art(), "input[type=text]"), "C:\\Games\\alpha.exe");
    await submit(one<HTMLFormElement>(art(), "form"));
    await waitFor(() => /must be a \.png, \.jpg, \.jpeg or \.webp file/.test(alertText(host)), "the refusal");
    assert.equal(one<HTMLInputElement>(art(), "input[type=text]").value, "C:\\Games\\alpha.exe");
    assert.match(art().textContent ?? "", /No image chosen\./);
    unmount();
  });
});

describe("the real wiring", () => {
  const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..", "..");
  const rust = (rel: string) => readFileSync(join(ROOT, rel), "utf8");
  const camel = (snake: string) => snake.replace(/_([a-z])/g, (_, c: string) => c.toUpperCase());
  /** Top-level parameter names of a Rust command, camelCased, without `State`. */
  function expectedArgs(command: string): string[] {
    const m = new RegExp(`fn\\s+${command}\\s*\\(([\\s\\S]*?)\\)\\s*->`).exec(rust("src-tauri/src/commands.rs"));
    assert.ok(m, `no fn ${command} in src-tauri/src/commands.rs`);
    return m[1]!
      .split(/,(?![^<(]*[>)])/)
      .map((p) => p.trim())
      .filter((p) => p !== "" && !/State\s*</.test(p))
      .map((p) => camel(p.split(":")[0]!.trim()))
      .sort();
  }
  const registered = () => /generate_handler!\[([\s\S]*?)\]/.exec(rust("src-tauri/src/lib.rs"))![1]!.split(",").map((s) => s.trim().replace(/^commands::/, ""));

  const view: CatalogueView = { categories: [], entries: [] };
  const detail = aDetail(gogGame);
  const answer = (cmd: string): unknown => (cmd === "catalogue_view" ? view : cmd === "game_detail" ? detail : { detail, view });

  test("every command name and argument is the one the Rust commands take (camelCase top-level arguments)", async () => {
    const tauri = installTauri(answer);
    const api = tauriCatalogueApi();
    assert.ok(api, "inside the webview there is a backend");
    await api.view();
    await api.detail(gogGame.id);
    await api.setCategories(gogGame.id, ["RPG"]);
    await api.setArtwork(gogGame.id, "C:\\a.png");
    await api.setArtwork(gogGame.id, null);
    assert.deepEqual(tauri.calls, [
      { cmd: "catalogue_view", args: {} },
      { cmd: "game_detail", args: { gameId: gogGame.id } },
      { cmd: "catalogue_set_categories", args: { gameId: gogGame.id, categories: ["RPG"] } },
      { cmd: "catalogue_set_artwork", args: { gameId: gogGame.id, path: "C:\\a.png" } },
      { cmd: "catalogue_set_artwork", args: { gameId: gogGame.id, path: null } },
    ]);
    tauri.remove();
  });

  test("each of those commands is registered in src-tauri and takes exactly the arguments the page sends", async () => {
    const tauri = installTauri(answer);
    const api = tauriCatalogueApi()!;
    await api.view();
    await api.detail(gogGame.id);
    await api.setCategories(gogGame.id, []);
    await api.setArtwork(gogGame.id, null);
    const names = registered();
    for (const c of tauri.calls) {
      assert.ok(names.includes(c.cmd), `${c.cmd} is sent by the page but not registered`);
      assert.deepEqual(Object.keys(c.args).sort(), expectedArgs(c.cmd), c.cmd);
    }
    assert.equal(tauri.calls.length, 4);
    assert.deepEqual(expectedArgs("catalogue_set_categories"), ["categories", "gameId"], "the parser really reads the Rust signature");
    tauri.remove();
  });

  test("in a plain browser there is no backend", () => {
    assert.equal(tauriCatalogueApi(), null);
  });

  test("data in the wrong shape is refused with a plain message instead of reaching the page", async () => {
    for (const junk of [[], null, "text", { categories: [] }, { categories: [{ name: 1 }], entries: [] }, { detail: {}, view }]) {
      const tauri = installTauri(() => junk);
      const api = tauriCatalogueApi()!;
      await assert.rejects(api.view(), /does not understand/);
      await assert.rejects(api.detail(gogGame.id), /does not understand/);
      await assert.rejects(api.setCategories(gogGame.id, []), /does not understand/);
      await assert.rejects(api.setArtwork(gogGame.id, null), /does not understand/);
      tauri.remove();
    }
  });

  test("a failing command's plain-text error reaches the page", async () => {
    const tauri = installTauri((cmd) => {
      if (cmd === "catalogue_view") throw "the catalogue file could not be read; it was left alone";
      return [];
    });
    const { host, unmount } = await mount(<GameLibrary api={library()} managerApi={null} playApi={null} catalogueApi={tauriCatalogueApi()} />);
    await settle();
    await waitFor(() => /catalogue file could not be read/.test(alertText(host)), "the error");
    assert.deepEqual(shown(host), [HD, ALPHA], "the games are still listed");
    unmount();
    tauri.remove();
  });
});
