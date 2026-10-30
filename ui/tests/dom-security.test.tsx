// The Security & Trust page, clicked through: a read-only report with a headline that is the weakest label on it,
// two design rules that say how they were checked, and the one live fact (whether Discord presence is on).
import { absent, click, mount, one, resetBrowser, settle, withText } from "./dom";
import assert from "node:assert/strict";
import { beforeEach, describe, test } from "node:test";
import { App } from "../src/App";
import { PresenceToggle } from "../src/components/PresenceToggle";
import { PresenceProvider } from "../src/presence";
import { CHECKED_ON } from "../src/security";
import SettingsView from "../src/views/SettingsView";
import { SecurityPanel } from "../src/views/SecurityPanel";

beforeEach(() => resetBrowser());

const cards = (host: HTMLElement) => [...host.querySelectorAll("section.card")];
const card = (host: HTMLElement, title: string) => cards(host).find((c) => c.querySelector("h3")?.textContent === title)!;
const items = (el: Element) => [...el.querySelectorAll(".trust-list li")];
const row = (host: HTMLElement, title: string) => items(host).find((li) => li.querySelector(".trust-title")?.textContent === title)!;
const badge = (el: Element) => el.querySelector(".badge")?.textContent;

async function panel() {
  const m = await mount(
    <PresenceProvider>
      <PresenceToggle />
      <SecurityPanel />
    </PresenceProvider>,
  );
  await settle();
  return m;
}

describe("the page", () => {
  test("four cards in order, and the headline is the weakest label on the page", async () => {
    const { host, unmount } = await panel();
    assert.deepEqual(cards(host).map((c) => c.querySelector("h3")?.textContent), ["Security & Trust", "Credentials", "Data sharing", "Protections"]);
    const overall = card(host, "Security & Trust");
    assert.equal(badge(overall), "Not investigated");
    assert.match(overall.textContent ?? "", /This page only reports\. It has no switches and changes nothing\./);
    assert.match(overall.textContent ?? "", new RegExp(`5 rows; checked against the code on ${CHECKED_ON}`));
    unmount();
  });

  test("every row carries its capability label", async () => {
    const { host, unmount } = await panel();
    assert.equal(items(host).length, 5);
    assert.equal(badge(row(host, "Discord Rich Presence")), "Experimental");
    assert.equal(badge(row(host, "Automated Social/Discord Webhooks")), "Not investigated");
    for (const t of ["Package scanning", "Quarantine", "Integrity checks"]) assert.equal(badge(row(host, t)), "Not investigated", t);
    for (const li of items(host)) assert.ok(li.querySelector(".badge"), "a label on every row");
    unmount();
  });

  test("the two rules are tagged as design rules, not audits, and say how they were checked and what was not", async () => {
    const { host, unmount } = await panel();
    for (const title of ["Credentials", "Data sharing"]) {
      const c = card(host, title);
      assert.equal(one(c, ".tag").textContent, "Design rule, not an audit");
      absent(c.querySelector("header .badge"), "a rule is not a capability, so its heading has no capability label");
      assert.match(c.textContent ?? "", /How this was checked\./);
      assert.ok((c.textContent ?? "").includes(CHECKED_ON));
    }
    assert.match(card(host, "Credentials").textContent ?? "", /does not store passwords, tokens or API keys/);
    assert.match(card(host, "Data sharing").textContent ?? "", /By default SUL sends nothing off this PC/);
    assert.match(card(host, "Data sharing").textContent ?? "", /network traffic was never captured/);
    unmount();
  });

  test("the webhook is shown as not built, and the three protections as not built", async () => {
    const { host, unmount } = await panel();
    assert.match(row(host, "Automated Social/Discord Webhooks").textContent ?? "", /Not built, so nothing can be posted\./);
    for (const t of ["Package scanning", "Quarantine", "Integrity checks"]) assert.match(row(host, t).textContent ?? "", /Not built\./, t);
    assert.match(row(host, "Integrity checks").textContent ?? "", /suspicious signal will never be reported as a confirmed violation/);
    unmount();
  });

  test("it is read-only: no control, link or field anywhere on the page", async () => {
    const m = await mount(<SecurityPanel />);
    absent(m.host.querySelector("button, input, select, textarea, a, [role=switch], [role=checkbox], [role=radio], [tabindex]"), "nothing to operate");
    m.unmount();
  });

  test("each card is named by its heading", async () => {
    const { host, unmount } = await panel();
    for (const c of cards(host)) {
      const id = c.getAttribute("aria-labelledby") ?? "";
      assert.ok(id && host.querySelector(`[id='${CSS.escape(id)}']`)?.tagName === "H3", c.textContent?.slice(0, 30));
    }
    unmount();
  });

  test("no figure is shown as a measurement", async () => {
    const { host, unmount } = await panel();
    assert.doesNotMatch(host.textContent ?? "", /\d+(\.\d+)?\s*(MB|KB|GB|%|ms)\b/i);
    unmount();
  });
});

describe("the live fact", () => {
  test("it says whether Discord presence is on, and follows the switch", async () => {
    const { host, unmount } = await panel();
    const now = () => row(host, "Discord Rich Presence").textContent ?? "";
    assert.match(now(), /Right now: off\./);
    await click(one(host, "button.rpc-toggle"));
    assert.match(now(), /Right now: on\./);
    await click(one(host, "button.rpc-toggle"));
    assert.match(now(), /Right now: off\./);
    unmount();
  });

  test("nothing else on the page changes with it", async () => {
    const { host, unmount } = await panel();
    const before = cards(host).map((c) => c.textContent).filter((t) => !(t ?? "").includes("Right now"));
    await click(one(host, "button.rpc-toggle"));
    const after = cards(host).map((c) => c.textContent).filter((t) => !(t ?? "").includes("Right now"));
    assert.deepEqual(after, before);
    unmount();
  });
});

describe("in Settings and in the whole app", () => {
  test("the Security & Trust tab shows this page, with no second page header", async () => {
    const { host, unmount } = await mount(
      <PresenceProvider>
        <SettingsView />
      </PresenceProvider>,
    );
    await click(withText(host, "[role=tab]", "Security & Trust"));
    const pane = one(host, "[role=tabpanel]");
    assert.ok(withText(pane, "h3", "Security & Trust"));
    assert.ok(withText(pane, "h3", "Protections"));
    absent(pane.querySelector("h1"), "the page header is not repeated");
    assert.equal(host.querySelectorAll("h1").length, 1, "only Settings' own header");
    unmount();
  });

  test("in the app: Settings, Security & Trust, and presence turned on in the sidebar shows on the page", async () => {
    const { host, unmount } = await mount(<App splashMs={0} />);
    await click(withText(host, "button.nav-item", "Settings"));
    await click(withText(host, "[role=tab]", "Security & Trust"));
    assert.match(one(host, "[role=tabpanel]").textContent ?? "", /Right now: off\./);
    await click(one(host, "button.rpc-toggle"));
    assert.match(one(host, "[role=tabpanel]").textContent ?? "", /Right now: on\./);
    unmount();
  });
});
