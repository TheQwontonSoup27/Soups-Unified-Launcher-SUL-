import { test } from "node:test";
import assert from "node:assert/strict";
import { THEMES, TOKEN_KEYS, checkPairs, contrast, effectiveTheme } from "../src/theme";

test("contrast helper matches known values", () => {
  assert.ok(Math.abs(contrast("#000000", "#ffffff") - 21) < 0.01);
  assert.ok(Math.abs(contrast("#777777", "#777777") - 1) < 0.01);
});

for (const t of THEMES) {
  test(`theme ${t.id} meets WCAG AA (4.5:1) for every text/background pair`, () => {
    for (const r of checkPairs(t)) assert.ok(r.ok, `${r.label}: ${r.ratio.toFixed(2)}:1`);
  });
  test(`theme ${t.id} defines every token as #rrggbb`, () => {
    for (const k of TOKEN_KEYS) assert.match(t[k], /^#[0-9a-f]{6}$/i, k);
  });
}

test("theme ids are unique and Team27 is the default", () => {
  assert.equal(new Set(THEMES.map((t) => t.id)).size, THEMES.length);
  assert.equal(THEMES[0]!.id, "team27");
});

test("Team27 theme follows the requested design", () => {
  const t = THEMES[0]!;
  assert.equal(t.bg, "#1ea40c");        // green app background
  assert.equal(t.titlebar, "#4d10b6");  // purple title bar
  assert.equal(t.buttonText, "#000000"); // black button text
  assert.equal(t.popup.toLowerCase().slice(1, 2), "2"); // green pop-ups (green channel dominant checked below)
  const n = parseInt(t.popup.slice(1), 16);
  assert.ok(((n >> 8) & 255) > ((n >> 16) & 255) && ((n >> 8) & 255) > (n & 255), "popup is green");
});

test("overrides apply on top of a theme and unknown ids fall back", () => {
  const e = effectiveTheme("team27", { button: "#123456" });
  assert.equal(e.button, "#123456");
  assert.equal(e.bg, "#1ea40c");
  assert.equal(effectiveTheme("nope", {}).id, "team27");
});
