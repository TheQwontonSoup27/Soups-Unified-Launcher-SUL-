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
