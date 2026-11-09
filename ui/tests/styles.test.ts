// Layout rules that a DOM simulation cannot measure. These only pin the text of the rule; what the rule does
// was measured in a real browser (see PROJECT_STATUS.md): in a 500 px high window the Settings tab strip was
// 2 px high without it and 41 px high with it.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

const css = readFileSync(join(dirname(fileURLToPath(import.meta.url)), "..", "src", "styles.css"), "utf8");

function rule(selector: string): string {
  const start = css.indexOf(`${selector} {`);
  assert.notEqual(start, -1, `no rule for ${selector}`);
  return css.slice(start, css.indexOf("}", start));
}

test("nothing in the scrolling content column may shrink, or a tall page squeezes the tab strip to nothing", () => {
  // `.content` is a column of fixed height. A block with `overflow` set (the `.seg` tab strip) has a minimum
  // height of zero as a flex item, so without this it is shrunk to a sliver when the page is taller than the window.
  assert.match(rule(".content > :not(.scrim)"), /flex-shrink:\s*0/);
  assert.match(rule(".content"), /flex-direction:\s*column/);
  assert.match(rule(".content"), /overflow-y:\s*auto/);
});

test("the measurement table's cells wrap and the density strip wraps, so neither pushes the page sideways", () => {
  // Measured in a real browser at 320, 360, 420, 520, 700 and 1100 px wide: with these rules the table stays inside
  // its card and the page has no horizontal scroll. Without the cell rule the content column overflowed by 8 px at 420 px.
  assert.match(rule(".telemetry th, .telemetry td"), /overflow-wrap:\s*anywhere/);
  assert.match(rule(".seg.wrap"), /flex-wrap:\s*wrap/);
});


test("a long preview scrolls inside its dialog so the buttons stay on screen, and the muted text stays readable on the pop-up", () => {
  // Measured in a real browser, 1280 x 600 window, a 205-file run preview: without these rules the dialog was 9786 px
  // tall, could not be scrolled, and its Cancel and Build buttons were at y = 9739, out of reach.
  const dialog = rule(".dialog");
  assert.match(dialog, /display:\s*flex/);
  assert.match(dialog, /flex-direction:\s*column/);
  assert.match(dialog, /max-height:\s*calc\(100vh - 32px\)/);
  assert.match(rule(".dialog > h3 + div"), /overflow-y:\s*auto/);
  assert.match(rule(".dialog > h3 + div"), /min-height:\s*0/);
  assert.match(rule(".dialog-actions"), /flex-shrink:\s*0/);
  // The page's muted green (#2c4a27) on the green pop-up (#27b014) is about 3.5:1; the pop-up's own text colour is about 7:1.
  assert.match(rule(".dialog .muted"), /color:\s*var\(--popup-text\)/);
});

test("the steps of a saved recipe are a plain numbered list without the dividers of the list around them", () => {
  assert.match(rule(".trust-list .steps li"), /border-bottom:\s*0/);
});
