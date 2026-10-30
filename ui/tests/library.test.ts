import { test } from "node:test";
import assert from "node:assert/strict";
import {
  COMPAT_LABEL,
  LibraryController,
  describeScan,
  filterGames,
  messageOf,
  pendingKey,
  playBlockedReason,
  type FeatureId,
  type GameView,
  type LibraryApi,
  type MatrixView,
  type ScanView,
} from "../src/lib/library";
import { gogGame, matrixFor, steamGame } from "./fixtures";

class FakeApi implements LibraryApi {
  games: GameView[] = [steamGame, gogGame];
  ticked: Record<string, FeatureId[]> = {};
  calls: string[] = [];
  failNext: Record<string, string> = {};
  /** Holds a call open until released, to test overlapping operations. */
  gate: Promise<void> | null = null;

  private async enter(name: string): Promise<void> {
    this.calls.push(name);
    if (this.gate) await this.gate;
    const failure = this.failNext[name.split(":")[0]!];
    if (failure !== undefined) {
      delete this.failNext[name.split(":")[0]!];
      throw failure; // `invoke` rejects with a plain string
    }
  }

  async list() {
    await this.enter("list");
    return this.games;
  }
  async scan(): Promise<ScanView> {
    await this.enter("scan");
    return { games: this.games, added: this.games.length, updated: 0, returned: 0, missing: 0 };
  }
  async matrix(id: string) {
    await this.enter(`matrix:${id}`);
    return this.build(id);
  }
  async tick(id: string, feature: FeatureId, enabled: boolean) {
    await this.enter(`tick:${id}:${feature}:${enabled}`);
    const game = this.games.find((g) => g.id === id)!;
    if (feature === "steam_headless" && game.source !== "steam") throw "steam_headless is not available for this game: Steam titles only";
    const now = new Set(this.ticked[id] ?? []);
    if (enabled) now.add(feature);
    else now.delete(feature);
    this.ticked[id] = [...now];
    return this.build(id);
  }
  async play(appId: number) {
    await this.enter(`play:${appId}`);
  }
  private build(id: string): MatrixView {
    return matrixFor(this.games.find((g) => g.id === id)!, this.ticked[id] ?? []);
  }
}

const settle = () => new Promise<void>((resolve) => setImmediate(resolve));

// ---------------------------------------------------------------------------
// Loading and scanning
// ---------------------------------------------------------------------------

test("opening the page shows the saved library without scanning", async () => {
  const api = new FakeApi();
  const lib = new LibraryController(api);
  assert.equal(lib.getSnapshot().loaded, false);
  await lib.load();
  assert.deepEqual(api.calls, ["list"]);
  assert.equal(lib.getSnapshot().loaded, true);
  assert.deepEqual(lib.getSnapshot().games.map((g) => g.id), ["steam-553850", "gog-1"]);
});

test("a failure reading the library is shown, and the page still opens", async () => {
  const api = new FakeApi();
  api.failNext["list"] = "the game library could not be opened: denied";
  const lib = new LibraryController(api);
  await lib.load();
  assert.equal(lib.getSnapshot().loaded, true);
  assert.equal(lib.getSnapshot().error, "the game library could not be opened: denied");
  assert.deepEqual(lib.getSnapshot().games, []);
});

test("without a backend nothing is called and nothing is faked", async () => {
  const lib = new LibraryController(null);
  assert.equal(lib.hasBackend, false);
  await lib.load();
  await lib.scan();
  lib.setTick("steam-553850", "mods", true);
  lib.play(steamGame);
  await settle();
  const s = lib.getSnapshot();
  assert.equal(s.loaded, true);
  assert.deepEqual(s.games, []);
  assert.equal(s.error, null);
  assert.equal(s.scanning, false);
});

test("a scan replaces the list and says what it found", async () => {
  const api = new FakeApi();
  const lib = new LibraryController(api);
  await lib.scan();
  const s = lib.getSnapshot();
  assert.equal(s.scanning, false);
  assert.equal(s.games.length, 2);
  assert.equal(s.scanSummary, "Found 2 games (2 new).");
});

test("pressing Scan twice runs one scan", async () => {
  const api = new FakeApi();
  let release!: () => void;
  api.gate = new Promise<void>((resolve) => (release = resolve));
  const lib = new LibraryController(api);
  const first = lib.scan();
  const second = lib.scan();
  assert.equal(lib.getSnapshot().scanning, true);
  release();
  await Promise.all([first, second]);
  assert.equal(api.calls.filter((c) => c === "scan").length, 1);
  assert.equal(lib.getSnapshot().scanning, false);
});

test("a failed scan keeps the games already shown and reports the reason", async () => {
  const api = new FakeApi();
  const lib = new LibraryController(api);
  await lib.load();
  api.failNext["scan"] = "scanning games is only supported on Windows";
  await lib.scan();
  const s = lib.getSnapshot();
  assert.equal(s.games.length, 2, "the saved list is untouched");
  assert.equal(s.error, "scanning games is only supported on Windows");
  assert.equal(s.scanning, false);
  assert.equal(s.scanSummary, null);
});

test("a scan refreshes the matrices that are open", async () => {
  const api = new FakeApi();
  const lib = new LibraryController(api);
  await lib.load();
  lib.toggleExpanded("steam-553850");
  await settle();
  api.calls.length = 0;
  await lib.scan();
  assert.deepEqual(api.calls, ["scan", "matrix:steam-553850"]);
});

test("scan summaries and Play reasons read plainly", () => {
  const view = (over: Partial<ScanView>): ScanView => ({ games: [steamGame], added: 0, updated: 0, returned: 0, missing: 0, ...over });
  assert.equal(describeScan(view({})), "Found 1 game.");
  assert.equal(describeScan(view({ games: [steamGame, gogGame], added: 1, returned: 1, missing: 3 })), "Found 2 games (1 new, 1 back, 3 no longer found).");
  assert.equal(describeScan(view({ games: [{ ...steamGame, present: false }] })), "Found 0 games.");

  assert.equal(playBlockedReason(steamGame), null);
  assert.match(playBlockedReason(gogGame)!, /Only Steam games/);
  assert.match(playBlockedReason({ ...steamGame, present: false })!, /Not found on the last scan/);
  assert.equal(messageOf("text"), "text");
  assert.equal(messageOf(new Error("boom")), "boom");
  assert.equal(messageOf(42), "Something went wrong.");
});

// ---------------------------------------------------------------------------
// The matrix
// ---------------------------------------------------------------------------

test("opening a card reads its matrix once; closing and reopening reuses it", async () => {
  const api = new FakeApi();
  const lib = new LibraryController(api);
  await lib.load();

  lib.toggleExpanded("steam-553850");
  assert.equal(lib.getSnapshot().expanded["steam-553850"], true);
  await settle();
  assert.equal(lib.getSnapshot().matrices["steam-553850"]?.rows.length, 8);

  lib.toggleExpanded("steam-553850"); // close
  assert.equal(lib.getSnapshot().expanded["steam-553850"], undefined);
  lib.toggleExpanded("steam-553850"); // reopen
  await settle();
  assert.equal(api.calls.filter((c) => c.startsWith("matrix:")).length, 1);
});

test("a failed matrix read is shown on that card only", async () => {
  const api = new FakeApi();
  api.failNext["matrix"] = "unknown game: steam-553850";
  const lib = new LibraryController(api);
  await lib.load();
  lib.toggleExpanded("steam-553850");
  await settle();
  const s = lib.getSnapshot();
  assert.equal(s.gameErrors["steam-553850"], "unknown game: steam-553850");
  assert.equal(s.gameErrors["gog-1"], undefined);
  assert.equal(s.matrices["steam-553850"], undefined);
});

test("ticking a feature shows the matrix the backend saved", async () => {
  const api = new FakeApi();
  const lib = new LibraryController(api);
  await lib.load();
  lib.toggleExpanded("steam-553850");
  await settle();

  lib.setTick("steam-553850", "steam_headless", true);
  assert.equal(lib.getSnapshot().pending[pendingKey("steam-553850", "steam_headless")], true, "locked while in flight");
  await settle();

  const s = lib.getSnapshot();
  assert.equal(s.pending[pendingKey("steam-553850", "steam_headless")], undefined);
  assert.equal(s.matrices["steam-553850"]!.rows.find((r) => r.feature === "steam_headless")!.enabled, true);
  assert.equal(s.gameErrors["steam-553850"], undefined);

  lib.setTick("steam-553850", "steam_headless", false);
  await settle();
  assert.equal(lib.getSnapshot().matrices["steam-553850"]!.rows.find((r) => r.feature === "steam_headless")!.enabled, false);
});

test("a tick the rules refuse leaves the matrix as it was and says why", async () => {
  const api = new FakeApi();
  const lib = new LibraryController(api);
  await lib.load();
  lib.toggleExpanded("gog-1");
  await settle();
  const before = lib.getSnapshot().matrices["gog-1"];

  lib.setTick("gog-1", "steam_headless", true);
  await settle();

  const s = lib.getSnapshot();
  assert.equal(s.matrices["gog-1"], before, "unchanged");
  assert.match(s.gameErrors["gog-1"]!, /Steam titles only/);

  // A later successful tick clears the message.
  lib.setTick("gog-1", "mods", true);
  await settle();
  assert.equal(lib.getSnapshot().gameErrors["gog-1"], undefined);
});

test("a double click on one checkbox sends one request; other features are independent", async () => {
  const api = new FakeApi();
  const lib = new LibraryController(api);
  await lib.load();
  let release!: () => void;
  api.gate = new Promise<void>((resolve) => (release = resolve));

  lib.setTick("steam-553850", "mods", true);
  lib.setTick("steam-553850", "mods", true);
  lib.setTick("steam-553850", "overlay", true);
  release();
  await settle();

  const ticks = api.calls.filter((c) => c.startsWith("tick:"));
  assert.deepEqual(ticks.sort(), ["tick:steam-553850:mods:true", "tick:steam-553850:overlay:true"]);
});

test("the grid's headline follows the saved matrix", async () => {
  const api = new FakeApi();
  const lib = new LibraryController(api);
  await lib.load();
  const original = api.tick.bind(api);
  api.tick = async (id, feature, enabled) => ({ ...(await original(id, feature, enabled)), headline: "experimental" });
  lib.setTick("steam-553850", "mods", true);
  await settle();
  assert.equal(lib.getSnapshot().games.find((g) => g.id === "steam-553850")!.headline, "experimental");
  assert.equal(lib.getSnapshot().games.find((g) => g.id === "gog-1")!.headline, "unsupported");
});

// ---------------------------------------------------------------------------
// Play
// ---------------------------------------------------------------------------

test("Play hands a Steam game's app id to the backend and confirms", async () => {
  const api = new FakeApi();
  const lib = new LibraryController(api);
  await lib.load();
  lib.play(steamGame);
  assert.equal(lib.getSnapshot().pending[pendingKey("steam-553850", "play")], true);
  await settle();
  assert.deepEqual(api.calls.filter((c) => c.startsWith("play:")), ["play:553850"]);
  assert.equal(lib.getSnapshot().notices["steam-553850"], "Asked Steam to start Helldivers 2.");
  assert.equal(lib.getSnapshot().pending[pendingKey("steam-553850", "play")], undefined);
});

test("Play never reaches the backend for a game that is not from Steam or not found", async () => {
  const api = new FakeApi();
  const lib = new LibraryController(api);
  await lib.load();

  lib.play(gogGame);
  lib.play({ ...steamGame, present: false });
  await settle();

  assert.deepEqual(api.calls.filter((c) => c.startsWith("play:")), []);
  assert.match(lib.getSnapshot().gameErrors["gog-1"]!, /Only Steam games/);
  assert.match(lib.getSnapshot().gameErrors["steam-553850"]!, /Not found on the last scan/);
});

test("a failed launch is shown and replaces an earlier confirmation", async () => {
  const api = new FakeApi();
  const lib = new LibraryController(api);
  await lib.load();
  lib.play(steamGame);
  await settle();
  assert.ok(lib.getSnapshot().notices["steam-553850"]);

  api.failNext["play"] = "cmd /C start failed: exit code: 1";
  lib.play(steamGame);
  await settle();
  const s = lib.getSnapshot();
  assert.equal(s.gameErrors["steam-553850"], "cmd /C start failed: exit code: 1");
  assert.equal(s.notices["steam-553850"], undefined);
});

test("subscribers hear about changes, and snapshots stay stable in between", async () => {
  const api = new FakeApi();
  const lib = new LibraryController(api);
  let heard = 0;
  const stop = lib.subscribe(() => heard++);
  const a = lib.getSnapshot();
  assert.equal(lib.getSnapshot(), a, "no change, same object");
  await lib.load();
  assert.ok(heard > 0);
  assert.notEqual(lib.getSnapshot(), a);
  stop();
  const after = heard;
  await lib.scan();
  assert.equal(heard, after, "unsubscribed");
});

test("every compatibility verdict has a plain label", () => {
  assert.deepEqual(COMPAT_LABEL, {
    unverified: "Unverified",
    compatible: "Compatible",
    incompatible: "Incompatible",
    not_applicable: "Not applicable",
  });
});

// --- searching the games ---------------------------------------------------

const driveGame: GameView = { ...gogGame, id: "drive-1", name: "Beta Quest", publisher: null, source: "drive_scan" };
const GAMES = [steamGame, gogGame, driveGame];
const names = (games: GameView[]) => games.map((g) => g.name);

test("an empty or blank search keeps every game, in order", () => {
  assert.deepEqual(names(filterGames(GAMES, "")), ["Helldivers 2", "Alpha", "Beta Quest"]);
  assert.deepEqual(names(filterGames(GAMES, "   \t ")), ["Helldivers 2", "Alpha", "Beta Quest"]);
});

test("a search matches the name in any case, anywhere in it", () => {
  assert.deepEqual(names(filterGames(GAMES, "hell")), ["Helldivers 2"]);
  assert.deepEqual(names(filterGames(GAMES, "DIVERS")), ["Helldivers 2"]);
  assert.deepEqual(names(filterGames(GAMES, "quest")), ["Beta Quest"]);
  assert.deepEqual(names(filterGames(GAMES, "zzz")), []);
});

test("a search also matches the publisher and where the game was found", () => {
  assert.deepEqual(names(filterGames(GAMES, "studio")), ["Alpha"]);
  assert.deepEqual(names(filterGames(GAMES, "steam")), ["Helldivers 2"]);
  assert.deepEqual(names(filterGames(GAMES, "gog")), ["Alpha"]);
  assert.deepEqual(names(filterGames(GAMES, "on a drive")), ["Beta Quest"]);
});

test("every word must match, in any order, and extra spaces do not matter", () => {
  assert.deepEqual(names(filterGames(GAMES, "2 divers")), ["Helldivers 2"]);
  assert.deepEqual(names(filterGames(GAMES, "  divers   hell ")), ["Helldivers 2"]);
  assert.deepEqual(names(filterGames(GAMES, "studio alpha")), ["Alpha"]);
  assert.deepEqual(names(filterGames(GAMES, "alpha steam")), [], "one word that matches nothing rules the game out");
});

test("the install folder is not searched, and the list given is not changed", () => {
  // Helldivers 2 lives in D:\\SteamLibrary\\...; a folder name is not what the user means by a game.
  assert.deepEqual(filterGames(GAMES, "steamlibrary"), []);
  const before = [...GAMES];
  filterGames(GAMES, "alpha");
  assert.deepEqual(GAMES, before);
});
