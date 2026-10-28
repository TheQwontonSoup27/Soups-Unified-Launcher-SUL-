import { click, key, mount, one, resetBrowser, settle, stubFetch, stubObjectUrls, type, withText, act } from "./dom";
import assert from "node:assert/strict";
import { File as NodeFile } from "node:buffer";
import { afterEach, beforeEach, describe, test } from "node:test";
import { Dialog } from "../src/components/Dialog";
import { loadCustomBg } from "../src/background";
import { PresenceProvider } from "../src/presence";
import { SettingsProvider } from "../src/settings";
import { THEMES, TOKEN_KEYS } from "../src/theme";
import SettingsView from "../src/views/SettingsView";

const WALLPAPERS = [
  { id: "w01", name: "Dusk shoreline", file: "wallpapers/w01.jpg", thumb: "wallpapers/thumbs/w01.jpg", lowRes: false },
  { id: "w02", name: "Small one", file: "wallpapers/w02.jpg", thumb: "wallpapers/thumbs/w02.jpg", lowRes: true },
];

let undo: Array<() => void> = [];
beforeEach(() => {
  resetBrowser();
  undo.push(stubFetch(WALLPAPERS));
});
afterEach(() => {
  for (const f of undo.reverse()) f();
  undo = [];
});

async function settings() {
  const m = await mount(
    <SettingsProvider>
      <PresenceProvider>
        <SettingsView />
      </PresenceProvider>
    </SettingsProvider>,
  );
  await settle();
  return m;
}

const tab = (host: HTMLElement, label: string) => withText<HTMLButtonElement>(host, "[role=tab]", label);
const bgChoice = (host: HTMLElement, label: string) => withText<HTMLButtonElement>(host, "[aria-label='Background type'] button", label);
const stored = (k: string) => JSON.parse(localStorage.getItem(k) ?? "null");

describe("Settings tabs", () => {
  test("it opens on Appearance, and each tab shows its own pane", async () => {
    const { host, unmount } = await settings();
    const tabs = [...host.querySelectorAll("[role=tab]")].map((t) => t.textContent);
    assert.deepEqual(tabs, ["Appearance", "Discord", "Security & Trust"]);
    assert.equal(one(host, "[role=tablist]").getAttribute("aria-label"), "Settings sections");
    assert.equal(tab(host, "Appearance").getAttribute("aria-selected"), "true");
    assert.ok(withText(host, "h3", "Theme"));

    await click(tab(host, "Discord"));
    assert.equal(tab(host, "Discord").getAttribute("aria-selected"), "true");
    assert.equal(tab(host, "Appearance").getAttribute("aria-selected"), "false");
    assert.ok(withText(host, "h3", "Discord Rich Presence"));
    assert.equal(host.querySelector(".theme-grid"), null, "the other pane is gone, not just hidden");

    await click(tab(host, "Security & Trust"));
    const pane = one(host, "[role=tabpanel]");
    assert.match(pane.textContent ?? "", /Not investigated/);
    assert.match(pane.textContent ?? "", /Nothing in this module is implemented yet/);
    assert.equal(pane.querySelector("h1"), null, "embedded: the page header is not repeated");

    await click(tab(host, "Appearance"));
    assert.ok(withText(host, "h3", "Theme"));
    unmount();
  });

  test("the page header names the three sections", async () => {
    const { host, unmount } = await settings();
    assert.equal(one(host, "h1").textContent, "Settings");
    assert.match(one(host, ".page-header p").textContent ?? "", /Appearance, Discord presence, and Security & Trust/);
    unmount();
  });
});

describe("Appearance: theme", () => {
  test("every theme is offered, the first is selected, and choosing one applies and keeps it", async () => {
    const { host, unmount } = await settings();
    const radios = [...host.querySelectorAll("[aria-label=Theme] [role=radio]")];
    assert.deepEqual(radios.map((r) => r.textContent), THEMES.map((t) => t.label));
    assert.equal(radios[0]!.getAttribute("aria-checked"), "true");
    assert.equal(document.documentElement.dataset.theme, THEMES[0]!.id);

    const target = THEMES[1]!;
    await click(withText(host, "[role=radio]", target.label));
    assert.equal(withText(host, "[role=radio]", target.label).getAttribute("aria-checked"), "true");
    assert.equal(radios[0]!.getAttribute("aria-checked"), "false");
    assert.equal(document.documentElement.dataset.theme, target.id);
    assert.equal(document.documentElement.style.getPropertyValue("--bg"), target.bg);
    assert.equal(document.documentElement.style.getPropertyValue("--titlebar-text"), target.titlebarText, "camelCase tokens become kebab-case variables");
    assert.equal(localStorage.getItem("sul.theme"), target.id);
    unmount();
  });

  test("a saved theme is used at start", async () => {
    localStorage.setItem("sul.theme", THEMES[3]!.id);
    const { host, unmount } = await settings();
    assert.equal(withText(host, "[role=radio]", THEMES[3]!.label).getAttribute("aria-checked"), "true");
    assert.equal(document.documentElement.dataset.theme, THEMES[3]!.id);
    unmount();
  });
});

describe("Appearance: colours", () => {
  test("every token has a colour picker with a readable name and its current value", async () => {
    const { host, unmount } = await settings();
    const pickers = host.querySelectorAll<HTMLInputElement>(".color-grid input[type=color]");
    assert.equal(pickers.length, TOKEN_KEYS.length);
    assert.equal(TOKEN_KEYS.length, 15);
    assert.equal(one<HTMLInputElement>(host, "input[aria-label='Title bar']").value, THEMES[0]!.titlebar);
    assert.ok([...pickers].every((p) => p.getAttribute("aria-label")), "no unlabelled picker");
    unmount();
  });

  test("changing a colour applies it at once, shows its value, and keeps it on top of the theme", async () => {
    const { host, unmount } = await settings();
    await type(one<HTMLInputElement>(host, "input[aria-label='Accent / focus ring']"), "#ff0000");
    assert.equal(document.documentElement.style.getPropertyValue("--accent"), "#ff0000");
    assert.deepEqual(stored("sul.colors"), { accent: "#ff0000" });
    assert.ok(withText(host, "code", "#ff0000"));

    // Switching theme keeps the edit; it is applied over whichever theme is selected.
    await click(withText(host, "[role=radio]", THEMES[1]!.label));
    assert.equal(document.documentElement.style.getPropertyValue("--accent"), "#ff0000");
    assert.equal(document.documentElement.style.getPropertyValue("--bg"), THEMES[1]!.bg);
    unmount();
  });

  test("Reset colours puts the theme's own colours back and forgets the edits", async () => {
    const { host, unmount } = await settings();
    await type(one<HTMLInputElement>(host, "input[aria-label='Accent / focus ring']"), "#ff0000");
    await click(withText(host, "button", "Reset colours to theme"));
    assert.equal(document.documentElement.style.getPropertyValue("--accent"), THEMES[0]!.accent);
    assert.deepEqual(stored("sul.colors"), {});
    unmount();
  });

  test("the readability check passes for the default theme and names each failing pair otherwise", async () => {
    const { host, unmount } = await settings();
    assert.match(one(host, ".contrast").textContent ?? "", /All text\/background pairs meet the 4.5:1/);
    assert.equal(host.querySelectorAll(".contrast li").length, 0);

    // Text the same colour as the cards it sits on.
    await type(one<HTMLInputElement>(host, "input[aria-label='Text']"), THEMES[0]!.panel);
    const failing = [...host.querySelectorAll(".contrast li")].map((li) => li.textContent ?? "");
    assert.ok(failing.length >= 1);
    assert.ok(failing.some((f) => /Text on cards/.test(f) && /1\.0:1 is hard to read/.test(f)), failing.join(" | "));
    assert.equal(one(host, ".contrast").getAttribute("role"), "status", "announced to screen readers");
    unmount();
  });
});

describe("Appearance: background", () => {
  test("it starts as a solid colour, My file is unavailable, and the gallery lists what is installed", async () => {
    const { host, unmount } = await settings();
    assert.equal(bgChoice(host, "Solid colour").getAttribute("aria-checked"), "true");
    assert.equal(bgChoice(host, "My file").disabled, true);
    const tiles = [...host.querySelectorAll(".wall")];
    assert.equal(tiles.length, 2);
    assert.match(tiles[0]!.textContent ?? "", /Dusk shoreline/);
    assert.match(tiles[1]!.textContent ?? "", /Small one • low-res/);
    assert.equal(tiles[1]!.getAttribute("title"), "Small one (low resolution)");
    unmount();
  });

  test("with no gallery images it says where to put them", async () => {
    undo.push(stubFetch([]));
    const { host, unmount } = await settings();
    assert.equal(host.querySelectorAll(".wall").length, 0);
    assert.match(host.textContent ?? "", /No gallery images are installed/);
    unmount();
  });

  test("picking a gallery image selects the gallery and keeps the choice", async () => {
    const { host, unmount } = await settings();
    await click(withText(host, ".wall", "Dusk shoreline"));
    assert.equal(bgChoice(host, "Gallery").getAttribute("aria-checked"), "true");
    assert.ok(withText(host, ".wall", "Dusk shoreline").classList.contains("selected"));
    assert.ok(!withText(host, ".wall", "Small one • low-res").classList.contains("selected"));
    assert.deepEqual(stored("sul.bg"), { mode: "gallery", galleryId: "w01", overlay: 0.35 });

    await click(bgChoice(host, "Solid colour"));
    assert.equal(stored("sul.bg").mode, "solid");
    unmount();
  });

  test("the dimming slider shows its percentage and keeps it", async () => {
    const { host, unmount } = await settings();
    const slider = one<HTMLInputElement>(host, "input[aria-label='Background dimming']");
    assert.equal(slider.value, "35");
    await type(slider, "60");
    assert.match(one(host, "label.row").textContent ?? "", /60%/);
    assert.equal(stored("sul.bg").overlay, 0.6);
    unmount();
  });
});

describe("Appearance: your own picture or video", () => {
  const urls = { current: null as ReturnType<typeof stubObjectUrls> | null };
  beforeEach(() => {
    urls.current = stubObjectUrls();
  });
  afterEach(() => urls.current?.restore());

  /** Hand the hidden file input a file, as the browser does after the user picks one. */
  async function choose(host: HTMLElement, file: unknown) {
    const input = one<HTMLInputElement>(host, "input[type=file]");
    Object.defineProperty(input, "files", { configurable: true, value: [file] });
    await act(async () => {
      input.dispatchEvent(new window.Event("change", { bubbles: true }));
    });
    await settle();
  }

  test("the file picker offers only the supported kinds", async () => {
    const { host, unmount } = await settings();
    assert.equal(one<HTMLInputElement>(host, "input[type=file]").accept, "image/png,image/jpeg,image/webp,image/gif,video/mp4,video/webm");
    unmount();
  });

  test("a file of another kind is refused with a reason and nothing changes", async () => {
    const { host, unmount } = await settings();
    await choose(host, new NodeFile(["%PDF"], "notes.pdf", { type: "application/pdf" }));
    assert.match(one(host, "p[role=status]").textContent ?? "", /Unsupported file type\. Use PNG, JPG, WebP, GIF, MP4 or WebM\./);
    assert.equal(bgChoice(host, "My file").disabled, true);
    assert.equal(await loadCustomBg(), null);
    unmount();
  });

  test("a file over 300 MB is refused", async () => {
    const { host, unmount } = await settings();
    const big = new NodeFile(["x"], "huge.mp4", { type: "video/mp4" });
    Object.defineProperty(big, "size", { value: 300 * 1024 * 1024 + 1 });
    await choose(host, big);
    assert.match(one(host, "p[role=status]").textContent ?? "", /File is larger than 300 MB\./);
    assert.equal(await loadCustomBg(), null);
    unmount();
  });

  test("an accepted picture is stored, selected, and can be removed after a confirmation", async () => {
    const { host, unmount } = await settings();
    await choose(host, new NodeFile(["\x89PNG"], "pic.png", { type: "image/png" }));
    assert.equal(host.querySelector("p[role=status]"), null, "no problem to report");
    assert.equal(bgChoice(host, "My file").getAttribute("aria-checked"), "true");
    assert.equal(stored("sul.bg").mode, "custom");
    assert.equal((await loadCustomBg())?.name, "pic.png");

    // Asking to remove it only asks; the file stays until the user says yes.
    await click(withText(host, "button", "Remove “pic.png”"));
    const dialog = one(document.body, "[role=dialog]");
    assert.equal(dialog.getAttribute("aria-label"), "Remove background file?");
    assert.match(dialog.textContent ?? "", /Your original file is not touched/);
    await click(withText(dialog, "button", "Keep it"));
    assert.equal(document.body.querySelector("[role=dialog]"), null);
    assert.equal((await loadCustomBg())?.name, "pic.png");

    await click(withText(host, "button", "Remove “pic.png”"));
    await click(withText(one(document.body, "[role=dialog]"), "button", "Remove"));
    assert.equal(document.body.querySelector("[role=dialog]"), null);
    assert.equal(await loadCustomBg(), null);
    assert.equal(bgChoice(host, "Solid colour").getAttribute("aria-checked"), "true");
    assert.equal(bgChoice(host, "My file").disabled, true);
    assert.ok(urls.current!.revoked.length >= 1, "the temporary address of the removed file is released");
    unmount();
  });

  test("if the browser will not store the file it is still shown for this session, and says so", async () => {
    const realOpen = indexedDB.open.bind(indexedDB);
    indexedDB.open = (() => {
      throw new Error("storage blocked");
    }) as typeof indexedDB.open;
    try {
      const { host, unmount } = await settings();
      await choose(host, new NodeFile(["\x89PNG"], "pic.png", { type: "image/png" }));
      assert.match(one(host, "p[role=status]").textContent ?? "", /Shown for this session only/);
      assert.equal(bgChoice(host, "My file").getAttribute("aria-checked"), "true");
      unmount();
    } finally {
      indexedDB.open = realOpen;
    }
  });
});

describe("Dialog", () => {
  function Host({ onClose, onAction }: { onClose: () => void; onAction: () => void }) {
    return (
      <Dialog title="Sure?" onClose={onClose} actions={<><button onClick={onAction}>Yes</button><button onClick={onClose}>No</button></>}>
        <p>Body text</p>
      </Dialog>
    );
  }

  test("it is a labelled modal dialog and focuses its first button", async () => {
    const { host, unmount } = await mount(<Host onClose={() => {}} onAction={() => {}} />);
    const dialog = one(host, "[role=dialog]");
    assert.equal(dialog.getAttribute("aria-modal"), "true");
    assert.equal(dialog.getAttribute("aria-label"), "Sure?");
    assert.equal(document.activeElement, withText(dialog, "button", "Yes"));
    unmount();
  });

  test("Escape and a click outside close it; a click inside does not", async () => {
    let closed = 0;
    const { host, unmount } = await mount(<Host onClose={() => closed++} onAction={() => {}} />);
    await click(one(host, ".dialog"));
    assert.equal(closed, 0, "a click inside is not a click outside");
    await key("a");
    assert.equal(closed, 0);
    await key("Escape");
    assert.equal(closed, 1);
    await click(one(host, ".scrim"));
    assert.equal(closed, 2);
    unmount();
  });

  test("it stops listening for Escape once it is gone", async () => {
    let closed = 0;
    const { unmount } = await mount(<Host onClose={() => closed++} onAction={() => {}} />);
    unmount();
    await key("Escape");
    assert.equal(closed, 0);
  });
});
