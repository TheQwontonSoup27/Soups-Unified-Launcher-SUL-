import { absent, click, installTauri, keyOn, mount, one, resetBrowser, settle, stubFetch, type, withText, act } from "./dom";
import assert from "node:assert/strict";
import { afterEach, beforeEach, describe, test } from "node:test";
import { App } from "../src/App";
import { Splash } from "../src/components/Splash";
import { TitleBar } from "../src/components/TitleBar";
import { getAppInfo, getInvoker, getWindowControls } from "../src/lib/backend";
import { SIDEBAR_MODULES } from "../src/modules/registry";
import { gogGame, steamGame } from "./fixtures";

let tauri: ReturnType<typeof installTauri> | null = null;
let undo: Array<() => void> = [];
beforeEach(() => {
  resetBrowser();
  undo.push(stubFetch([]));
  window.history.replaceState(null, "", "/");
});
afterEach(() => {
  tauri?.remove();
  tauri = null;
  for (const f of undo.reverse()) f();
  undo = [];
});

const wait = (ms: number) => act(async () => void (await new Promise((r) => setTimeout(r, ms))));
const nav = (host: HTMLElement, label: string) => withText<HTMLButtonElement>(host, "button.nav-item", label);

describe("splash", () => {
  test("the logo is shown first, and a click skips it", async () => {
    const { host, unmount } = await mount(<App splashMs={60000} />);
    const splash = one(host, ".splash");
    assert.equal(splash.getAttribute("role"), "img");
    assert.equal(splash.getAttribute("aria-label"), "Soups Unified Launcher, Team27");
    assert.equal(one(splash, "img").getAttribute("src"), "logo.png");
    absent(host.querySelector(".shell"));
    await click(splash);
    absent(host.querySelector(".splash"));
    assert.ok(host.querySelector(".shell"));
    unmount();
  });

  test("it leaves by itself after the set time", async () => {
    const { host, unmount } = await mount(<App splashMs={40} />);
    assert.ok(host.querySelector(".splash"));
    await wait(120);
    absent(host.querySelector(".splash"));
    assert.ok(host.querySelector(".shell"));
    unmount();
  });

  test("with a zero time the app opens straight on the shell", async () => {
    const { host, unmount } = await mount(<App splashMs={0} />);
    absent(host.querySelector(".splash"));
    assert.ok(host.querySelector(".shell"));
    unmount();
  });

  test("for people who asked for less motion a long splash is cut to 600 ms", async () => {
    const original = window.matchMedia;
    window.matchMedia = ((q: string) => ({ matches: q.includes("prefers-reduced-motion"), media: q })) as typeof window.matchMedia;
    try {
      let done = 0;
      const { unmount } = await mount(<Splash ms={60000} onDone={() => done++} />);
      await wait(300);
      assert.equal(done, 0, "not yet");
      await wait(450);
      assert.equal(done, 1, "gone by 750 ms");
      unmount();
    } finally {
      window.matchMedia = original;
    }
  });

  test("a splash that is closed early does not fire later", async () => {
    let done = 0;
    const { unmount } = await mount(<Splash ms={50} onDone={() => done++} />);
    unmount();
    await wait(120);
    assert.equal(done, 0);
  });
});

describe("navigation", () => {
  test("it opens on the Game Library, and every sidebar entry opens its own page", async () => {
    const { host, unmount } = await mount(<App splashMs={0} />);
    assert.equal(one(host, "h1").textContent, "Game Library");
    assert.equal(nav(host, "Game Library").getAttribute("aria-current"), "page");

    const pages: Array<[string, string]> = [
      ...SIDEBAR_MODULES.map((m): [string, string] => [m.title, m.title]),
      ["Settings", "Settings"],
      ["General Playtime Tracker", "General Playtime Tracker"],
      ["Game Library", "Game Library"],
    ];
    for (const [label, heading] of pages) {
      await click(nav(host, label));
      assert.equal(one(host, "h1").textContent, heading, label);
      assert.equal(nav(host, label).getAttribute("aria-current"), "page", label);
      assert.equal(host.querySelectorAll("[aria-current=page]").length, 1, "exactly one entry is current");
    }
    unmount();
  });

  test("the placeholder pages say so, and the playtime tracker tracks nothing", async () => {
    const { host, unmount } = await mount(<App splashMs={0} />);
    await click(nav(host, "General Playtime Tracker"));
    assert.match(host.textContent ?? "", /Placeholder\. Nothing is tracked yet\./);
    await click(nav(host, "Emulation"));
    assert.match(host.textContent ?? "", /Nothing in this module is implemented yet/);
    assert.match(host.textContent ?? "", /Not investigated/);
    unmount();
  });

  test("Settings, opened from the sidebar, shows its tabs and the footer stays on every page", async () => {
    const { host, unmount } = await mount(<App splashMs={0} />);
    await click(nav(host, "Settings"));
    assert.equal(host.querySelectorAll("[role=tab]").length, 3);
    assert.ok(host.querySelector("footer.status"));
    unmount();
  });

  test("the sidebar's top row holds the app icon, the search box and the presence switch, in that order", async () => {
    const { host, unmount } = await mount(<App splashMs={0} />);
    const kids = [...one(host, ".sidebar-top").children].map((c) => c.tagName + "." + c.className);
    assert.deepEqual(kids, ["IMG.app-icon", "INPUT.global-search", "BUTTON.rpc-toggle"]);
    assert.equal(one(host, ".app-icon").getAttribute("src"), "logo.png");
    assert.equal(one(host, ".global-search").getAttribute("aria-label"), "Search games");
    assert.equal(one(host, ".global-search").getAttribute("placeholder"), "Search games");
    unmount();
  });
});

describe("the search box", () => {
  const search = (host: HTMLElement) => one<HTMLInputElement>(host, ".global-search");
  const shown = (host: HTMLElement) => [...host.querySelectorAll("[aria-label='Your games'] .game-card h3")].map((h) => h.textContent);
  const withGames = () => {
    tauri = installTauri((cmd) => (cmd === "list_games" ? [steamGame, gogGame] : null));
    return tauri;
  };

  test("typing narrows the games on the page as you type, and clearing brings them all back", async () => {
    withGames();
    const { host, unmount } = await mount(<App splashMs={0} />);
    await settle();
    assert.deepEqual(shown(host), ["Helldivers 2", "Alpha"]);

    await type(search(host), "alp");
    assert.deepEqual(shown(host), ["Alpha"]);
    assert.equal(one(host, "[aria-label=Scan] p[role=status]").textContent, "Showing 1 of 2 games matching “alp”.");

    await type(search(host), "");
    assert.deepEqual(shown(host), ["Helldivers 2", "Alpha"]);
    unmount();
  });

  test("Escape empties the box and shows every game again", async () => {
    withGames();
    const { host, unmount } = await mount(<App splashMs={0} />);
    await settle();
    await type(search(host), "hell");
    assert.deepEqual(shown(host), ["Helldivers 2"]);

    await keyOn(search(host), "Escape");
    assert.equal(search(host).value, "");
    assert.deepEqual(shown(host), ["Helldivers 2", "Alpha"]);
    unmount();
  });

  test("searching from another page goes to the Game Library, where the games are", async () => {
    withGames();
    const { host, unmount } = await mount(<App splashMs={0} />);
    await settle();
    await click(nav(host, "Settings"));
    assert.equal(one(host, ".page-header h1").textContent, "Settings");

    await type(search(host), "alpha");
    await settle();
    assert.equal(one(host, ".page-header h1").textContent, "Game Library");
    assert.deepEqual(shown(host), ["Alpha"]);
    assert.equal(nav(host, "Game Library").getAttribute("aria-current"), "page");
    unmount();
  });

  test("a blank search does not move you off the page you are on", async () => {
    withGames();
    const { host, unmount } = await mount(<App splashMs={0} />);
    await settle();
    await click(nav(host, "Settings"));
    await type(search(host), "   ");
    assert.equal(one(host, ".page-header h1").textContent, "Settings");
    unmount();
  });

  test("searching is done on the page: the backend is asked for the games once, not per letter", async () => {
    const t = withGames();
    const { host, unmount } = await mount(<App splashMs={0} />);
    await settle();
    for (const text of ["a", "al", "alp", "alph"]) await type(search(host), text);
    assert.equal(t.calls.filter((c) => c.cmd === "list_games").length, 1);
    unmount();
  });
});

describe("the desktop footer", () => {
  test("a plain browser says there is no backend", async () => {
    const { host, unmount } = await mount(<App splashMs={0} />);
    await settle();
    assert.equal(one(host, "footer.status").textContent, "Browser preview: no Tauri backend connected");
    unmount();
  });

  test("with the desktop app it shows the version, install mode and folder the backend reports", async () => {
    tauri = installTauri((cmd) => (cmd === "app_info" ? { name: "Soups Unified Launcher", version: "0.1.0", install_mode: "portable", app_root: "D:\\SUL" } : []));
    const { host, unmount } = await mount(<App splashMs={0} />);
    await settle();
    assert.equal(one(host, "footer.status").textContent, "v0.1.0 · portable install · D:\\SUL");
    unmount();
  });
});

describe("title bar", () => {
  test("in a plain browser it has the name and the early-development note but no window buttons", async () => {
    const { host, unmount } = await mount(<TitleBar />);
    assert.equal(one(host, "strong").textContent, "Soups Unified Launcher");
    assert.match(one(host, ".titlebar-note").textContent ?? "", /Early development/);
    absent(host.querySelector(".wincontrols"));
    assert.equal(one(host, "img").getAttribute("width"), "24");
    unmount();
  });

  test("?desktop=1 shows the buttons for design review, and pressing them in a browser does nothing", async () => {
    window.history.replaceState(null, "", "/?desktop=1");
    const { host, unmount } = await mount(<TitleBar />);
    const labels = [...host.querySelectorAll(".wincontrols button")].map((b) => b.getAttribute("aria-label"));
    assert.deepEqual(labels, ["Minimize", "Maximize or restore", "Close"]);
    for (const b of host.querySelectorAll(".wincontrols button")) await click(b);
    unmount();
  });

  test("with the desktop app each button sends its own window command to the main window", async () => {
    tauri = installTauri(() => null);
    const { host, unmount } = await mount(<TitleBar />);
    await click(one(host, "[aria-label=Minimize]"));
    await click(one(host, "[aria-label='Maximize or restore']"));
    await click(one(host, "[aria-label=Close]"));
    assert.deepEqual(
      tauri.calls.map((c) => [c.cmd, c.args.label]),
      [
        ["plugin:window|minimize", "main"],
        ["plugin:window|toggle_maximize", "main"],
        ["plugin:window|close", "main"],
      ],
    );
    unmount();
  });

  test("the bar is a drag area, and the buttons are not (so they stay clickable)", async () => {
    tauri = installTauri(() => null);
    const { host, unmount } = await mount(<TitleBar />);
    assert.ok(one(host, "header").hasAttribute("data-tauri-drag-region"));
    assert.ok(one(host, "strong").hasAttribute("data-tauri-drag-region"));
    for (const b of host.querySelectorAll(".wincontrols button")) assert.ok(!b.hasAttribute("data-tauri-drag-region"));
    unmount();
  });
});

describe("the backend bridge", () => {
  test("in a plain browser there is no backend of any kind", async () => {
    assert.equal(await getAppInfo(), null);
    assert.equal(getInvoker(), null);
    assert.equal(getWindowControls(), null);
  });

  test("with the desktop app the bridge passes command names and arguments through unchanged", async () => {
    tauri = installTauri((cmd) => (cmd === "app_info" ? { version: "9.9.9" } : "ok"));
    assert.deepEqual(await getAppInfo(), { version: "9.9.9" });
    const invoke = getInvoker()!;
    assert.equal(await invoke("any_command", { a: 1 }), "ok");
    assert.deepEqual(tauri.calls, [
      { cmd: "app_info", args: {} },
      { cmd: "any_command", args: { a: 1 } },
    ]);
  });

  test("if the window object cannot be built the controls are simply absent", async () => {
    tauri = installTauri(() => null);
    delete (window as unknown as { __TAURI_INTERNALS__: { metadata?: unknown } }).__TAURI_INTERNALS__.metadata;
    assert.equal(getWindowControls(), null);
  });
});
