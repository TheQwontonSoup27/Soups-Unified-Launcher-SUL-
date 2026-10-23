import { test } from "node:test";
import assert from "node:assert/strict";
import { renderToString } from "react-dom/server";
import { App } from "../src/App";
import { MODULES, renderModule } from "../src/modules/registry";

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

test("shell renders overview and every module nav entry", () => {
  const html = renderToString(<App splashMs={0} />);
  assert.match(html, /Soups Unified Launcher/);
  assert.match(html, /Keep every game version under control/);
  for (const m of MODULES) assert.ok(html.includes(m.title.replace(/&/g, "&amp;")), `nav missing ${m.title}`);
});

test("every module page renders without throwing", () => {
  for (const m of MODULES) assert.ok(renderToString(<>{renderModule(m.id)}</>).length > 0);
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
});

import { setPresence, loadPresenceWanted } from "../src/discord";

test("Discord presence is off by default and the toggle is in the title bar", () => {
  assert.equal(loadPresenceWanted(), false);
  const html = renderToString(<App splashMs={0} />);
  assert.match(html, /Discord presence: Off/);
});

test("setPresence: preview mode, success, and error paths", async () => {
  assert.deepEqual(await setPresence(false, null), { on: false, message: "" });
  assert.equal((await setPresence(true, null)).on, true);
  assert.match((await setPresence(true, null)).message, /Preview only/);
  assert.deepEqual(await setPresence(true, async () => "on"), { on: true, message: "" });
  assert.deepEqual(await setPresence(false, async () => "off"), { on: false, message: "" });
  const failed = await setPresence(true, async () => { throw "Discord does not appear to be running"; });
  assert.deepEqual(failed, { on: false, message: "Discord does not appear to be running" });
  const weird = await setPresence(true, async () => { throw new Error("x"); });
  assert.equal(weird.on, false);
});
