import { test } from "node:test";
import assert from "node:assert/strict";
import { renderToString } from "react-dom/server";
import { App } from "../src/App";
import { MODULES, SIDEBAR_MODULES, renderModule } from "../src/modules/registry";
import { BOTTOM_NAV, MODULE_NAV, TOP_NAV } from "../src/nav";

const esc = (s: string) => s.replace(/&/g, "&amp;");

test("registry has the 10 spec modules with unique ids", () => {
  assert.equal(MODULES.length, 10);
  assert.equal(new Set(MODULES.map((m) => m.id)).size, 10);
});

test("only Appearance claims any support, and only partially", () => {
  for (const m of MODULES) {
    if (m.id === "appearance") assert.equal(m.status, "partial");
    else assert.equal(m.status, "not_investigated");
  }
});

test("every module has exactly one home in the shell", () => {
  const homes = new Set(MODULES.map((m) => m.home));
  assert.deepEqual([...homes].sort(), ["library", "settings", "sidebar"]);
  assert.deepEqual(MODULES.filter((m) => m.home === "settings").map((m) => m.id).sort(), ["appearance", "security"]);
  assert.deepEqual(MODULES.filter((m) => m.home === "library").map((m) => m.id), ["library"]);
  assert.equal(SIDEBAR_MODULES.length, 7);
  assert.equal(MODULE_NAV.length, SIDEBAR_MODULES.length);
});

test("sidebar order follows the Step 1 spec: Game Library first, Settings then Playtime Tracker last", () => {
  assert.deepEqual(TOP_NAV.map((n) => n.label), ["Game Library"]);
  assert.deepEqual(BOTTOM_NAV.map((n) => n.label), ["Settings", "General Playtime Tracker"]);
  const html = renderToString(<App splashMs={0} />);
  const order = ["Game Library", ...MODULE_NAV.map((n) => esc(n.label)), "Settings", "General Playtime Tracker"].map((label) => html.indexOf(`>${label}</button>`));
  assert.ok(order.every((i) => i >= 0), `a nav entry is missing: ${order}`);
  assert.deepEqual(order, [...order].sort((a, b) => a - b), "nav entries are out of order");
});

test("shell renders the Game Library page with the status legend", () => {
  const html = renderToString(<App splashMs={0} />);
  assert.match(html, /Soups Unified Launcher/);
  assert.match(html, /Keep every game version under control/);
  assert.match(html, /Supported and tested/);
  assert.match(html, /Not investigated/);
  assert.match(html, /no Tauri backend connected/);
});

test("every sidebar module page renders without throwing and says it is a placeholder", () => {
  for (const m of SIDEBAR_MODULES) {
    const html = renderToString(<>{renderModule(m.id)}</>);
    assert.ok(html.includes(esc(m.title)), m.title);
    assert.match(html, /Nothing in this module is implemented yet/);
  }
  assert.equal(renderModule("nope"), null);
});

test("embedded module pages carry no page header", () => {
  const html = renderToString(<>{renderModule("security", true)}</>);
  assert.ok(!html.includes("<h1>"));
  assert.match(html, /Not investigated/);
});

test("splash shows the logo first when enabled", () => {
  const html = renderToString(<App splashMs={1800} />);
  assert.match(html, /class="splash"/);
  assert.match(html, /logo\.png/);
  assert.ok(!html.includes("Keep every game version"));
});

test("shell shows the title bar with the app icon", () => {
  const html = renderToString(<App splashMs={0} />);
  assert.match(html, /class="titlebar"/);
  assert.match(html, /titlebar-icon/);
  assert.match(html, /data-tauri-drag-region/);
  // A plain browser has no window to control, so no window buttons are drawn.
  assert.ok(!html.includes('aria-label="Close"'));
});

test("Discord presence is off by default and the switch is in the sidebar's top row", () => {
  const html = renderToString(<App splashMs={0} />);
  assert.match(html, /role="switch"[^>]*aria-checked="false"/);
  assert.match(html, /aria-label="Discord presence: Off"/);
  const top = html.slice(html.indexOf("sidebar-top"), html.indexOf("</aside>"));
  const icon = top.indexOf("app-icon");
  const search = top.indexOf("global-search");
  const rpc = top.indexOf("rpc-toggle");
  assert.ok(icon >= 0 && icon < search && search < rpc, "icon, search, switch order");
});
