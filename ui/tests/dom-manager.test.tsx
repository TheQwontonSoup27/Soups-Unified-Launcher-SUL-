import { absent, act, choose, click, installTauri, mount, one, resetBrowser, settle, submit, type, withText } from "./dom";
import assert from "node:assert/strict";
import { beforeEach, describe, test } from "node:test";
import { ManagedBuildsPanel } from "../src/components/ManagedBuildsPanel";
import { tauriManagerApi } from "../src/lib/manager";
import type { LibraryApi, ScanView } from "../src/lib/library";
import GameLibrary from "../src/views/GameLibrary";
import { gogGame, steamGame } from "./fixtures";
import { build, fakeManager, root } from "./manager-fake";

beforeEach(() => resetBrowser());

const GAMES = [steamGame, gogGame];
const panel = (host: HTMLElement) => one(host, "section[aria-label='Folders and builds']");
const form = (host: HTMLElement, label: string) => one<HTMLFormElement>(host, `form[aria-label='${label}']`);
/** The input or select of the label whose own text (not its options') is `label`. */
const field = (f: HTMLElement, label: string) => {
  const found = [...f.querySelectorAll("label")].filter((l) => l.firstChild?.textContent?.trim() === label);
  if (found.length !== 1) throw new Error(`expected exactly one label ${JSON.stringify(label)}, found ${found.length}`);
  return found[0]!.querySelector("input, select") as HTMLInputElement & HTMLSelectElement;
};
const dialog = (host: HTMLElement) => one(host, "[role=dialog]");
const noDialog = (host: HTMLElement) => absent(host.querySelector("[role=dialog]"), "no question should be open");
const alertText = (host: HTMLElement) => host.querySelector("[role=alert]")?.textContent ?? "";

async function start(initial = {}, games = GAMES) {
  const fake = fakeManager(initial);
  const m = await mount(<ManagedBuildsPanel api={fake.api} games={games} />);
  await settle();
  return { fake, ...m };
}

describe("the folders-and-builds panel, clicked through", () => {
  test("in a plain browser it says it needs the desktop app and offers nothing to press", async () => {
    const { host, unmount } = await mount(<ManagedBuildsPanel api={null} games={GAMES} />);
    assert.match(panel(host).textContent ?? "", /Browser preview.*need the desktop app/);
    absent(host.querySelector("button, input, select"), "nothing to operate without a backend");
    assert.equal(one(panel(host), ".badge").textContent, "Experimental", "even the preview says what the label is");
    unmount();
  });

  test("it is labelled Experimental and says no real game adapter exists", async () => {
    const { host, unmount } = await start();
    const badge = one(panel(host), ".badge");
    assert.equal(badge.textContent, "Experimental");
    assert.ok(badge.classList.contains("badge-experimental"));
    assert.match(panel(host).textContent ?? "", /Experimental/);
    assert.match(panel(host).textContent ?? "", /No adapter for a real game exists yet/);
    assert.match(panel(host).textContent ?? "", /original installs are only ever read/);
    unmount();
  });

  test("it lists the saved folders and builds, marks the active one, and describes where each lives", async () => {
    const { host, unmount } = await start({
      roots: [root("main", "Main")],
      builds: [build("hd2-original-1", steamGame.id, "original", { active: true }), build("hd2-legacy-2", steamGame.id, "legacy", { location: { kind: "external_install", path: "D:\\SteamLibrary" } })],
      slots: [{ game_id: steamGame.id, slot: "original", build_id: "hd2-original-1", history: [] }],
    });
    assert.match(one(host, "[aria-label='Library folders']").textContent ?? "", /Main.*Game builds.*500 GB free/);
    const builds = one(host, "[aria-label='Builds of Helldivers 2']");
    assert.equal(builds.querySelectorAll("li").length, 2);
    assert.match(withText(builds, "li", builds.querySelectorAll("li")[0]!.textContent!.trim()).textContent ?? "", /Original.*Active.*Matches its manifest.*A copy in Main/);
    assert.match(builds.querySelectorAll("li")[1]!.textContent ?? "", /Legacy.*The original install, where it is \(read only\)/);
    unmount();
  });

  test("a folder that is missing, or has another drive in its place, is flagged where it is listed", async () => {
    const { host, unmount } = await start({ roots: [root("a", "Gone", "builds", { present: false, marker_ok: false }), root("b", "Swapped", "mods", { marker_ok: false })] });
    const text = one(host, "[aria-label='Library folders']").textContent ?? "";
    assert.match(text, /The folder is not there/);
    assert.match(text, /A different folder or drive is here/);
    unmount();
  });

  test("an operation that was interrupted is shown as a warning that names what it was doing", async () => {
    const { host, unmount } = await start({ interrupted: [{ id: "7", op: "import", detail: "copying Helldivers 2 into Main" }] });
    const banner = one(host, "[aria-label='Unfinished operations']");
    assert.match(banner.textContent ?? "", /An earlier operation did not finish/);
    assert.match(banner.textContent ?? "", /import.*copying Helldivers 2 into Main/);
    assert.match(banner.textContent ?? "", /does not repair this by itself/);
    unmount();
  });

  test("adding a folder sends a tidy id, the typed path and the kind, then clears the form and says so", async () => {
    const { fake, host, unmount } = await start();
    const f = form(host, "Add a library folder");
    await type(field(f, "Name"), "Big Disk");
    await type(field(f, "Path"), "D:\\SUL\\Big");
    await choose(field(f, "Holds") as unknown as HTMLSelectElement, "mods");
    await submit(f);
    assert.deepEqual(fake.changes(), [{ op: "addRoot", args: ["big-disk", "Big Disk", "D:\\SUL\\Big", "mods"] }]);
    assert.match(one(host, "[aria-label='Library folders']").textContent ?? "", /Big Disk.*Mods/);
    assert.match(panel(host).textContent ?? "", /Added the folder “Big Disk”/);
    assert.equal(field(form(host, "Add a library folder"), "Name").value, "");
    unmount();
  });

  test("adding a folder with no path says so and asks the backend nothing; a refusal keeps what was typed", async () => {
    const { fake, host, unmount } = await start();
    const f = form(host, "Add a library folder");
    await type(field(f, "Name"), "Big Disk");
    await submit(f);
    assert.match(alertText(host), /Type the path/);
    assert.deepEqual(fake.changes(), []);
    await type(field(f, "Path"), "D:\\nope");
    fake.failNext("addRoot", "that folder does not exist, so SUL will not create it");
    await submit(f);
    assert.match(alertText(host), /does not exist, so SUL will not create it/);
    assert.equal(field(form(host, "Add a library folder"), "Name").value, "Big Disk");
    unmount();
  });

  test("stopping to list a folder asks first; Cancel changes nothing; the core's refusal is shown", async () => {
    const { fake, host, unmount } = await start({
      roots: [root("main", "Main")],
      builds: [build("b1", steamGame.id, "original")],
    });
    await click(one(host, "button[aria-label='Stop listing Main']"));
    assert.match(dialog(host).textContent ?? "", /The folder and everything in it stay exactly as they are/);
    assert.deepEqual(fake.changes(), []);
    await click(withText(dialog(host), "button", "Cancel"));
    noDialog(host);
    assert.deepEqual(fake.changes(), []);

    await click(one(host, "button[aria-label='Stop listing Main']"));
    await click(withText(dialog(host), "button", "Stop listing"));
    noDialog(host);
    assert.deepEqual(fake.changes(), [{ op: "forgetRoot", args: ["main"] }]);
    assert.match(alertText(host), /still holds 1 build/);
    assert.match(one(host, "[aria-label='Library folders']").textContent ?? "", /Main/);
    unmount();
  });

  test("a folder that holds no build can be forgotten after confirming, and the folder itself is untouched", async () => {
    const { fake, host, unmount } = await start({ roots: [root("spare", "Spare")] });
    await click(one(host, "button[aria-label='Stop listing Spare']"));
    await click(withText(dialog(host), "button", "Stop listing"));
    assert.deepEqual(fake.changes(), [{ op: "forgetRoot", args: ["spare"] }]);
    assert.match(one(host, "[aria-label='Library folders']").textContent ?? "", /No library folders yet/);
    assert.match(panel(host).textContent ?? "", /The folder and its files were not touched/);
    unmount();
  });

  test("pointing a folder at its new place sends the new path", async () => {
    const { fake, host, unmount } = await start({ roots: [root("main", "Main", "builds", { present: false, marker_ok: false })] });
    await click(one(host, "button[aria-label='Point Main at a new place']"));
    const f = form(host, "New place for Main");
    await type(field(f, "New path"), "E:\\SUL\\Main");
    await submit(f);
    assert.deepEqual(fake.changes(), [{ op: "relinkRoot", args: ["main", "E:\\SUL\\Main"] }]);
    absent(host.querySelector("form[aria-label='New place for Main']"), "the form closes once it worked");
    absent(host.querySelector("[aria-label='Library folders'] .matrix-error"), "the problem is gone");
    unmount();
  });

  test("a copy is only made after the question is answered Copy, and the question names the source, the target and the size", async () => {
    const { fake, host, unmount } = await start({ roots: [root("main", "Main")] });
    const f = form(host, "Add a build");
    await type(field(f, "Game version"), "7.1.1");
    await click(withText(f, "button", "Copy into a library folder…"));
    const q = dialog(host).textContent ?? "";
    assert.match(q, /copy every file of Helldivers 2 from D:\\SteamLibrary\\steamapps\\common\\Helldivers 2 into “Main”/);
    assert.match(q, /500 GB free/);
    assert.match(q, /original install is only read and is never changed/);
    assert.match(q, /cannot be cancelled once it starts/);
    assert.deepEqual(fake.changes(), [], "nothing is sent before the answer");

    await click(withText(dialog(host), "button", "Cancel"));
    noDialog(host);
    assert.deepEqual(fake.changes(), [], "Cancel sends nothing");

    await click(withText(f, "button", "Copy into a library folder…"));
    await click(withText(dialog(host), "button", "Copy"));
    noDialog(host);
    assert.deepEqual(fake.changes(), [
      { op: "importBuild", args: [{ gameId: steamGame.id, variant: "original", baseVersion: "7.1.1", sourceDescription: "my own install" }, "main"] },
    ]);
    assert.match(panel(host).textContent ?? "", /Copied Helldivers 2 into “Main” and checked the copy/);
    assert.equal(one(host, "[aria-label='Builds of Helldivers 2']").querySelectorAll("li").length, 1);
    unmount();
  });

  test("Escape closes the question without sending anything", async () => {
    const { fake, host, unmount } = await start({ roots: [root("main", "Main")] });
    await type(field(form(host, "Add a build"), "Game version"), "1");
    await click(withText(form(host, "Add a build"), "button", "Copy into a library folder…"));
    await act(async () => {
      window.dispatchEvent(new window.KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    });
    noDialog(host);
    assert.deepEqual(fake.changes(), []);
    unmount();
  });

  test("a custom variant name is cleaned up and sent as a custom variant; an empty one is not sent", async () => {
    const { fake, host, unmount } = await start({ roots: [root("main", "Main")] });
    const f = form(host, "Add a build");
    await choose(field(f, "Slot") as unknown as HTMLSelectElement, "custom");
    await type(field(f, "Game version"), "7.1.1");
    await click(withText(f, "button", "Record in place…"));
    assert.match(alertText(host), /Give the custom variant a name/);
    noDialog(host);

    await type(field(form(host, "Add a build"), "Custom name"), "Tuned Audio!");
    await type(field(form(host, "Add a build"), "Where it came from"), "Steam, version 7.1.1");
    await click(withText(form(host, "Add a build"), "button", "Record in place…"));
    assert.match(dialog(host).textContent ?? "", /copies nothing and writes nothing there/);
    assert.deepEqual(fake.changes(), []);
    await click(withText(dialog(host), "button", "Record"));
    assert.deepEqual(fake.changes(), [
      { op: "registerInPlace", args: [{ gameId: steamGame.id, variant: { custom: "tuned-audio" }, baseVersion: "7.1.1", sourceDescription: "Steam, version 7.1.1" }] },
    ]);
    unmount();
  });

  test("a build needs a version, a folder and a game; the form says what is missing and asks nothing", async () => {
    const { fake, host, unmount } = await start();
    const f = form(host, "Add a build");
    await click(withText(f, "button", "Copy into a library folder…"));
    assert.match(alertText(host), /Type the game's version/);
    await type(field(f, "Game version"), "1.0");
    await click(withText(form(host, "Add a build"), "button", "Copy into a library folder…"));
    assert.match(alertText(host), /Add a library folder for builds first/);
    noDialog(host);
    assert.deepEqual(fake.calls.filter((c) => c.op !== "view"), []);
    unmount();
  });

  test("only games the last scan found can be chosen; with none, the panel says to scan first", async () => {
    const lost = { ...gogGame, present: false };
    const { host, unmount } = await start({ roots: [root("main", "Main")] }, [steamGame, lost]);
    const options = [...field(form(host, "Add a build"), "Game").querySelectorAll("option")].map((o) => o.textContent);
    assert.deepEqual(options, ["Helldivers 2"]);
    unmount();

    const none = await start({}, []);
    assert.match(panel(none.host).textContent ?? "", /No game to add yet. Scan for games above first/);
    absent(none.host.querySelector("form[aria-label='Add a build']"), "no build form without a game");
    none.unmount();
  });

  test("a refused import shows the core's own words and keeps the form", async () => {
    const { fake, host, unmount } = await start({ roots: [root("main", "Main")] });
    fake.failNext("importBuild", "not enough free space in “Main”: the copy needs 60 GB and 10 GB is free");
    const f = form(host, "Add a build");
    await type(field(f, "Game version"), "7.1.1");
    await click(withText(f, "button", "Copy into a library folder…"));
    await click(withText(dialog(host), "button", "Copy"));
    assert.match(alertText(host), /not enough free space/);
    assert.equal(field(form(host, "Add a build"), "Game version").value, "7.1.1");
    assert.equal(host.querySelector("[aria-label='Builds of Helldivers 2']"), null);
    unmount();
  });

  test("while an operation runs every button is off and the page says it is working", async () => {
    const { fake, host, unmount } = await start({ roots: [root("main", "Main")], builds: [build("b1", steamGame.id, "original")] });
    const release = fake.hold();
    await click(one(host, "button[aria-label='Check the files of b1']"));
    assert.match(panel(host).textContent ?? "", /Working…/);
    for (const b of host.querySelectorAll<HTMLButtonElement>("button")) assert.equal(b.disabled, true, `${b.textContent} should be disabled`);
    release();
    await settle();
    assert.match(panel(host).textContent ?? "", /Matches its manifest/);
    assert.equal(one<HTMLButtonElement>(host, "button[aria-label='Check the files of b1']").disabled, false);
    unmount();
  });

  test("Check files shows the result next to the build; a mismatch is stated as a mismatch", async () => {
    const { fake, host, unmount } = await start({ builds: [build("b1", steamGame.id, "original")] });
    fake.drift("b1");
    await click(one(host, "button[aria-label='Check the files of b1']"));
    assert.match(one(host, "[aria-label='Builds of Helldivers 2']").textContent ?? "", /Last check: Does not match its manifest: 0 missing, 1 changed\./);
    unmount();
  });

  test("Make active is off for the active build; making another active turns on Roll back, which goes back", async () => {
    const { fake, host, unmount } = await start({
      builds: [build("a", steamGame.id, "original", { active: true }), build("b", steamGame.id, "original")],
      slots: [{ game_id: steamGame.id, slot: "original", build_id: "a", history: [] }],
    });
    assert.equal(one<HTMLButtonElement>(host, "button[aria-label='Make a the active build']").disabled, true);
    absent(host.querySelector("button[aria-label^='Roll the original slot']"), "nothing to roll back to yet");

    await click(one(host, "button[aria-label='Make b the active build']"));
    assert.deepEqual(fake.changes(), [{ op: "activate", args: ["b"] }]);
    assert.equal(one<HTMLButtonElement>(host, "button[aria-label='Make b the active build']").disabled, true);

    await click(one(host, "button[aria-label='Roll the original slot of Helldivers 2 back']"));
    assert.deepEqual(fake.changes().map((c) => c.op), ["activate", "rollback"]);
    assert.equal(one<HTMLButtonElement>(host, "button[aria-label='Make a the active build']").disabled, true);
    absent(host.querySelector("button[aria-label^='Roll the original slot']"), "the history is used up");
    unmount();
  });

  test("copies SUL no longer manages are counted and left alone, in words that say SUL never deletes them", async () => {
    const { host, unmount } = await start({ retired: [{ build_id: "old", game_id: steamGame.id, location: { kind: "managed", root_id: "main", dir: "old" }, retired_unix: 1, reason: "moved" }] });
    assert.match(panel(host).textContent ?? "", /1 old copy SUL no longer manages is still on disk. SUL never deletes them/);
    unmount();
  });
});

describe("the panel on the Game Library page", () => {
  const libraryApi = (games = GAMES): LibraryApi => ({
    async list() {
      return games;
    },
    async scan(): Promise<ScanView> {
      return { games, added: 0, updated: 0, returned: 0, missing: 0 };
    },
    async matrix() {
      throw "unused";
    },
    async tick() {
      throw "unused";
    },
    async play() {},
  });

  test("it appears below the games and offers the games the library found", async () => {
    const fake = fakeManager({ roots: [root("main", "Main")] });
    const { host, unmount } = await mount(<GameLibrary api={libraryApi()} managerApi={fake.api} />);
    await settle();
    const options = [...field(form(host, "Add a build"), "Game").querySelectorAll("option")].map((o) => o.textContent);
    assert.deepEqual(options, ["Helldivers 2", "Alpha"]);
    unmount();
  });

  test("given a fake library and no manager backend, it does not reach for the real one", async () => {
    const calls = installTauri(() => {
      throw "the real manager backend must not be used here";
    });
    const { host, unmount } = await mount(<GameLibrary api={libraryApi()} />);
    await settle();
    assert.match(panel(host).textContent ?? "", /Browser preview/);
    assert.deepEqual(calls.calls, []);
    calls.remove();
    unmount();
  });
});

describe("what the panel sends to the real invoke", () => {
  test("every command name and argument is the one the Rust commands take (camelCase arguments)", async () => {
    const tauri = installTauri((cmd) => {
      if (cmd === "manager_import_build" || cmd === "manager_register_in_place") return { build_id: "x", view: emptyView() };
      if (cmd === "manager_verify_build") return { check: { state: "ready", report: { missing: [], modified: [], extra: [] } }, view: emptyView() };
      return emptyView();
    });
    const api = tauriManagerApi();
    assert.ok(api, "inside the webview there is a backend");
    const req = { gameId: "g", variant: { custom: "t" } as const, baseVersion: "1", sourceDescription: "s" };
    await api.view();
    await api.addRoot("id", "Label", "D:\\p", "builds");
    await api.relinkRoot("id", "E:\\q");
    await api.forgetRoot("id");
    await api.importBuild(req, "root");
    await api.registerInPlace(req);
    await api.verify("b");
    await api.activate("b");
    await api.rollback("g", "original");
    assert.deepEqual(tauri.calls, [
      { cmd: "manager_view", args: {} },
      { cmd: "manager_add_root", args: { id: "id", label: "Label", path: "D:\\p", kind: "builds" } },
      { cmd: "manager_relink_root", args: { id: "id", path: "E:\\q" } },
      { cmd: "manager_forget_root", args: { id: "id" } },
      { cmd: "manager_import_build", args: { gameId: "g", variant: { custom: "t" }, baseVersion: "1", sourceDescription: "s", rootId: "root" } },
      { cmd: "manager_register_in_place", args: { gameId: "g", variant: { custom: "t" }, baseVersion: "1", sourceDescription: "s" } },
      { cmd: "manager_verify_build", args: { buildId: "b" } },
      { cmd: "manager_activate_build", args: { buildId: "b" } },
      { cmd: "manager_rollback_slot", args: { gameId: "g", slot: "original" } },
    ]);
    tauri.remove();
  });

  test("in a plain browser there is no backend", () => {
    assert.equal(tauriManagerApi(), null);
  });

  test("data in the wrong shape is refused with a plain message instead of reaching the page", async () => {
    for (const junk of [[], null, "text", { roots: [] }, { roots: [], builds: [], slots: [], retired: [] }]) {
      const tauri = installTauri(() => junk);
      const api = tauriManagerApi()!;
      await assert.rejects(api.view(), /does not understand/);
      await assert.rejects(api.activate("b"), /does not understand/);
      await assert.rejects(api.importBuild({ gameId: "g", variant: "original", baseVersion: "1", sourceDescription: "s" }, "r"), /does not understand/);
      await assert.rejects(api.verify("b"), /does not understand/);
      tauri.remove();
    }
  });

  test("a build or a check without its list of folders and builds is refused too", async () => {
    const tauri = installTauri((cmd) => (cmd === "manager_verify_build" ? { check: { state: "ready" }, view: emptyView() } : { build_id: 7, view: emptyView() }));
    const api = tauriManagerApi()!;
    await assert.rejects(api.verify("b"), /does not understand/);
    await assert.rejects(api.registerInPlace({ gameId: "g", variant: "original", baseVersion: "1", sourceDescription: "s" }), /does not understand/);
    tauri.remove();
  });

  test("a page whose backend sends junk shows an error and stays up", async () => {
    const tauri = installTauri(() => []);
    const { host, unmount } = await mount(<ManagedBuildsPanel games={[]} />);
    await settle();
    assert.match(one(host, "[role=alert]").textContent ?? "", /does not understand/);
    unmount();
    tauri.remove();
  });
});

function emptyView() {
  return { roots: [], builds: [], slots: [], retired: [], interrupted: [] };
}
