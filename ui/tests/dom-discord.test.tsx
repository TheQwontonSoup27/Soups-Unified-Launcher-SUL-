import { click, installTauri, mount, one, resetBrowser, settle, submit, type, withText } from "./dom";
import assert from "node:assert/strict";
import { afterEach, beforeEach, describe, test } from "node:test";
import { PresenceMessage, PresenceToggle } from "../src/components/PresenceToggle";
import { PresenceProvider } from "../src/presence";
import { DiscordPanel } from "../src/views/DiscordPanel";

const WANTED = "sul.discord";
const SMALL = "sul.discord.small";

/** The sidebar switch, the settings panel and the sidebar message, all sharing one provider as in the app. */
function Screen() {
  return (
    <PresenceProvider>
      <PresenceToggle />
      <PresenceMessage />
      <DiscordPanel />
    </PresenceProvider>
  );
}

let tauri: ReturnType<typeof installTauri> | null = null;
beforeEach(() => resetBrowser());
afterEach(() => {
  tauri?.remove();
  tauri = null;
});

const sidebarSwitch = (host: HTMLElement) => one<HTMLButtonElement>(host, "button.rpc-toggle");
const panelSwitch = (host: HTMLElement) => one<HTMLButtonElement>(host, ".card button[role=switch]");

describe("Discord presence, in a plain browser (no desktop app)", () => {
  test("it is off by default and nothing is stored", async () => {
    const { host, unmount } = await mount(<Screen />);
    assert.equal(sidebarSwitch(host).getAttribute("aria-checked"), "false");
    assert.equal(sidebarSwitch(host).getAttribute("aria-label"), "Discord presence: Off");
    assert.ok(!sidebarSwitch(host).classList.contains("on"));
    assert.equal(panelSwitch(host).textContent, "Turn presence on");
    assert.equal(localStorage.getItem(WANTED), null, "opt-in: nothing is saved until the user chooses");
    assert.equal(host.querySelector(".presence-msg"), null);
    unmount();
  });

  test("the sidebar switch and the settings panel are one switch", async () => {
    const { host, unmount } = await mount(<Screen />);
    await click(sidebarSwitch(host));
    assert.equal(sidebarSwitch(host).getAttribute("aria-checked"), "true");
    assert.ok(sidebarSwitch(host).classList.contains("on"));
    assert.equal(panelSwitch(host).getAttribute("aria-checked"), "true");
    assert.equal(panelSwitch(host).textContent, "Turn presence off");
    assert.match(one(host, ".presence-msg").textContent ?? "", /Preview only/);
    assert.equal(localStorage.getItem(WANTED), "1");

    await click(panelSwitch(host));
    assert.equal(sidebarSwitch(host).getAttribute("aria-checked"), "false");
    assert.equal(host.querySelector(".presence-msg"), null, "turning it off clears the message");
    assert.equal(localStorage.getItem(WANTED), "0");
    unmount();
  });

  test("a saved wish to be on is honoured at start", async () => {
    localStorage.setItem(WANTED, "1");
    const { host, unmount } = await mount(<Screen />);
    await settle();
    assert.equal(sidebarSwitch(host).getAttribute("aria-checked"), "true");
    unmount();
  });
});

describe("Discord presence, with the desktop app", () => {
  test("turning it on asks the backend and shows it on only when the backend says so", async () => {
    tauri = installTauri((_cmd, args) => (args.enabled ? "on" : "off"));
    const { host, unmount } = await mount(<Screen />);
    await click(sidebarSwitch(host));
    assert.deepEqual(tauri.calls, [{ cmd: "discord_set_enabled", args: { enabled: true, smallImage: undefined } }]);
    assert.equal(sidebarSwitch(host).getAttribute("aria-checked"), "true");
    assert.equal(localStorage.getItem(WANTED), "1");

    await click(sidebarSwitch(host));
    assert.deepEqual(tauri.calls[1], { cmd: "discord_set_enabled", args: { enabled: false, smallImage: undefined } });
    assert.equal(sidebarSwitch(host).getAttribute("aria-checked"), "false");
    assert.equal(localStorage.getItem(WANTED), "0");
    unmount();
  });

  test("when Discord cannot be reached the switch stays off, the reason is shown, and the wish is not kept", async () => {
    tauri = installTauri(() => {
      throw "Discord is not running.";
    });
    const { host, unmount } = await mount(<Screen />);
    await click(sidebarSwitch(host));
    assert.equal(sidebarSwitch(host).getAttribute("aria-checked"), "false");
    assert.equal(one(host, ".presence-msg").textContent, "Discord is not running.");
    assert.equal(localStorage.getItem(WANTED), "0");
    unmount();
  });

  test("at start, a saved wish that the backend now refuses is switched off and forgotten", async () => {
    localStorage.setItem(WANTED, "1");
    tauri = installTauri(() => {
      throw "Discord application ID not set.";
    });
    const { host, unmount } = await mount(<Screen />);
    await settle();
    assert.equal(sidebarSwitch(host).getAttribute("aria-checked"), "false");
    assert.equal(localStorage.getItem(WANTED), "0", "it does not retry on every launch");
    assert.match(one(host, ".presence-msg").textContent ?? "", /application ID/);
    unmount();
  });
});

describe("the small corner image", () => {
  test("Save is disabled until the key changes, and a saved key is trimmed and stored", async () => {
    const { host, unmount } = await mount(<Screen />);
    const input = one<HTMLInputElement>(host, "input[aria-label='Small image asset key']");
    const save = withText<HTMLButtonElement>(host, "button", "Save");
    assert.equal(save.disabled, true);

    await type(input, "  team27_corner  ");
    assert.equal(save.disabled, false);
    await submit(one<HTMLFormElement>(host, "form"));
    assert.equal(localStorage.getItem(SMALL), "team27_corner");
    assert.equal(save.disabled, true, "saved, so nothing is left to save");
    unmount();
  });

  test("a key that Discord would reject is stored as none", async () => {
    const { host, unmount } = await mount(<Screen />);
    const input = one<HTMLInputElement>(host, "input[aria-label='Small image asset key']");
    await type(input, "bad\u0007key");
    await submit(one<HTMLFormElement>(host, "form"));
    assert.equal(localStorage.getItem(SMALL), "");
    unmount();
  });

  test("a saved key is shown at start, unless hand-edited storage made it too long for Discord", async () => {
    localStorage.setItem(SMALL, "x".repeat(256));
    let m = await mount(<Screen />);
    assert.equal(one<HTMLInputElement>(m.host, "input[aria-label='Small image asset key']").value, "x".repeat(256), "256 is the longest allowed");
    m.unmount();

    localStorage.setItem(SMALL, "x".repeat(257));
    m = await mount(<Screen />);
    assert.equal(one<HTMLInputElement>(m.host, "input[aria-label='Small image asset key']").value, "", "257 is ignored, not cut");
    m.unmount();
  });

  test("the box refuses more than 256 characters", async () => {
    const { host, unmount } = await mount(<Screen />);
    const input = one<HTMLInputElement>(host, "input[aria-label='Small image asset key']");
    assert.equal(input.maxLength, 256);
    unmount();
  });

  test("while presence is on, saving a new key publishes again with it", async () => {
    tauri = installTauri((_cmd, args) => (args.enabled ? "on" : "off"));
    const { host, unmount } = await mount(<Screen />);
    await click(sidebarSwitch(host));
    const input = one<HTMLInputElement>(host, "input[aria-label='Small image asset key']");
    await type(input, "team27_corner");
    await submit(one<HTMLFormElement>(host, "form"));
    assert.deepEqual(tauri.calls.at(-1), { cmd: "discord_set_enabled", args: { enabled: true, smallImage: "team27_corner" } });
    assert.equal(sidebarSwitch(host).getAttribute("aria-checked"), "true");
    unmount();
  });

  test("while presence is off, saving a key does not contact Discord", async () => {
    tauri = installTauri((_cmd, args) => (args.enabled ? "on" : "off"));
    const { host, unmount } = await mount(<Screen />);
    const input = one<HTMLInputElement>(host, "input[aria-label='Small image asset key']");
    await type(input, "team27_corner");
    await submit(one<HTMLFormElement>(host, "form"));
    assert.deepEqual(tauri.calls, []);
    assert.equal(localStorage.getItem(SMALL), "team27_corner");
    unmount();
  });

  test("the panel states the capability honestly and offers no webhook, chat, friends or login", async () => {
    const { host, unmount } = await mount(<Screen />);
    const text = host.textContent ?? "";
    assert.match(text, /Capability: Experimental, not yet tested against a live Discord client/);
    assert.match(text, /so a scale control cannot be offered/);
    assert.doesNotMatch(text, /webhook|friends|log in|login|chat overlay/i);
    unmount();
  });
});

