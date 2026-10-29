// The contract between the page and the desktop shell, checked from both ends: what the UI really
// sends, against what `src-tauri` really registers, accepts and permits.
import { installTauri, mount, resetBrowser, settle, click, one } from "./dom";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { afterEach, beforeEach, describe, test } from "node:test";
import { fileURLToPath } from "node:url";
import { TitleBar } from "../src/components/TitleBar";
import { setPresence } from "../src/discord";
import { getAppInfo, getInvoker } from "../src/lib/backend";
import { tauriLibraryApi } from "../src/lib/library";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..", "..");
const text = (rel: string) => readFileSync(join(ROOT, rel), "utf8");
const rustSources = () => text("src-tauri/src/lib.rs") + "\n" + text("src-tauri/src/commands.rs");

/** Command names inside `tauri::generate_handler![ ... ]`. */
function registeredCommands(): string[] {
  const body = /generate_handler!\[([\s\S]*?)\]/.exec(text("src-tauri/src/lib.rs"))?.[1];
  assert.ok(body, "no generate_handler! list found in lib.rs");
  return body.split(",").map((s) => s.trim().replace(/^commands::/, "")).filter(Boolean);
}

/** Split on commas that are not inside <...> or (...), so `State<'_, AppState>` stays one parameter. */
function topLevelSplit(s: string): string[] {
  const out: string[] = [];
  let depth = 0;
  let cur = "";
  for (const ch of s) {
    if (ch === "<" || ch === "(") depth++;
    if (ch === ">" || ch === ")") depth--;
    if (ch === "," && depth === 0) {
      out.push(cur);
      cur = "";
    } else cur += ch;
  }
  if (cur.trim()) out.push(cur);
  return out.map((p) => p.trim()).filter(Boolean);
}

const camel = (snake: string) => snake.replace(/_([a-z])/g, (_, c: string) => c.toUpperCase());

/** The argument names the webview must send for a Rust command: its parameters, camelCased, minus `State`. */
function expectedArgs(command: string): string[] {
  const m = new RegExp(`fn\\s+${command}\\s*\\(([\\s\\S]*?)\\)\\s*(?:->|\\{)`).exec(rustSources());
  assert.ok(m, `no fn ${command} in src-tauri`);
  return topLevelSplit(m[1]!)
    .filter((p) => !/State\s*</.test(p))
    .map((p) => camel(p.split(":")[0]!.trim()))
    .sort();
}

let tauri: ReturnType<typeof installTauri>;
beforeEach(() => {
  resetBrowser();
  tauri = installTauri((cmd) => (cmd === "discord_set_enabled" ? "on" : []));
});
afterEach(() => tauri.remove());

describe("commands the UI sends", () => {
  /** Drive every part of the UI that talks to the backend, and return what it sent. */
  async function everythingTheUiSends() {
    const lib = tauriLibraryApi()!;
    await lib.list();
    await lib.scan();
    await lib.matrix("g");
    await lib.tick("g", "mods", true);
    await lib.play(480);
    await getAppInfo();
    await setPresence(true, getInvoker(), "corner");
    const { host, unmount } = await mount(<TitleBar />);
    await click(one(host, "[aria-label=Minimize]"));
    await click(one(host, "[aria-label='Maximize or restore']"));
    await click(one(host, "[aria-label=Close]"));
    unmount();
    await settle();
    return tauri.calls;
  }

  test("every app command the UI sends is registered in src-tauri", async () => {
    const registered = new Set(registeredCommands());
    const sent = (await everythingTheUiSends()).map((c) => c.cmd).filter((c) => !c.startsWith("plugin:"));
    assert.deepEqual([...new Set(sent)].sort(), ["app_info", "discord_set_enabled", "get_game_matrix", "launch_game_command", "list_games", "scan_games_command", "update_feature_tick"]);
    for (const cmd of new Set(sent)) assert.ok(registered.has(cmd), `${cmd} is sent by the UI but not registered`);
  });

  test("the argument names it sends are exactly the Rust parameters, camelCased", async () => {
    const calls = await everythingTheUiSends();
    for (const call of calls.filter((c) => !c.cmd.startsWith("plugin:"))) {
      assert.deepEqual(Object.keys(call.args).sort(), expectedArgs(call.cmd), call.cmd);
    }
  });

  test("the parser really reads the Rust signatures (guards this test against going blind)", () => {
    assert.deepEqual(expectedArgs("get_game_matrix"), ["gameId"]);
    assert.deepEqual(expectedArgs("update_feature_tick"), ["enabled", "feature", "gameId"]);
    assert.deepEqual(expectedArgs("launch_game_command"), ["appId"]);
    assert.deepEqual(expectedArgs("discord_set_enabled"), ["enabled", "smallImage"]);
    assert.deepEqual(expectedArgs("list_games"), []);
    assert.deepEqual(expectedArgs("app_info"), []);
  });

  test("every window command the title bar sends is allowed by the capability file, and dragging is too", async () => {
    const perms = (JSON.parse(text("src-tauri/capabilities/default.json")) as { permissions: string[] }).permissions;
    const windowCalls = (await everythingTheUiSends()).filter((c) => c.cmd.startsWith("plugin:window|"));
    assert.equal(windowCalls.length, 3);
    for (const { cmd } of windowCalls) {
      const permission = "core:window:allow-" + cmd.slice("plugin:window|".length).replace(/_/g, "-");
      assert.ok(perms.includes(permission), `${cmd} needs ${permission}`);
    }
    assert.ok(perms.includes("core:window:allow-start-dragging"), "the bar is a drag region (data-tauri-drag-region)");
  });
});

describe("the shell's configuration the UI depends on", () => {
  const conf = () => JSON.parse(text("src-tauri/tauri.conf.json")) as {
    identifier: string;
    app: { windows: Array<{ label: string; decorations: boolean; minWidth: number }>; security: { csp: string } };
  };

  test("the window is borderless (the UI draws its own title bar) and the capability targets that window", () => {
    const win = conf().app.windows[0]!;
    assert.equal(win.decorations, false);
    const cap = JSON.parse(text("src-tauri/capabilities/default.json")) as { windows: string[] };
    assert.deepEqual(cap.windows, [win.label]);
  });

  test("the capability grants no file, shell, network or other plugin access", () => {
    const perms = (JSON.parse(text("src-tauri/capabilities/default.json")) as { permissions: string[] }).permissions;
    for (const p of perms) assert.match(p, /^core:(default|window:allow-(minimize|toggle-maximize|close|start-dragging))$/, p);
  });

  test("the content security policy allows the app's own files and blob: pictures/videos, and nothing remote or eval", () => {
    const csp = conf().app.security.csp;
    const directive = (name: string) => new RegExp(`(?:^|;)\\s*${name}\\s+([^;]*)`).exec(csp)?.[1]?.split(/\s+/) ?? [];
    assert.deepEqual(directive("default-src"), ["'self'"]);
    for (const name of ["img-src", "media-src"]) assert.ok(directive(name).includes("blob:"), `${name} needs blob: for the custom background`);
    assert.ok(!/https?:|\*|unsafe-eval/.test(csp), `CSP is too open: ${csp}`);
    assert.ok(!directive("script-src").includes("'unsafe-inline'"));
  });

  test("the app identifier is the one the owner chose to keep", () => {
    assert.equal(conf().identifier, "app.soups.unifiedlauncher");
  });
});
