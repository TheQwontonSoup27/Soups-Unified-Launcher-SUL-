import { test } from "node:test";
import assert from "node:assert/strict";
import { THEMES, contrast } from "../src/theme";

test("contrast helper matches known values", () => {
  assert.ok(Math.abs(contrast("#000000", "#ffffff") - 21) < 0.01);
  assert.ok(Math.abs(contrast("#777777", "#777777") - 1) < 0.01);
});

for (const t of THEMES) {
  test(`theme ${t.id} meets WCAG AA for text, muted text and accent button`, () => {
    assert.ok(contrast(t.text, t.bg) >= 4.5, "text on bg");
    assert.ok(contrast(t.text, t.panel) >= 4.5, "text on panel");
    assert.ok(contrast(t.muted, t.panel) >= 4.5, "muted on panel");
    assert.ok(contrast(t.onAccent, t.accent) >= 4.5, "label on accent button");
  });
}

test("theme ids are unique", () => {
  assert.equal(new Set(THEMES.map((t) => t.id)).size, THEMES.length);
});
