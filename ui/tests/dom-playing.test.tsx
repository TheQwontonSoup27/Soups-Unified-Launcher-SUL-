import { absent, click, installTauri, mount, one, resetBrowser, settle, waitFor, type Mounted } from "./dom";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { afterEach, beforeEach, describe, test } from "node:test";
import { fileURLToPath } from "node:url";
import type { ReactElement } from "react";
import { PlayPanel } from "../src/components/PlayPanel";
import GameLibrary from "../src/views/GameLibrary";
import { tauriPlayApi, type LaunchRecord } from "../src/lib/playing";
import { gogGame, steamGame } from "./fixtures";
import { aBuild, aRefusal, aView, fakePlay } from "./play-fake";

beforeEach(() => resetBrowser());

// A panel that polls keeps a timer. A test that fails before it unmounts its panel would leave that timer running and the
// whole test process would never end, so every panel mounted here is unmounted after its test, passed or failed.
const mounted: Mounted[] = [];
async function mountTracked(ui: ReactElement): Promise<Mounted> {
  const m = await mount(ui);
  let gone = false;
  const tracked = {
    host: m.host,
    unmount() {
      if (gone) return;
      gone = true;
      m.unmount();
    },
  };
  mounted.push(tracked);
  return tracked;
}
afterEach(() => {
  for (const m of mounted.splice(0)) m.unmount();
});

const GAMES = [steamGame, gogGame];
const GOG = gogGame.id; // "gog-1": the id the fake's builds use
const BUILD = "gog-1-experimental-1";
const panel = (host: HTMLElement) => one(host, "section[aria-label='Play a managed build']");
const status = (host: HTMLElement) => one(panel(host), "[role=status]").textContent ?? "";
const alertText = (host: HTMLElement) => [...host.querySelectorAll("[role=alert]")].map((e) => e.textContent).join(" | ");
const row = (host: HTMLElement, id = BUILD) => one(host, `li[aria-label='Build ${id}']`);
const play = (host: HTMLElement, id = BUILD) => one<HTMLButtonElement>(row(host, id), `button[aria-label='Play build ${id}']`);
const history = (host: HTMLElement) => one(host, "div[aria-label='Launch history']");

async function start(view = aView(), pollMs = 60_000) {
  const fake = fakePlay(view);
  const m = await mountTracked(<PlayPanel api={fake.api} games={GAMES} pollMs={pollMs} />);
  await settle();
  return { fake, ...m };
}

describe("the play panel, clicked through", () => {
  test("in a plain browser it says it needs the desktop app and offers nothing to press", async () => {
    const { host, unmount } = await mountTracked(<PlayPanel api={null} games={GAMES} />);
    assert.match(panel(host).textContent ?? "", /Browser preview: starting a build needs the desktop app/);
    absent(host.querySelector("button, input, select"), "nothing to operate without a backend");
    assert.equal(one(panel(host), ".badge").textContent, "Experimental");
    unmount();
  });

  test("it is labelled Experimental and says what is not true yet: no real adapter, no risk checks on launch, no stop", async () => {
    const { host } = await start();
    const badge = one(panel(host), ".badge");
    assert.equal(badge.textContent, "Experimental");
    assert.ok(badge.classList.contains("badge-experimental"));
    const text = panel(host).textContent ?? "";
    assert.match(text, /tested on made-up programs only/);
    assert.match(text, /No adapter for a real game exists yet, so Play is refused for every real game today/);
    assert.match(text, /game-file risk checks \(integrity and anti-cheat signals\) are not connected to\s+launching yet/);
    assert.match(text, /SUL cannot stop a program it started/);
    assert.match(text, /Play button on a game card \(Steam\) is separate and unchanged/);
  });

  test("it lists builds under their game's name, with how each is kept and its state", async () => {
    const view = aView([aBuild(BUILD), aBuild("gog-1-original-1", { slot: "original", managed: false, active: false, state: "unverified" })]);
    const { host } = await start(view);
    const card = one(host, `div[aria-label='Builds of ${gogGame.name}']`);
    assert.match(card.textContent ?? "", /Not running/);
    assert.match(row(host).textContent ?? "", /managed copy.*active.*Matches its manifest/);
    assert.match(row(host, "gog-1-original-1").textContent ?? "", /install stays in place.*Not checked/);
    assert.doesNotMatch(row(host, "gog-1-original-1").textContent ?? "", /active/);
  });

  test("a build of a game with no adapter cannot be played, says why, and presses send nothing", async () => {
    const { host, fake } = await start(aView([aBuild(BUILD, { has_adapter: false })]));
    assert.match(row(host).textContent ?? "", /SUL has no adapter for this game, so it does not know how to start it/);
    assert.equal(play(host).disabled, true);
    await click(play(host));
    assert.equal(fake.starts().length, 0);
  });

  test("a build that is not ready cannot be played and the page says what to do", async () => {
    const { host } = await start(aView([aBuild(BUILD, { state: "failed" })]));
    assert.match(row(host).textContent ?? "", /Not ready: does not match its manifest\. Check it on the builds list first/);
    assert.equal(play(host).disabled, true);
  });

  test("Play sends that build and the output choice, shows the game as running, and holds Play while it runs", async () => {
    const { host, fake } = await start();
    await click(play(host));
    assert.deepEqual(fake.starts(), [{ op: "start", args: [BUILD, false] }]);
    assert.match(status(host), new RegExp(`Started build ${BUILD} \\(process 4242\\)\\.`));
    assert.match(one(host, `div[aria-label='Builds of ${gogGame.name}']`).textContent ?? "", /Running \(process 4242\) since 2023-11-14 22:13 UTC/);
    assert.equal(play(host).disabled, true);
    assert.match(row(host).textContent ?? "", /Running\./);
  });

  test("the output choice is sent with the start: off by default, on when ticked", async () => {
    const { host, fake } = await start();
    const box = one<HTMLInputElement>(panel(host), "input[type=checkbox]");
    assert.equal(box.checked, false);
    await click(box);
    assert.equal(box.checked, true);
    await click(play(host));
    assert.deepEqual(fake.starts(), [{ op: "start", args: [BUILD, true] }]);
    assert.match(panel(host).textContent ?? "", /off by default;\s+capped, with secrets masked/);
  });

  test("warnings of a start that went ahead are listed", async () => {
    const { host, fake } = await start(aView([aBuild(BUILD, { managed: false })]));
    fake.warnWith(["The program differs from the one recorded when the install was registered."]);
    await click(play(host));
    assert.match(one(host, "ul[aria-label='Warnings about this start']").textContent ?? "", /differs from the one recorded/);
  });

  test("a refusal from the core is shown in its own words, kept in the history, and the page then holds Play", async () => {
    const { host, fake } = await start();
    // The core's state moved on after the page last asked: the build no longer has an adapter.
    fake.change(BUILD, { has_adapter: false });
    await click(play(host));
    assert.match(alertText(host), /There is no adapter registered for this game, so SUL does not know how to start it/);
    assert.match(history(host).textContent ?? "", /was not started: There is no adapter registered/);
    assert.equal(play(host).disabled, true);
    assert.match(row(host).textContent ?? "", /SUL has no adapter for this game/);
  });

  test("an unexpected failure is shown as text and Play can be pressed again", async () => {
    const { host, fake } = await start();
    fake.failNext("start", "the launcher stopped working; restart the app");
    await click(play(host));
    assert.match(alertText(host), /launcher stopped working/);
    assert.equal(play(host).disabled, false);
    await click(play(host));
    assert.equal(fake.starts().length, 2);
    assert.equal(alertText(host), "");
  });

  test("while a start is working the buttons are held and the page says so", async () => {
    const { host, fake } = await start();
    const release = fake.hold();
    // Awaited on purpose: the click only starts the call, which then waits on the held gate. An un-awaited `act` would
    // leave React's act scope open and poison the tests that follow.
    await click(play(host));
    assert.match(status(host), /Starting…/);
    assert.equal(play(host).disabled, true);
    release();
    await settle();
    assert.doesNotMatch(status(host), /Starting…/);
  });

  test("a view that fails to load says why and lists nothing", async () => {
    const fake = fakePlay();
    fake.failNext("view", "the launch manager is damaged");
    const { host } = await mountTracked(<PlayPanel api={fake.api} games={GAMES} />);
    await settle();
    assert.match(alertText(host), /launch manager is damaged/);
    absent(host.querySelector("div[aria-label='Builds you can start']"), "no builds without a view");
  });

  test("with no builds the page says where to add one", async () => {
    const { host } = await start(aView([]));
    assert.match(panel(host).textContent ?? "", /No builds yet\. Add one from the library folders and builds list above/);
    assert.match(history(host).textContent ?? "", /Nothing has been started or refused since SUL opened/);
  });
});

describe("following a running game", () => {
  test("a running game is asked about on a timer, and its end is shown in the state, the history and the notice", async () => {
    const { host, fake } = await start(aView(), 20);
    fake.endsAfter(GOG, 1, 0);
    await click(play(host));
    await waitFor(() => /Last run ended normally/.test(panel(host).textContent ?? ""), "the end of the run to show");
    assert.match(status(host), new RegExp(`Build ${BUILD} ended normally\\.`));
    assert.match(history(host).textContent ?? "", /ended normally/);
    assert.match(history(host).textContent ?? "", /1 launch ended while this page was open/);
    assert.equal(play(host).disabled, false, "Play is available again");
    assert.ok(fake.calls.filter((c) => c.op === "refresh").length >= 2);
  });

  test("a failed exit is shown with its code and without a guessed cause", async () => {
    const { host, fake } = await start(aView(), 20);
    fake.endsAfter(GOG, 0, 3);
    await click(play(host));
    await waitFor(() => /exit code 3/.test(panel(host).textContent ?? ""), "the failure to show");
    assert.match(history(host).textContent ?? "", /ended with exit code 3\. The program ended soon after it started\. SUL cannot tell why\./);
    assert.match(panel(host).textContent ?? "", /Last run ended with exit code 3 at/);
  });

  test("the timer asks nothing when nothing runs, stops when the game ends, and stops when the panel goes away", async () => {
    const { host, fake, unmount } = await start(aView(), 15);
    await new Promise((r) => setTimeout(r, 80));
    assert.equal(fake.calls.filter((c) => c.op === "refresh").length, 0, "nothing runs, so nothing is asked");

    fake.endsAfter(GOG, 1000);
    await click(play(host));
    await waitFor(() => fake.calls.filter((c) => c.op === "refresh").length >= 2, "the timer to ask");
    unmount();
    const asked = fake.calls.filter((c) => c.op === "refresh").length;
    await new Promise((r) => setTimeout(r, 80));
    assert.equal(fake.calls.filter((c) => c.op === "refresh").length, asked, "no timer is left behind");
  });

  test("no timer exists while nothing runs: one is made when a game starts and cleared when it ends and when the panel goes away", async () => {
    // Count the intervals this panel keeps, told apart from every other timer by an interval length nothing else here uses.
    const MS = 37;
    const live = new Set<unknown>();
    const realSet = globalThis.setInterval;
    const realClear = globalThis.clearInterval;
    globalThis.setInterval = ((fn: () => void, ms?: number, ...rest: unknown[]) => {
      const id = realSet(fn, ms, ...rest);
      if (ms === MS) live.add(id);
      return id;
    }) as typeof setInterval;
    globalThis.clearInterval = ((id: Parameters<typeof clearInterval>[0]) => {
      live.delete(id);
      realClear(id);
    }) as typeof clearInterval;
    try {
      const { host, fake, unmount } = await start(aView(), MS);
      assert.equal(live.size, 0, "nothing runs, so no timer");
      fake.endsAfter(GOG, 1000);
      await click(play(host));
      assert.equal(live.size, 1, "a game runs, so one timer");
      unmount();
      assert.equal(live.size, 0, "the panel is gone, so is the timer");

      // A game that ends: the timer goes when the page learns of it.
      const second = await start(aView(), MS);
      second.fake.endsAfter(GOG, 0);
      await click(play(second.host));
      assert.equal(live.size, 1);
      await waitFor(() => /Last run ended normally/.test(panel(second.host).textContent ?? ""), "the end of the run to show");
      assert.equal(live.size, 0, "the game ended, so the timer is gone");
      second.unmount();
    } finally {
      globalThis.setInterval = realSet;
      globalThis.clearInterval = realClear;
    }
  });

  test("the history lists the newest launch first, keeps the output file's place and cuts after 25 rows", async () => {
    const records: LaunchRecord[] = Array.from({ length: 30 }, (_, i) => ({ ...aRefusal(`b${i}`, { reason: "no_such_build" }, `launch-${i}`), warnings: i === 0 ? ["watch out"] : [] }));
    records[1] = { ...records[1]!, outcome: { kind: "exited", code: 0, success: true, explanation: null }, log_file: "C:\\state\\launch-logs\\launch-1.log" };
    const { host } = await start(aView([aBuild(BUILD)], { history: records }));
    const rows = [...history(host).querySelectorAll("ul[aria-label='Launches, newest first'] > li")];
    assert.equal(rows.length, 25);
    assert.match(rows[0]!.textContent ?? "", /Build b0 was not started/);
    assert.match(rows[0]!.textContent ?? "", /watch out/);
    assert.match(rows[1]!.textContent ?? "", /Output kept in C:\\state\\launch-logs\\launch-1\.log\./);
    assert.match(history(host).textContent ?? "", /…and 5 older launches/);
  });
});

describe("on the Game Library page", () => {
  test("the Play panel is on the page, beside the Steam cards, and a test backend is not mixed up with the real one", async () => {
    const fake = fakePlay();
    const { host } = await mountTracked(<GameLibrary api={null} managerApi={null} playApi={fake.api} />);
    await settle();
    assert.ok(panel(host));
    assert.ok(row(host));

    // With a library backend given and no play backend, the Play panel gets none rather than reaching for the real one.
    const other = await mountTracked(<GameLibrary api={null} managerApi={null} />);
    assert.match(other.host.textContent ?? "", /Browser preview: starting a build needs the desktop app/);
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

  const record = aRefusal(BUILD, { reason: "no_adapter" });
  const answer = (cmd: string): unknown => {
    const view = aView();
    if (cmd === "launch_start") return { record, view };
    if (cmd === "launch_refresh") return { finished: [record], view };
    return view;
  };

  test("every command name and argument is the one the Rust commands take (camelCase top-level arguments)", async () => {
    const tauri = installTauri(answer);
    const api = tauriPlayApi();
    assert.ok(api, "inside the webview there is a backend");
    await api.view();
    await api.start(BUILD, false);
    await api.start(BUILD, true);
    await api.refresh();
    assert.deepEqual(tauri.calls, [
      { cmd: "launch_view", args: {} },
      { cmd: "launch_start", args: { buildId: BUILD, captureOutput: false } },
      { cmd: "launch_start", args: { buildId: BUILD, captureOutput: true } },
      { cmd: "launch_refresh", args: {} },
    ]);
    tauri.remove();
  });

  test("each of those commands is registered in src-tauri and takes exactly the arguments the page sends", async () => {
    const tauri = installTauri(answer);
    const api = tauriPlayApi()!;
    await api.view();
    await api.start(BUILD, false);
    await api.refresh();
    const names = registered();
    for (const c of tauri.calls) {
      assert.ok(names.includes(c.cmd), `${c.cmd} is sent by the page but not registered`);
      assert.deepEqual(Object.keys(c.args).sort(), expectedArgs(c.cmd), c.cmd);
    }
    assert.equal(tauri.calls.length, 3);
    assert.deepEqual(expectedArgs("launch_start"), ["buildId", "captureOutput"], "the parser really reads the Rust signature");
    tauri.remove();
  });

  test("in a plain browser there is no backend", () => {
    assert.equal(tauriPlayApi(), null);
  });

  test("data in the wrong shape is refused with a plain message instead of reaching the page", async () => {
    for (const junk of [[], null, "text", { builds: [] }, { builds: [], games: [] }, { record: {}, view: aView() }]) {
      const tauri = installTauri(() => junk);
      const api = tauriPlayApi()!;
      await assert.rejects(api.view(), /does not understand/);
      await assert.rejects(api.start(BUILD, false), /does not understand/);
      await assert.rejects(api.refresh(), /does not understand/);
      tauri.remove();
    }
  });

  test("a failing command's plain-text error reaches the page", async () => {
    const tauri = installTauri(() => {
      throw "the program launcher stopped working; restart the app";
    });
    const { host } = await mountTracked(<PlayPanel games={GAMES} />);
    await settle();
    assert.match(alertText(host), /launcher stopped working/);
    tauri.remove();
  });
});

