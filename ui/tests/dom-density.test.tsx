// Display density, clicked through: the Settings card, a module's own strip on the Performance Overview, the
// inline "Details" panel, and the fact that the two doors edit one setting. Nothing here is measured yet, so a
// test also checks that no figure appears anywhere.
import { absent, click, mount, one, resetBrowser, settle, stubFetch, waitFor, withText } from "./dom";
import assert from "node:assert/strict";
import { afterEach, beforeEach, describe, test } from "node:test";
import { App } from "../src/App";
import { DensityCard } from "../src/components/DensityCard";
import { DensityProvider } from "../src/densityContext";
import DiagnosticsView from "../src/views/DiagnosticsView";

const ENGINE = "clipping-engine";
const KEY = "sul.density";

let undo: Array<() => void> = [];
beforeEach(() => {
  resetBrowser();
  undo.push(stubFetch([]));
});
afterEach(() => {
  for (const f of undo.reverse()) f();
  undo = [];
});

const saved = () => JSON.parse(localStorage.getItem(KEY) ?? "null");

/** Both doors to the one setting, under one provider. */
async function both() {
  const m = await mount(
    <DensityProvider>
      <DensityCard />
      <DiagnosticsView />
    </DensityProvider>,
  );
  await settle();
  return m;
}

const globalChoice = (host: HTMLElement, label: string) => withText<HTMLButtonElement>(one(host, ".card[aria-labelledby=density-title]"), "button[role=radio]", label);
const moduleChoice = (host: HTMLElement, label: string) =>
  one<HTMLButtonElement>(host, `button[role=radio][aria-label='${label} for Media & Clipping Engine']`);
const area = (host: HTMLElement) => one(host, `.perf-area[data-module=${ENGINE}]`);
const panel = (host: HTMLElement) => area(host).querySelector("[role=region]");
const checked = (el: Element) => el.getAttribute("aria-checked");
const detailsButton = (host: HTMLElement) => one<HTMLButtonElement>(area(host), ".perf-row button");
const now = (host: HTMLElement) => one(area(host), ".density-row .muted").textContent;

describe("the Display density card", () => {
  test("it starts on Compact for all modules with no overrides", async () => {
    const { host, unmount } = await both();
    const card = one(host, ".card[aria-labelledby=density-title]");
    assert.equal(one(card, "h3").textContent, "Display density");
    assert.equal(one(card, "[role=radiogroup]").getAttribute("aria-labelledby"), "density-global-label");
    assert.equal(checked(globalChoice(host, "Compact")), "true");
    assert.equal(checked(globalChoice(host, "Detailed")), "false");
    assert.match(card.textContent ?? "", /No module overrides: every module follows this setting\./);
    absent(card.querySelector(".overrides"), "no override list yet");
    absent(card.querySelector(".btn"), "nothing to clear yet");
    unmount();
  });

  test("choosing Detailed is saved and read back by the next start", async () => {
    const first = await both();
    await click(globalChoice(first.host, "Detailed"));
    assert.equal(checked(globalChoice(first.host, "Detailed")), "true");
    assert.equal(checked(globalChoice(first.host, "Compact")), "false");
    assert.deepEqual(saved(), { global: "detailed", overrides: {} });
    first.unmount();

    const second = await both();
    assert.equal(checked(globalChoice(second.host, "Detailed")), "true");
    second.unmount();
  });

  test("it shows the Settings > Appearance page in the real app, below Background", async () => {
    const { host, unmount } = await mount(<App splashMs={0} />);
    await click(withText(host, "button.nav-item", "Settings"));
    const titles = [...host.querySelectorAll(".card h3")].map((h) => h.textContent);
    assert.ok(titles.includes("Display density"), titles.join(", "));
    assert.ok(titles.indexOf("Display density") > titles.indexOf("Background"), "after the Background card");
    assert.ok(titles.indexOf("Display density") === titles.length - 1, "the last card, as in the wireframe");
    unmount();
  });
});

describe("in the whole app", () => {
  test("a choice made in Settings is what the Diagnostics page shows, and the other way round", async () => {
    const { host, unmount } = await mount(<App splashMs={0} />);
    await click(withText(host, "button.nav-item", "Settings"));
    await click(globalChoice(host, "Detailed"));

    await click(withText(host, "button.nav-item", "Diagnostics & Performance"));
    assert.ok(panel(host), "the panel is open because all modules are Detailed");
    assert.equal(now(host), "now: Detailed (from Global)");
    await click(moduleChoice(host, "Always Compact"));
    absent(panel(host), "this module now overrides to Compact");

    await click(withText(host, "button.nav-item", "Settings"));
    const card = one(host, ".card[aria-labelledby=density-title]");
    assert.equal(checked(globalChoice(host, "Detailed")), "true");
    assert.deepEqual([...card.querySelectorAll(".overrides li")].map((li) => li.textContent), ["Media & Clipping Engine: Always Compact"]);
    assert.deepEqual(saved(), { global: "detailed", overrides: { [ENGINE]: "compact" } });
    unmount();
  });
});

describe("the Performance Overview", () => {
  test("the Diagnostics page opens on its placeholder, then the overview with one section", async () => {
    const { host, unmount } = await mount(<App splashMs={0} />);
    await click(withText(host, "button.nav-item", "Diagnostics & Performance"));
    assert.ok(withText(host, "h1", "Diagnostics & Performance"), "the module's own header is still there");
    assert.match(host.textContent ?? "", /Nothing in this module is implemented yet/);
    assert.ok(withText(host, "h2", "Performance Overview"));
    const areas = host.querySelectorAll(".perf-area");
    assert.equal(areas.length, 1);
    assert.equal(one(areas[0]!, "h3").textContent, "Media & Clipping Engine");
    assert.match(areas[0]!.querySelector(".badge")?.textContent ?? "", /Not investigated/);
    unmount();
  });

  test("Compact: one line with no figures and a Details button; the panel is not there", async () => {
    const { host, unmount } = await both();
    const line = one(area(host), ".perf-row");
    assert.match(line.textContent ?? "", /Engine total · RAM — \| CPU —/);
    assert.equal(one(line, ".tag").getAttribute("aria-label"), "Engine total: RAM not measured, CPU not measured");
    assert.equal(detailsButton(host).textContent, "Details →");
    assert.equal(detailsButton(host).getAttribute("aria-expanded"), "false");
    assert.equal(detailsButton(host).hasAttribute("aria-controls"), false);
    absent(panel(host), "the full panel is closed");
    absent(area(host).querySelector("table"), "no table while compact and closed");
    unmount();
  });

  test("Details opens the full panel in place and Hide details closes it", async () => {
    const opened: unknown[] = [];
    const realOpen = window.open;
    window.open = ((...args: unknown[]) => void opened.push(args)) as unknown as typeof window.open;
    const { host, unmount } = await both();
    await click(detailsButton(host));
    const open = one(area(host), "[role=region]");
    assert.equal(open.getAttribute("aria-label"), "Media & Clipping Engine, engine total: details");
    assert.equal(detailsButton(host).textContent, "Hide details");
    assert.equal(detailsButton(host).getAttribute("aria-expanded"), "true");
    assert.equal(detailsButton(host).getAttribute("aria-controls"), open.id);
    assert.ok(open.id.length > 0 && host.querySelectorAll(`[id='${CSS.escape(open.id)}']`).length === 1, "one element has that id");
    assert.ok(area(host).contains(open), "inline: it opens inside the section, not in a new window");
    assert.equal(opened.length, 0, "no window was opened");

    await click(detailsButton(host));
    absent(panel(host), "the panel is closed again");
    assert.equal(detailsButton(host).textContent, "Details →");
    assert.equal(detailsButton(host).getAttribute("aria-expanded"), "false");
    window.open = realOpen;
    unmount();
  });

  test("the panel has the rows the finished one will fill, and every figure is a dash", async () => {
    const { host, unmount } = await both();
    await click(detailsButton(host));
    const table = one(area(host), "table.telemetry");
    assert.deepEqual([...table.querySelectorAll("thead th")].map((th) => th.textContent), ["Measure", "Predicted spec", "Live (10 s average)", "Difference"]);
    assert.deepEqual([...table.querySelectorAll("tbody th")].map((th) => th.textContent), [
      "RAM (this subsystem)", "CPU (this subsystem)", "Frame-time variance", "Active threads", "Kernel interrupts",
    ]);
    for (const th of table.querySelectorAll("thead th")) assert.equal(th.getAttribute("scope"), "col");
    for (const th of table.querySelectorAll("tbody th")) assert.equal(th.getAttribute("scope"), "row");
    const rows = [...table.querySelectorAll("tbody tr")].map((tr) => [...tr.querySelectorAll("td")].map((td) => td.textContent));
    assert.deepEqual(rows[0], ["—", "—", "—"]);
    assert.deepEqual(rows[1], ["—", "—", "—"]);
    assert.deepEqual(rows[2], ["—", "not available", "—"]);
    assert.match(rows[4]![0] ?? "", /system-wide only, so they cannot be attributed to SUL/);
    assert.match(one(area(host), ".telemetry-panel").textContent ?? "", /Nothing is measured yet: the sampler is not built, so no figure is shown\./);
    assert.match(one(area(host), ".telemetry-panel h4 .badge").textContent ?? "", /Not investigated/);
    unmount();
  });

  test("no number is ever shown as a measurement, in either view", async () => {
    const { host, unmount } = await both();
    const figure = /\d+(\.\d+)?\s*(MB|KB|GB|%|\/s|ms)\b/i;
    assert.doesNotMatch(host.textContent ?? "", figure);
    await click(detailsButton(host));
    assert.doesNotMatch(host.textContent ?? "", figure);
    await click(globalChoice(host, "Detailed"));
    assert.doesNotMatch(host.textContent ?? "", figure);
    unmount();
  });
});

describe("the two doors to one setting", () => {
  test("Detailed for all modules shows the panel straight away, with no Details button", async () => {
    const { host, unmount } = await both();
    await click(globalChoice(host, "Detailed"));
    assert.ok(panel(host), "panel is open");
    absent(area(host).querySelector(".perf-row"), "nothing to open when it is already open");
    assert.equal(now(host), "now: Detailed (from Global)");
    assert.equal(checked(moduleChoice(host, "Inherit Global")), "true");
    await click(globalChoice(host, "Compact"));
    absent(panel(host), "back to compact and closed");
    assert.ok(area(host).querySelector(".perf-row"));
    unmount();
  });

  test("Always Detailed on the module wins over a Compact global and is listed on the card", async () => {
    const { host, unmount } = await both();
    assert.equal(now(host), "now: Compact (from Global)");
    await click(moduleChoice(host, "Always Detailed"));
    assert.equal(checked(moduleChoice(host, "Always Detailed")), "true");
    assert.equal(checked(moduleChoice(host, "Inherit Global")), "false");
    assert.equal(now(host), "now: Detailed (set for this module)");
    assert.ok(panel(host), "the panel is open for this module");
    const card = one(host, ".card[aria-labelledby=density-title]");
    assert.deepEqual([...card.querySelectorAll(".overrides li")].map((li) => li.textContent), ["Media & Clipping Engine: Always Detailed"]);
    assert.equal(checked(globalChoice(host, "Compact")), "true", "the global setting did not move");
    assert.deepEqual(saved(), { global: "compact", overrides: { [ENGINE]: "detailed" } });
    unmount();
  });

  test("Always Compact holds even when the global view becomes Detailed", async () => {
    const { host, unmount } = await both();
    await click(moduleChoice(host, "Always Compact"));
    await click(globalChoice(host, "Detailed"));
    assert.equal(now(host), "now: Compact (set for this module)");
    absent(panel(host), "this module stays compact");
    assert.deepEqual(saved(), { global: "detailed", overrides: { [ENGINE]: "compact" } });
    unmount();
  });

  test("Inherit Global removes the override, and so does Clear overrides on the card", async () => {
    const { host, unmount } = await both();
    await click(moduleChoice(host, "Always Detailed"));
    await click(moduleChoice(host, "Inherit Global"));
    assert.deepEqual(saved(), { global: "compact", overrides: {} });
    absent(host.querySelector(".overrides"), "no override listed any more");

    await click(moduleChoice(host, "Always Detailed"));
    await click(withText(host, "button.btn", "Clear overrides"));
    assert.deepEqual(saved(), { global: "compact", overrides: {} });
    assert.equal(checked(moduleChoice(host, "Inherit Global")), "true");
    assert.equal(now(host), "now: Compact (from Global)");
    assert.match(one(host, ".card[aria-labelledby=density-title]").textContent ?? "", /No module overrides/);
    unmount();
  });

  test("an override survives a restart", async () => {
    const first = await both();
    await click(moduleChoice(first.host, "Always Detailed"));
    first.unmount();
    const second = await both();
    assert.equal(now(second.host), "now: Detailed (set for this module)");
    assert.ok(panel(second.host));
    second.unmount();
  });

  test("damaged saved settings do not stop the page from opening", async () => {
    localStorage.setItem(KEY, "{not json");
    const { host, unmount } = await both();
    assert.equal(now(host), "now: Compact (from Global)");
    assert.equal(checked(globalChoice(host, "Compact")), "true");
    unmount();
  });

  test("the strip is a labelled radio group that says what is on now, politely", async () => {
    const { host, unmount } = await both();
    const group = one(area(host), "[role=radiogroup]");
    const label = one(area(host), `#${CSS.escape(group.getAttribute("aria-labelledby") ?? "")}`);
    assert.equal(label.textContent, "View");
    assert.equal(group.querySelectorAll("button[role=radio]").length, 3);
    const status = one(area(host), `#${CSS.escape(group.getAttribute("aria-describedby") ?? "")}`);
    assert.equal(status.getAttribute("aria-live"), "polite");
    await waitFor(() => status.textContent === "now: Compact (from Global)", "the status text");
    unmount();
  });
});
