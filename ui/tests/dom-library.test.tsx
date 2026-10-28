import { click, installTauri, mount, one, resetBrowser, settle, withText } from "./dom";
import assert from "node:assert/strict";
import { afterEach, beforeEach, describe, test } from "node:test";
import { tauriLibraryApi, type FeatureId, type GameView, type LibraryApi, type MatrixView, type ScanView } from "../src/lib/library";
import GameLibrary from "../src/views/GameLibrary";
import { gogGame, matrixFor, steamGame } from "./fixtures";

beforeEach(() => resetBrowser());

/** A backend in memory that follows the core's rules: Steam headless launch is for Steam games only. */
function fakeBackend(saved: GameView[] = []) {
  const calls: string[] = [];
  let games = saved;
  const ticked: Record<string, FeatureId[]> = {};
  const matrix = (id: string): MatrixView => matrixFor(games.find((g) => g.id === id)!, ticked[id] ?? []);
  const api: LibraryApi = {
    async list() {
      calls.push("list");
      return games;
    },
    async scan(): Promise<ScanView> {
      calls.push("scan");
      games = [steamGame, gogGame];
      return { games, added: 2, updated: 0, returned: 0, missing: 0 };
    },
    async matrix(id) {
      calls.push(`matrix ${id}`);
      return matrix(id);
    },
    async tick(id, feature, enabled) {
      calls.push(`tick ${id} ${feature} ${enabled}`);
      if (feature === "steam_headless" && enabled && games.find((g) => g.id === id)?.source !== "steam") {
        throw "Steam headless launch is for Steam games only.";
      }
      ticked[id] = enabled ? [...(ticked[id] ?? []), feature] : (ticked[id] ?? []).filter((f) => f !== feature);
      return matrix(id);
    },
    async play(appId) {
      calls.push(`play ${appId}`);
    },
  };
  return { api, calls };
}

/** A game's own card in the grid (the "Planned game adapters" cards below it are not these). */
const card = (host: HTMLElement, name: string) =>
  withText(one(host, "[aria-label='Your games']"), "article h3", name).closest("article") as HTMLElement;

describe("the Game Library page, clicked through", () => {
  test("saved games are listed at start without scanning", async () => {
    const { api, calls } = fakeBackend([steamGame]);
    const { host, unmount } = await mount(<GameLibrary api={api} />);
    await settle();
    assert.deepEqual(calls, ["list"]);
    assert.equal(host.querySelectorAll(".game-card").length, 1);
    assert.match(one(host, "[aria-label=Scan] p[role=status]").textContent ?? "", /Scans only read/);
    unmount();
  });

  test("an empty library invites a scan; Scan finds the games and says how many", async () => {
    const { api, calls } = fakeBackend();
    const { host, unmount } = await mount(<GameLibrary api={api} />);
    await settle();
    assert.match(one(host, "[aria-label=Scan] p[role=status]").textContent ?? "", /No games yet/);
    assert.equal(host.querySelector("[aria-label='Your games']"), null);

    await click(withText(host, "button", "Scan for games"));
    assert.deepEqual(calls, ["list", "scan"]);
    assert.equal(host.querySelectorAll(".game-card").length, 2);
    assert.equal(one(host, "[aria-label=Scan] p[role=status]").textContent, "Found 2 games (2 new).");
    unmount();
  });

  test("Compatibility opens a game's matrix inside its own card; ticking shows only what the backend saved", async () => {
    const { api, calls } = fakeBackend([steamGame, gogGame]);
    const { host, unmount } = await mount(<GameLibrary api={api} />);
    await settle();

    await click(withText(card(host, "Helldivers 2"), "button", "Compatibility"));
    assert.ok(card(host, "Helldivers 2").querySelector(".matrix"));
    assert.equal(card(host, "Alpha").querySelector(".matrix"), null, "the other card stays closed");
    assert.equal(card(host, "Helldivers 2").querySelectorAll("input[type=checkbox]").length, 8);
    assert.ok(calls.includes("matrix steam-553850"));

    const mods = withText(card(host, "Helldivers 2"), "label span", "Mods").parentElement!.querySelector("input")!;
    assert.equal(mods.checked, false);
    await click(mods);
    assert.ok(calls.includes("tick steam-553850 mods true"));
    assert.equal(withText(card(host, "Helldivers 2"), "label span", "Mods").parentElement!.querySelector("input")!.checked, true);

    await click(withText(card(host, "Helldivers 2"), "button", "Hide compatibility"));
    assert.equal(card(host, "Helldivers 2").querySelector(".matrix"), null);
    unmount();
  });

  test("a feature that cannot apply is locked, and its card says why", async () => {
    const { api } = fakeBackend([steamGame, gogGame]);
    const { host, unmount } = await mount(<GameLibrary api={api} />);
    await settle();
    await click(withText(card(host, "Alpha"), "button", "Compatibility"));
    const headless = withText(card(host, "Alpha"), "label span", "Steam headless launch").parentElement!.querySelector("input")!;
    assert.equal(headless.disabled, true, "the checkbox is locked for a non-Steam game");
    assert.match(card(host, "Alpha").textContent ?? "", /Steam titles only/);
    assert.equal(card(host, "Helldivers 2").querySelector("[role=alert]"), null);
    unmount();
  });

  test("a tick the backend refuses bounces back, and the reason is shown on that card only", async () => {
    const api: LibraryApi = { ...fakeBackend([steamGame, gogGame]).api, tick: async () => Promise.reject("Mods cannot be enabled yet.") };
    const { host, unmount } = await mount(<GameLibrary api={api} />);
    await settle();
    await click(withText(card(host, "Helldivers 2"), "button", "Compatibility"));
    const mods = () => withText(card(host, "Helldivers 2"), "label span", "Mods").parentElement!.querySelector("input")!;
    await click(mods());
    assert.equal(mods().checked, false, "the box shows what was saved, not what was clicked");
    assert.equal(one(card(host, "Helldivers 2"), "[role=alert]").textContent, "Mods cannot be enabled yet.");
    assert.equal(card(host, "Alpha").querySelector("[role=alert]"), null);
    unmount();
  });

  test("Play on a Steam game asks Steam with its app id and confirms on that card", async () => {
    const { api, calls } = fakeBackend([steamGame, gogGame]);
    const { host, unmount } = await mount(<GameLibrary api={api} />);
    await settle();
    await click(withText(card(host, "Helldivers 2"), "button", "Play"));
    assert.ok(calls.includes("play 553850"));
    assert.equal(one(card(host, "Helldivers 2"), "[role=status]").textContent, "Asked Steam to start Helldivers 2.");
    assert.equal(card(host, "Alpha").querySelector(".game-notice"), null);
    unmount();
  });

  test("Play is disabled for a game that is not from Steam, and the card says why", async () => {
    const { api, calls } = fakeBackend([gogGame]);
    const { host, unmount } = await mount(<GameLibrary api={api} />);
    await settle();
    const play = withText<HTMLButtonElement>(card(host, "Alpha"), "button", "Play");
    assert.equal(play.disabled, true);
    assert.match(card(host, "Alpha").textContent ?? "", /Only Steam games can be started from here for now/);
    await click(play);
    assert.ok(!calls.some((c) => c.startsWith("play")));
    unmount();
  });

  test("a library that cannot be read is reported as an alert, not a blank page", async () => {
    const api: LibraryApi = { ...fakeBackend().api, list: async () => Promise.reject("the game library could not be opened: denied") };
    const { host, unmount } = await mount(<GameLibrary api={api} />);
    await settle();
    assert.equal(one(host, "p[role=alert]").textContent, "the game library could not be opened: denied");
    unmount();
  });

  test("in a plain browser the scan button is off and the page says scanning needs the desktop app", async () => {
    const { host, unmount } = await mount(<GameLibrary />);
    await settle();
    assert.equal(withText<HTMLButtonElement>(host, "button", "Scan for games").disabled, true);
    assert.match(host.textContent ?? "", /scanning needs the desktop app/);
    unmount();
  });
});

describe("the real bridge to the desktop app", () => {
  let tauri: ReturnType<typeof installTauri> | null = null;
  afterEach(() => {
    tauri?.remove();
    tauri = null;
  });

  test("there is no bridge in a plain browser", () => {
    assert.equal(tauriLibraryApi(), null);
  });

  test("each call sends the command name and camelCase argument names the Rust side expects", async () => {
    tauri = installTauri(() => []);
    const api = tauriLibraryApi()!;
    await api.list();
    await api.scan();
    await api.matrix("steam-553850");
    await api.tick("steam-553850", "steam_headless", true);
    await api.play(553850);
    assert.deepEqual(tauri.calls, [
      { cmd: "list_games", args: {} },
      { cmd: "scan_games_command", args: {} },
      { cmd: "get_game_matrix", args: { gameId: "steam-553850" } },
      { cmd: "update_feature_tick", args: { gameId: "steam-553850", feature: "steam_headless", enabled: true } },
      { cmd: "launch_game_command", args: { appId: 553850 } },
    ]);
  });

  test("an error string from a Rust command reaches the page unchanged", async () => {
    tauri = installTauri((cmd) => {
      if (cmd === "launch_game_command") throw "a failure from Rust";
      return [steamGame];
    });
    const { host, unmount } = await mount(<GameLibrary />);
    await settle();
    await click(withText(card(host, "Helldivers 2"), "button", "Play"));
    assert.equal(one(card(host, "Helldivers 2"), "[role=alert]").textContent, "a failure from Rust");
    unmount();
  });
});
