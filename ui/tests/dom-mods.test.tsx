import { absent, act, choose, click, installTauri, mount, one, resetBrowser, settle, submit, type, withText } from "./dom";
import assert from "node:assert/strict";
import { beforeEach, describe, test } from "node:test";
import { ModsPanel } from "../src/components/ModsPanel";
import type { LibraryApi } from "../src/lib/library";
import ModsView from "../src/views/ModsView";
import { tauriModsApi, type DeployPlan, type PurgeItem } from "../src/lib/mods";
import { gogGame, steamGame } from "./fixtures";
import { buildMods, fakeMods, mod, modRoot, problem, profile } from "./mods-fake";

beforeEach(() => resetBrowser());

const GAMES = [steamGame, gogGame];
const HD2 = steamGame.id;
const panel = (host: HTMLElement) => one(host, "section[aria-label='Mods and profiles']");
const form = (host: HTMLElement, label: string) => one<HTMLFormElement>(host, `form[aria-label='${label}']`);
/** The input, select or textarea of the label whose own text (not its options') is `label`. */
const field = (f: HTMLElement, label: string) => {
  const found = [...f.querySelectorAll("label")].filter((l) => l.firstChild?.textContent?.trim() === label);
  if (found.length !== 1) throw new Error(`expected exactly one label ${JSON.stringify(label)}, found ${found.length}`);
  return found[0]!.querySelector("input, select, textarea") as HTMLInputElement & HTMLSelectElement & HTMLTextAreaElement;
};
/** `type` from the helpers sets an input's value; a textarea has its own setter, so it needs its own few lines. */
async function typeInto(area: HTMLTextAreaElement, value: string) {
  await act(async () => {
    const setter = Object.getOwnPropertyDescriptor(window.HTMLTextAreaElement.prototype, "value")?.set;
    if (!setter) throw new Error("no value setter on HTMLTextAreaElement");
    setter.call(area, value);
    area.dispatchEvent(new window.Event("input", { bubbles: true }));
  });
}
const dialog = (host: HTMLElement) => one(host, "[role=dialog]");
const noDialog = (host: HTMLElement) => absent(host.querySelector("[role=dialog]"), "no question should be open");
const alertText = (host: HTMLElement) => [...host.querySelectorAll("[role=alert]")].map((e) => e.textContent).join(" | ");
const status = (host: HTMLElement) => one(panel(host), "[role=status]").textContent ?? "";

/** A copy of a game with one managed build of it, a mod library, one mod and one profile that holds it. */
function world(extra: Parameters<typeof fakeMods>[0] = {}) {
  return {
    mods: [mod(HD2, "louder", "1.0", { name: "Louder" })],
    profiles: [profile("main", HD2, "hd2-legacy-2", [{ mod_id: "louder", version: "1.0", enabled: true }], { name: "Main" })],
    builds: [buildMods("hd2-legacy-2", HD2, "legacy")],
    mod_roots: [modRoot()],
    deployable_games: [HD2],
    ...extra,
  };
}

async function start(initial: Parameters<typeof fakeMods>[0] = world(), games = GAMES) {
  const fake = fakeMods(initial);
  const m = await mount(<ModsPanel api={fake.api} games={games} />);
  await settle();
  return { fake, ...m };
}

const profileCard = (host: HTMLElement, name = "Main") => one(host, `article[aria-label='Profile ${name}']`);
const deployed = (profileId = "main") => ({
  id: `dep-${profileId}`,
  profile_id: profileId,
  profile_name: "Main",
  status: "deployed" as const,
  started_unix: 5,
  finished_unix: 6,
  files: 2,
});
const item = (path: string, action: PurgeItem["action"], detail = ""): PurgeItem => ({ path, mod_id: "louder", action, detail });

describe("the mods panel, clicked through", () => {
  test("in a plain browser it says it needs the desktop app and offers nothing to press", async () => {
    const { host, unmount } = await mount(<ModsPanel api={null} games={GAMES} />);
    assert.match(panel(host).textContent ?? "", /Browser preview.*need the desktop app/);
    absent(host.querySelector("button, input, select"), "nothing to operate without a backend");
    assert.equal(one(panel(host), ".badge").textContent, "Experimental", "even the preview says what the label is");
    unmount();
  });

  test("it is labelled Experimental and says that Deploy is refused for every game today", async () => {
    const { host, unmount } = await start();
    const badge = one(panel(host), ".badge");
    assert.equal(badge.textContent, "Experimental");
    assert.ok(badge.classList.contains("badge-experimental"));
    const text = panel(host).textContent ?? "";
    assert.match(text, /tested on made-up folders only/);
    assert.match(text, /no adapter for a real game exists yet, so Deploy is\s+refused for every game today/);
    assert.match(text, /anti-cheat/);
    unmount();
  });

  test("it lists the mods, profiles and what is deployed, in the owner's words, with the notes marked as unchecked", async () => {
    const { host, unmount } = await start(
      world({
        mods: [mod(HD2, "louder", "1.0", { name: "Louder", source: "my own work", license_note: "mine", compatible_versions: ["7.1.1"] }), mod(HD2, "quieter", "2.0", { name: "Quieter" })],
        builds: [buildMods("hd2-legacy-2", HD2, "legacy", { deployment: deployed() })],
      }),
    );
    const mods = one(host, "[aria-label='Mods']").textContent ?? "";
    assert.match(mods, /Louder 1\.0.*louder.*Helldivers 2.*2 files, 7 B/);
    assert.match(mods, /Said to work with game version 7\.1\.1/);
    assert.match(mods, /From: my own work\. Licence note \(your words, not checked\): mine/);
    assert.match(mods, /Quieter 2\.0.*Nobody said which game versions it works with, so SUL does not assume any/);
    assert.match(profileCard(host).textContent ?? "", /Main.*Deployed.*Helldivers 2 · build legacy hd2-legacy-2/);
    assert.match(one(host, "[aria-label='Deployed mods']").textContent ?? "", /Helldivers 2 · legacy.*Deployed: “Main” \(2 files\)/);
    assert.match(
      one(host, "[aria-label='Deployed mods']").textContent ?? "",
      /removes only what SUL.s record proves it put there.*exactly what was deployed.*recovery\s+folder rather than being deleted.*latest purge can be undone until something else changes in the build\. A purge needs no adapter/,
      "the page says what a purge may and may not do",
    );
    unmount();
  });

  test("the first load failing shows the reason and leaves the page standing", async () => {
    const fake = fakeMods(world());
    fake.failNext("view", "the mods file is damaged");
    const { host, unmount } = await mount(<ModsPanel api={fake.api} games={GAMES} />);
    await settle();
    assert.match(alertText(host), /the mods file is damaged/);
    absent(host.querySelector("[aria-label='Mods']"), "nothing is listed from data that could not be read");
    unmount();
  });

  test("an interrupted deploy is flagged and points to a purge preview, which undoes whatever was done", async () => {
    const { host, unmount } = await start(world({ builds: [buildMods("hd2-legacy-2", HD2, "legacy", { deployment: { ...deployed(), status: "in_progress" } })] }));
    const banner = one(host, "[aria-label='Unfinished deploys']");
    assert.match(banner.textContent ?? "", /A deploy into hd2-legacy-2 did not finish, so some of its files may be in place/);
    assert.match(banner.textContent ?? "", /Preview a purge for that build/);
    assert.match(one(host, "[aria-label='Deployed mods']").textContent ?? "", /A deploy of “Main” \(2 files, not finished\)/);
    unmount();
  });
});

describe("importing a mod", () => {
  async function fillIn(host: HTMLElement) {
    const f = form(host, "Add a mod");
    await type(field(f, "Mod name"), "Brighter Lights");
    await type(field(f, "Version"), " 1.2 ");
    await type(field(f, "Folder with the mod's files"), " D:\\Mods\\Lights ");
    await type(field(f, "Where it came from"), "my own work");
    await type(field(f, "Licence or permission"), "mine");
    await type(field(f, "Needs these mods (ids)"), "louder >= 1.0, extra");
    await type(field(f, "Works with game versions"), "7.1.1, 7.1.0");
    return f;
  }

  test("nothing is copied until the question is answered Copy, and the question names the folder, the target and what is not done", async () => {
    const { fake, host, unmount } = await start(world({ mod_roots: [modRoot("modlib", "Mod library"), modRoot("spare", "Spare")] }));
    const f = await fillIn(host);
    await click(withText(f, "button", "Copy into the mod library…"));
    const q = dialog(host).textContent ?? "";
    assert.match(q, /copy every file in D:\\Mods\\Lights into the mod library “Mod library” as Brighter Lights\s+1\.2 for Helldivers 2/);
    assert.match(q, /The folder is only read\. Nothing is deployed to a game/);
    assert.deepEqual(fake.changes(), [], "nothing is sent before the answer");

    await click(withText(dialog(host), "button", "Cancel"));
    noDialog(host);
    assert.deepEqual(fake.changes(), [], "Cancel sends nothing");
    assert.equal(field(form(host, "Add a mod"), "Mod name").value, "Brighter Lights", "what was typed is kept after Cancel");

    await click(withText(form(host, "Add a mod"), "button", "Copy into the mod library…"));
    await click(withText(dialog(host), "button", "Copy"));
    noDialog(host);
    assert.deepEqual(fake.changes(), [
      {
        op: "importMod",
        args: [
          {
            game_id: HD2,
            mod_id: "brighter-lights",
            name: "Brighter Lights",
            version: "1.2",
            requires: [{ mod_id: "louder", min_version: "1.0" }, { mod_id: "extra", min_version: null }],
            compatible_versions: ["7.1.1", "7.1.0"],
            source: "my own work",
            license_note: "mine",
            path: "D:\\Mods\\Lights",
          },
          "modlib",
        ],
      },
    ]);
    assert.match(status(host), /Copied “Brighter Lights” into the mod library and checked the copy\. The folder it came from was only read\./);
    assert.match(one(host, "[aria-label='Mods']").textContent ?? "", /Brighter Lights 1\.2/);
    assert.equal(field(form(host, "Add a mod"), "Mod name").value, "", "the form is cleared once it worked");
    unmount();
  });

  test("a different library folder can be chosen, and the question names it", async () => {
    const { fake, host, unmount } = await start(world({ mod_roots: [modRoot("modlib", "Mod library"), modRoot("spare", "Spare")] }));
    const f = await fillIn(host);
    await choose(field(f, "Copy into") as unknown as HTMLSelectElement, "spare");
    await click(withText(f, "button", "Copy into the mod library…"));
    assert.match(dialog(host).textContent ?? "", /mod library “Spare”/);
    await click(withText(dialog(host), "button", "Copy"));
    assert.equal(fake.changes()[0]!.args[1], "spare");
    unmount();
  });

  test("a refusal keeps what was typed and shows the core's reason", async () => {
    const { fake, host, unmount } = await start(world({ mods: [mod(HD2, "brighter-lights", "1.2", {})] }));
    const f = await fillIn(host);
    await click(withText(f, "button", "Copy into the mod library…"));
    await click(withText(dialog(host), "button", "Copy"));
    assert.match(alertText(host), /brighter-lights@1\.2 is already in the store/);
    assert.equal(fake.changes().length, 1);
    assert.equal(field(form(host, "Add a mod"), "Folder with the mod's files").value, " D:\\Mods\\Lights ", "the typed path is still there");
    unmount();
  });

  test("a missing name, version or folder, or a bad id list, is said on the page and asks the backend nothing", async () => {
    const { fake, host, unmount } = await start();
    const f = form(host, "Add a mod");
    await click(withText(f, "button", "Copy into the mod library…"));
    assert.match(dialog(host).textContent ?? "", /Copy this mod/, "the page asks, and the controller refuses afterwards");
    await click(withText(dialog(host), "button", "Copy"));
    assert.match(alertText(host), /Give the mod a name that has some letters or digits in it/);
    await type(field(f, "Mod name"), "Lights");
    await click(withText(f, "button", "Copy into the mod library…"));
    await click(withText(dialog(host), "button", "Copy"));
    assert.match(alertText(host), /Type the mod's version/);
    await type(field(f, "Version"), "1");
    await click(withText(f, "button", "Copy into the mod library…"));
    await click(withText(dialog(host), "button", "Copy"));
    assert.match(alertText(host), /Type the path of the folder/);
    await type(field(f, "Needs these mods (ids)"), "good, !!!");
    await click(withText(f, "button", "Copy into the mod library…"));
    noDialog(host);
    assert.match(alertText(host), /“!!!” is not a mod id/);
    assert.deepEqual(fake.changes(), []);
    unmount();
  });

  test("with no library folder for mods it says where to add one and offers no copy", async () => {
    const { fake, host, unmount } = await start(world({ mod_roots: [] }));
    const f = form(host, "Add a mod");
    await type(field(f, "Mod name"), "Lights");
    await click(withText(f, "button", "Copy into the mod library…"));
    noDialog(host);
    assert.match(alertText(host), /Add a library folder for mods first, on the Game Library page/);
    assert.deepEqual(fake.changes(), []);
    unmount();
  });

  test("Check files asks the backend to verify the one mod and shows the result next to it", async () => {
    const { fake, host, unmount } = await start();
    await click(one(host, "button[aria-label='Check the files of Louder 1.0']"));
    assert.deepEqual(fake.calls.filter((c) => c.op === "verifyMod"), [{ op: "verifyMod", args: [HD2, "louder", "1.0"] }]);
    assert.match(one(host, "[aria-label='Mods']").textContent ?? "", /Last check: Matches what was imported\./);
    unmount();
  });
});

describe("profiles", () => {
  test("a profile is made from a name, a game and one of the game's builds, with a tidy id; the original is marked as taking no mods", async () => {
    const { fake, host, unmount } = await start(world({ profiles: [], builds: [buildMods("hd2-original-1", HD2, "original"), buildMods("hd2-legacy-2", HD2, "legacy")] }));
    const f = form(host, "Make a profile");
    const options = [...field(f, "Build").querySelectorAll("option")].map((o) => o.textContent);
    assert.deepEqual(options, ["original (takes no mods) · hd2-original-1", "legacy · hd2-legacy-2"]);
    await type(field(f, "Name"), "My Big Setup");
    await choose(field(f, "Build") as unknown as HTMLSelectElement, "hd2-legacy-2");
    await submit(f);
    assert.deepEqual(fake.changes(), [{ op: "createProfile", args: ["my-big-setup", "My Big Setup", HD2, "hd2-legacy-2"] }]);
    assert.match(status(host), /Made the profile “My Big Setup”\. Nothing was deployed\./);
    assert.equal(field(form(host, "Make a profile"), "Name").value, "", "the name is cleared once it worked");
    assert.match(one(host, "[aria-label='Profiles']").textContent ?? "", /My Big Setup/);
    unmount();
  });

  test("with no managed build it says where to add one instead of showing a form", async () => {
    const { host, unmount } = await start(world({ profiles: [], builds: [] }));
    absent(host.querySelector("form[aria-label='Make a profile']"), "no form without a build");
    assert.match(one(host, "[aria-label='Profiles']").textContent ?? "", /needs a build SUL manages as a copy of a game\. Add one on the Game Library page first/);
    unmount();
  });

  test("a profile without a usable name is refused on the page, and a duplicate id is refused by the backend", async () => {
    const { fake, host, unmount } = await start();
    const f = form(host, "Make a profile");
    await type(field(f, "Name"), "???");
    await submit(f);
    assert.match(alertText(host), /Give the profile a name that has some letters or digits in it/);
    assert.deepEqual(fake.changes(), []);
    await type(field(f, "Name"), "Main");
    await submit(f);
    assert.match(alertText(host), /a profile “main” already exists/);
    assert.equal(field(form(host, "Make a profile"), "Name").value, "Main", "what was typed is kept");
    unmount();
  });

  test("adding a mod puts it at the end; a mod already in the profile is not offered again", async () => {
    const { fake, host, unmount } = await start(world({ mods: [mod(HD2, "louder", "1.0", { name: "Louder" }), mod(HD2, "quieter", "2.0", { name: "Quieter", imported_unix: 9 }), mod(HD2, "quieter", "1.0", { name: "Quieter", imported_unix: 1 })] }));
    const f = form(host, "Add a mod to Main");
    assert.deepEqual([...field(f, "Add a mod").querySelectorAll("option")].map((o) => o.textContent), ["Quieter 2.0"], "only the most recently imported version of each mod not yet in the profile");
    await click(withText(f, "button", "Add to the end"));
    assert.deepEqual(fake.changes(), [{ op: "editProfile", args: ["main", { action: "add_mod", mod_id: "quieter", version: "2.0", position: null }] }]);
    assert.deepEqual([...profileCard(host).querySelectorAll("ol li .trust-title")].map((e) => e.textContent), ["Louder", "Quieter"]);
    assert.match(status(host), /^Saved\.$/);
    unmount();
  });

  test("load order: Earlier and Later move by one, and the ends cannot move further", async () => {
    const { fake, host, unmount } = await start(
      world({
        mods: [mod(HD2, "a", "1", { name: "A" }), mod(HD2, "b", "1", { name: "B" })],
        profiles: [profile("main", HD2, "hd2-legacy-2", [{ mod_id: "a", version: "1", enabled: true }, { mod_id: "b", version: "1", enabled: true }], { name: "Main" })],
      }),
    );
    assert.equal(one<HTMLButtonElement>(host, "button[aria-label='Load a earlier']").disabled, true);
    assert.equal(one<HTMLButtonElement>(host, "button[aria-label='Load b later']").disabled, true);
    await click(one(host, "button[aria-label='Load b earlier']"));
    assert.deepEqual(fake.changes(), [{ op: "editProfile", args: ["main", { action: "move", mod_id: "b", to_index: 0 }] }]);
    assert.deepEqual([...profileCard(host).querySelectorAll("ol li .trust-title")].map((e) => e.textContent), ["B", "A"]);
    await click(one(host, "button[aria-label='Load b later']"));
    assert.deepEqual(fake.changes()[1], { op: "editProfile", args: ["main", { action: "move", mod_id: "b", to_index: 1 }] });
    unmount();
  });

  test("enabling, choosing a version and taking a mod out each send one edit", async () => {
    const { fake, host, unmount } = await start(world({ mods: [mod(HD2, "louder", "1.0", { name: "Louder" }), mod(HD2, "louder", "2.0", { name: "Louder" })] }));
    const card = profileCard(host);
    await click(one(card, "input[type=checkbox]"));
    await choose(field(one(card, "ol li"), "Version") as unknown as HTMLSelectElement, "2.0");
    await click(one(host, "button[aria-label='Take louder out of Main']"));
    assert.deepEqual(
      fake.changes().map((c) => c.args[1]),
      [{ action: "set_enabled", mod_id: "louder", enabled: false }, { action: "set_version", mod_id: "louder", version: "2.0" }, { action: "remove_mod", mod_id: "louder" }],
    );
    assert.match(profileCard(host).textContent ?? "", /No mods in this profile yet/);
    unmount();
  });

  test("a version that is not in the store is shown as such, and a profile's own problems are listed on the card", async () => {
    const { host, unmount } = await start(
      world({
        profiles: [
          profile("main", HD2, "hd2-legacy-2", [{ mod_id: "louder", version: "9.9", enabled: true }], { name: "Main", problems: [problem("louder 9.9 is not in the mod store"), problem("louder has no stated game version", "warning")] }),
        ],
      }),
    );
    const card = profileCard(host);
    assert.ok([...card.querySelectorAll("option")].some((o) => o.textContent === "9.9 (not in the store)"));
    assert.match(one(card, "[aria-label='Problems that block a deploy']").textContent ?? "", /louder 9\.9 is not in the mod store/);
    assert.match(one(card, "[aria-label='Things to check before a deploy']").textContent ?? "", /louder has no stated game version/);
    unmount();
  });

  test("rename, notes, settings and moving to another build each send one edit; a refusal is shown and keeps the typed text", async () => {
    const { fake, host, unmount } = await start(world({ builds: [buildMods("hd2-legacy-2", HD2, "legacy"), buildMods("hd2-other-3", HD2, "optimized")] }));
    const card = profileCard(host);

    // Notes, a setting and a move come first, while the profile is still called Main; the rename comes last because it changes the name the card goes by.
    const notes = one<HTMLFormElement>(card, "form[aria-label='Notes for Main']");
    await typeInto(field(notes, "Notes"), "for solo play");
    await submit(notes);
    const setting = one<HTMLFormElement>(profileCard(host), "form[aria-label='Add a setting to Main']");
    await type(field(setting, "Setting"), " difficulty ");
    await type(field(setting, "Value"), "9");
    await submit(setting);
    const move = one<HTMLFormElement>(profileCard(host), "form[aria-label='Move Main to another build']");
    await click(withText(move, "button", "Move the profile"));
    const rename = one<HTMLFormElement>(profileCard(host), "form[aria-label='Rename Main']");
    await type(field(rename, "Name"), "Heavy");
    await submit(rename);

    assert.deepEqual(
      fake.changes().map((c) => c.args[1]),
      [{ action: "notes", notes: "for solo play" }, { action: "set_config", key: "difficulty", value: "9" }, { action: "set_build", build_id: "hd2-other-3" }, { action: "rename", name: "Heavy" }],
    );
    assert.match(profileCard(host, "Heavy").textContent ?? "", /difficulty.*9/);
    assert.match(profileCard(host, "Heavy").textContent ?? "", /optimized/);
    unmount();
  });

  test("moving a deployed profile to another build is refused with the core's words, and the page says to purge first", async () => {
    const { fake, host, unmount } = await start(world({ builds: [buildMods("hd2-legacy-2", HD2, "legacy", { deployment: deployed() }), buildMods("hd2-other-3", HD2, "optimized")] }));
    assert.match(profileCard(host).textContent ?? "", /This profile is deployed in its build\. Purge it there before moving it/);
    await click(withText(form(host, "Move Main to another build"), "button", "Move the profile"));
    assert.match(alertText(host), /deployed in its build; purge it there/);
    assert.equal(fake.changes().length, 1);
    assert.match(profileCard(host).textContent ?? "", /build legacy/);
    unmount();
  });

  test("editing a deployed profile says the build keeps what was deployed until the next deploy", async () => {
    const { host, unmount } = await start(world({ builds: [buildMods("hd2-legacy-2", HD2, "legacy", { deployment: deployed() })] }));
    await click(one(host, "input[type=checkbox]"));
    assert.match(status(host), /The build keeps what was deployed; the change applies at the next deploy/);
    unmount();
  });

  test("a game with no adapter cannot be previewed or deployed from the page, and the card says why", async () => {
    const { fake, host, unmount } = await start(world({ deployable_games: [] }));
    const card = profileCard(host);
    assert.equal(withText<HTMLButtonElement>(card, "button", "Preview deploy…").disabled, true);
    assert.equal(withText<HTMLButtonElement>(card, "button", "Purge & Deploy…").disabled, true);
    assert.match(card.textContent ?? "", /SUL has no adapter for Helldivers 2, so it does not know which folders mods may change there, and it deploys nothing without that/);
    await click(withText(card, "button", "Preview deploy…"));
    assert.deepEqual(fake.calls.filter((c) => c.op === "planDeploy"), []);
    unmount();
  });
});

describe("deploy, purge and undo ask before they change anything", () => {
  test("Preview deploy shows the files and changes nothing; Cancel sends nothing; Deploy sends one deploy and reports it", async () => {
    const { fake, host, unmount } = await start();
    await click(withText(profileCard(host), "button", "Preview deploy…"));
    const q = dialog(host);
    assert.match(q.textContent ?? "", /This is exactly what a deploy would do now\. Nothing has been changed yet/);
    assert.match(q.textContent ?? "", /1 file, 4 B\. A file it replaces is moved aside and kept, not deleted/);
    assert.match(one(q, "[aria-label='Files this deploy would change']").textContent ?? "", /Add data\/louder\.bin from louder 1\.0/);
    assert.deepEqual(fake.changes(), [], "a preview changes nothing");

    await click(withText(q, "button", "Cancel"));
    noDialog(host);
    assert.deepEqual(fake.changes(), [], "Cancel sends nothing");

    await click(withText(profileCard(host), "button", "Preview deploy…"));
    await click(withText(dialog(host), "button", "Deploy"));
    noDialog(host);
    assert.deepEqual(fake.changes(), [{ op: "deploy", args: ["main"] }]);
    assert.match(status(host), /Deployed 1 file: 1 added, 0 replaced/);
    assert.match(one(host, "[aria-label='Deployed mods']").textContent ?? "", /Deployed: “Main” \(1 file\)/);
    unmount();
  });

  test("a blocked preview names what blocks it and its Deploy button is disabled; nothing is sent", async () => {
    const { fake, host, unmount } = await start();
    const plan: DeployPlan = { profile_id: "main", build_id: "hd2-legacy-2", files: [], problems: [problem("louder needs extra, which is not in the profile")], total_bytes: 0 };
    fake.setPlan(plan);
    await click(withText(profileCard(host), "button", "Preview deploy…"));
    const q = dialog(host);
    assert.match(q.textContent ?? "", /This deploy is blocked, so nothing will be changed/);
    assert.match(q.textContent ?? "", /louder needs extra, which is not in the profile/);
    assert.equal(withText<HTMLButtonElement>(q, "button", "Deploy").disabled, true);
    await click(withText(q, "button", "Deploy"));
    assert.deepEqual(fake.changes(), []);
    unmount();
  });

  test("a preview of many files shows the first rows and says the deploy covers all of them", async () => {
    const { fake, host, unmount } = await start();
    const files = Array.from({ length: 205 }, (_, i) => ({ path: `data/f${i}.bin`, mod_id: "louder", mod_version: "1.0", action: "add" as const, sha256: "aa", size: 1, replaces: null, overrides: [] }));
    fake.setPlan({ profile_id: "main", build_id: "hd2-legacy-2", files, problems: [], total_bytes: 205 });
    await click(withText(profileCard(host), "button", "Preview deploy…"));
    assert.equal(host.querySelectorAll("[aria-label='Files this deploy would change'] li").length, 200);
    assert.match(dialog(host).textContent ?? "", /…and 5 more\. The deploy covers all 205\./);
    unmount();
  });

  test("a replaced file is shown with what wins over what, and a file the build's record did not list is called out", async () => {
    const { fake, host, unmount } = await start();
    fake.setPlan({
      profile_id: "main",
      build_id: "hd2-legacy-2",
      files: [
        { path: "data/a.bin", mod_id: "louder", mod_version: "1.0", action: "replace", sha256: "aa", size: 4, replaces: { sha256: "bb", size: 4, in_build_manifest: false }, overrides: ["quieter"] },
      ],
      problems: [],
      total_bytes: 4,
    });
    await click(withText(profileCard(host), "button", "Preview deploy…"));
    const row = one(dialog(host), "[aria-label='Files this deploy would change'] li").textContent ?? "";
    assert.match(row, /Replace data\/a\.bin from louder 1\.0/);
    assert.match(row, /the build’s record did not list this file/);
    assert.match(row, /wins over quieter/);
    unmount();
  });

  test("the core refusing a confirmed deploy (no adapter, file changed) is shown, and the page keeps its lists", async () => {
    const { fake, host, unmount } = await start();
    await click(withText(profileCard(host), "button", "Preview deploy…"));
    fake.failNext("deploy", "a file in the build changed since it was planned; nothing was changed");
    await click(withText(dialog(host), "button", "Deploy"));
    assert.match(alertText(host), /changed since it was planned; nothing was changed/);
    assert.match(profileCard(host).textContent ?? "", /Main/);
    assert.match(one(host, "[aria-label='Deployed mods']").textContent ?? "", /Nothing is deployed in any build/);
    unmount();
  });

  test("Preview purge lists each file with what would happen, flags files that would be left, and Purge sends one purge", async () => {
    const { fake, host, unmount } = await start(world({ builds: [buildMods("hd2-legacy-2", HD2, "legacy", { deployment: deployed() })] }));
    fake.setPurgeItems([item("data/a.bin", "remove_mod_file"), item("data/b.bin", "remove_mod_file_and_restore_base")]);
    await click(one(host, "button[aria-label='Preview a purge of hd2-legacy-2']"));
    const q = dialog(host);
    assert.match(q.textContent ?? "", /This is what a purge of “Main” would do now\. Nothing has been changed yet/);
    const rows = [...q.querySelectorAll("[aria-label='Files this purge would handle'] li")].map((l) => l.textContent ?? "");
    assert.match(rows[0]!, /data\/a\.bin.*louder.*Move the mod's file to the recovery folder/);
    assert.match(rows[1]!, /data\/b\.bin.*put the original file back/);
    absent(q.querySelector("[role=alert]"), "nothing is left, so no warning");
    assert.deepEqual(fake.changes(), [], "the preview changes nothing");
    await click(withText(q, "button", "Purge"));
    assert.deepEqual(fake.changes(), [{ op: "purge", args: ["hd2-legacy-2"] }]);
    assert.match(status(host), /Purged: 2 files moved to the recovery folder, 0 original files put back\./);
    assert.match(one(host, "[aria-label='Deployed mods']").textContent ?? "", /Nothing is deployed here.*The latest purge \(2 files of “Main”\) can still be undone/);
    unmount();
  });

  test("a purge that would leave a changed file says the build would stay partly modded, and what is left is reported afterwards", async () => {
    const { fake, host, unmount } = await start(world({ builds: [buildMods("hd2-legacy-2", HD2, "legacy", { deployment: deployed() })] }));
    fake.setPurgeItems([item("data/a.bin", "remove_mod_file"), item("data/b.bin", "leave_changed_since_deploy", "changed since it was deployed")]);
    await click(one(host, "button[aria-label='Preview a purge of hd2-legacy-2']"));
    const q = dialog(host);
    assert.match(q.textContent ?? "", /1 file would be left in place, so the build would stay partly modded/);
    assert.ok(one(q, "li.left-alone").textContent?.includes("data/b.bin"), "the file that stays is marked in the list");
    await click(withText(q, "button", "Purge"));
    assert.match(status(host), /1 file left in place, so the build is still partly modded: data\/b\.bin \(changed since it was deployed\)/);
    assert.match(one(host, "[aria-label='Deployed mods']").textContent ?? "", /Deployed: “Main”/, "the deployment stays on record because a file was left");
    unmount();
  });

  test("a build with nothing deployed offers no purge preview, so nothing is asked of the backend", async () => {
    const { fake, host, unmount } = await start(world({ builds: [buildMods("hd2-legacy-2", HD2, "legacy", { history: [{ id: "h1", kind: "purge", profile_id: "main", unix: 1, files: 1, note: "" }] })] }));
    assert.equal(one<HTMLButtonElement>(host, "button[aria-label='Preview a purge of hd2-legacy-2']").disabled, true, "nothing to purge, so the page does not even ask");
    assert.deepEqual(fake.calls.filter((c) => c.op === "planPurge"), []);
    unmount();
  });

  test("Undo asks first, names the profile and the file count, and sends one undo only after the answer", async () => {
    const { fake, host, unmount } = await start(world({ builds: [buildMods("hd2-legacy-2", HD2, "legacy", { undoable_purge: { id: "purge-1", profile_name: "Main", unix: 9, files: 3 } })] }));
    await click(one(host, "button[aria-label='Undo the latest purge of hd2-legacy-2']"));
    const q = dialog(host);
    assert.match(q.textContent ?? "", /put back the 3 files that purge of “Main” took out.*overwrites nothing: if anything changed it refuses and changes nothing/);
    assert.deepEqual(fake.calls.filter((c) => c.op === "undoPurge"), [], "asking sends nothing");
    await click(withText(q, "button", "Cancel"));
    assert.deepEqual(fake.changes(), []);
    await click(one(host, "button[aria-label='Undo the latest purge of hd2-legacy-2']"));
    await click(withText(dialog(host), "button", "Undo the purge"));
    assert.deepEqual(fake.changes(), [{ op: "undoPurge", args: ["hd2-legacy-2", "purge-1"] }]);
    assert.match(status(host), /Undid the purge: 1 file put back\./);
    unmount();
  });

  test("Purge & Deploy is only offered for a deployed profile, asks first, and sends one combined call after the answer", async () => {
    const { fake, host, unmount } = await start(world({ builds: [buildMods("hd2-legacy-2", HD2, "legacy", { deployment: deployed() })] }));
    fake.setPurgeItems([item("data/louder.bin", "remove_mod_file")]);
    await click(withText(profileCard(host), "button", "Purge & Deploy…"));
    const q = dialog(host);
    assert.match(q.textContent ?? "", /Purge, then deploy “Main”\?/);
    assert.match(q.textContent ?? "", /the files it would add are not listed here because they depend on the purge/);
    assert.deepEqual(fake.changes(), []);
    await click(withText(q, "button", "Purge & Deploy"));
    assert.deepEqual(fake.changes(), [{ op: "purgeAndDeploy", args: ["main"] }]);
    assert.match(status(host), /Purged: 1 file moved to the recovery folder.*Deployed 1 file/);
    unmount();
  });

  test("Purge & Deploy where something is left says the deploy was not attempted", async () => {
    const { fake, host, unmount } = await start(world({ builds: [buildMods("hd2-legacy-2", HD2, "legacy", { deployment: deployed() })] }));
    fake.setPurgeItems([item("data/louder.bin", "leave_changed_since_deploy", "changed since it was deployed")]);
    await click(withText(profileCard(host), "button", "Purge & Deploy…"));
    assert.match(dialog(host).textContent ?? "", /Purge & Deploy would then not deploy/);
    await click(withText(dialog(host), "button", "Purge & Deploy"));
    assert.match(status(host), /The deploy was not attempted\./);
    unmount();
  });

  test("Purge & Deploy is not offered for a profile that is not deployed", async () => {
    const { host, unmount } = await start();
    assert.equal(withText<HTMLButtonElement>(profileCard(host), "button", "Purge & Deploy…").disabled, true);
    unmount();
  });

  test("Escape closes any question without sending anything", async () => {
    const { fake, host, unmount } = await start();
    await click(withText(profileCard(host), "button", "Preview deploy…"));
    await act(async () => {
      window.dispatchEvent(new window.KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    });
    noDialog(host);
    assert.deepEqual(fake.changes(), []);
    unmount();
  });

  test("while an operation runs the buttons are disabled and the page says it cannot be cancelled; a second click sends nothing", async () => {
    const { fake, host, unmount } = await start();
    await click(withText(profileCard(host), "button", "Preview deploy…"));
    const release = fake.hold();
    // `click` returns while the held call is still open: it only waits a turn, not for the backend.
    await click(withText(dialog(host), "button", "Deploy"));
    assert.match(status(host), /Working… this can take a while for a large mod, and it cannot be cancelled once started\./);
    assert.equal(withText<HTMLButtonElement>(profileCard(host), "button", "Preview deploy…").disabled, true);
    await click(withText(profileCard(host), "button", "Preview deploy…"));
    release();
    await settle();
    assert.equal(fake.changes().length, 1, "one deploy, however many times the page was pressed");
    unmount();
  });
});

describe("what the panel sends to the real invoke", () => {
  const view = () => ({ mods: [], profiles: [], builds: [], mod_roots: [], deployable_games: [] });
  const plan = { profile_id: "p", build_id: "b", files: [], problems: [], total_bytes: 0 };
  const purgePlan = { build_id: "b", deployment: null, items: [] };
  const deployReport = { deployment_id: "d", added: [], replaced: [], kept: [], warnings: [] };
  const purgeReport = { purge_id: null, removed: [], restored: [], forgotten: [], left: [], recovery_dir: "D:\\r" };

  function answer(cmd: string) {
    switch (cmd) {
      case "mods_verify":
        return { report: { missing: [], modified: [], extra: [] }, view: view() };
      case "mods_plan_deploy":
        return plan;
      case "mods_deploy":
        return { report: deployReport, view: view() };
      case "mods_plan_purge":
        return purgePlan;
      case "mods_purge":
        return { report: purgeReport, view: view() };
      case "mods_undo_purge":
        return { report: { put_back: [], originals_moved_aside_again: [] }, view: view() };
      case "mods_purge_and_deploy":
        return { report: { purge: purgeReport, deploy: null }, view: view() };
      default:
        return view();
    }
  }

  test("every command name and argument is the one the Rust commands take (camelCase top-level arguments, snake_case objects)", async () => {
    const tauri = installTauri(answer);
    const api = tauriModsApi();
    assert.ok(api, "inside the webview there is a backend");
    const source = { game_id: "g", mod_id: "m", name: "M", version: "1", requires: [], compatible_versions: [], source: "s", license_note: "l", path: "D:\\m" };
    await api.view();
    await api.importMod(source, "root");
    await api.verifyMod("g", "m", "1");
    await api.createProfile("id", "Name", "g", "b");
    await api.editProfile("id", { action: "move", mod_id: "m", to_index: 2 });
    await api.planDeploy("id");
    await api.deploy("id");
    await api.planPurge("b");
    await api.purge("b");
    await api.undoPurge("b", "purge-1");
    await api.purgeAndDeploy("id");
    assert.deepEqual(tauri.calls, [
      { cmd: "mods_view", args: {} },
      { cmd: "mods_import", args: { source, rootId: "root" } },
      { cmd: "mods_verify", args: { gameId: "g", modId: "m", version: "1" } },
      { cmd: "mods_create_profile", args: { id: "id", name: "Name", gameId: "g", buildId: "b" } },
      { cmd: "mods_edit_profile", args: { profileId: "id", edit: { action: "move", mod_id: "m", to_index: 2 } } },
      { cmd: "mods_plan_deploy", args: { profileId: "id" } },
      { cmd: "mods_deploy", args: { profileId: "id" } },
      { cmd: "mods_plan_purge", args: { buildId: "b" } },
      { cmd: "mods_purge", args: { buildId: "b" } },
      { cmd: "mods_undo_purge", args: { buildId: "b", purgeId: "purge-1" } },
      { cmd: "mods_purge_and_deploy", args: { profileId: "id" } },
    ]);
    tauri.remove();
  });

  test("in a plain browser there is no backend", () => {
    assert.equal(tauriModsApi(), null);
  });

  test("data in the wrong shape is refused with a plain message instead of reaching the page", async () => {
    for (const junk of [[], null, "text", { mods: [] }, { mods: [], profiles: [], builds: [], mod_roots: [] }]) {
      const tauri = installTauri(() => junk);
      const api = tauriModsApi()!;
      await assert.rejects(api.view(), /does not understand/);
      await assert.rejects(api.deploy("p"), /does not understand/);
      await assert.rejects(api.purge("b"), /does not understand/);
      await assert.rejects(api.planDeploy("p"), /does not understand/);
      await assert.rejects(api.planPurge("b"), /does not understand/);
      await assert.rejects(api.verifyMod("g", "m", "1"), /does not understand/);
      tauri.remove();
    }
  });

  test("a page whose backend sends junk shows an error and stays up", async () => {
    const tauri = installTauri(() => []);
    const { host, unmount } = await mount(<ModsPanel games={[]} />);
    await settle();
    assert.match(one(host, "[role=alert]").textContent ?? "", /does not understand/);
    unmount();
    tauri.remove();
  });

  test("a failing Rust command (a plain string) is shown as it is", async () => {
    const tauri = installTauri(() => {
      throw "SUL has no adapter for “madeupgame”, so it deploys nothing without that. Nothing was changed.";
    });
    const { host, unmount } = await mount(<ModsPanel games={[]} />);
    await settle();
    assert.match(one(host, "[role=alert]").textContent ?? "", /SUL has no adapter for “madeupgame”/);
    unmount();
    tauri.remove();
  });
});

describe("the Mods & Mod Builder page", () => {
  const libraryApi = (): LibraryApi => ({
    list: async () => GAMES,
    scan: async () => ({ games: GAMES, added: 0, updated: 0, returned: 0, missing: 0 }),
    matrix: async () => {
      throw "not used here";
    },
    tick: async () => {
      throw "not used here";
    },
    play: async () => {},
  });

  test("it says what is built and what is not, uses the library's game names, and the mods panel sits between them", async () => {
    const fake = fakeMods(world());
    const { host, unmount } = await mount(<ModsView api={fake.api} libraryApi={libraryApi()} />);
    await settle();
    assert.equal(one(host, "h1").textContent, "Mods & Mod Builder");
    assert.match(host.textContent ?? "", /mod profiles, deploy\/purge, the builder, patches and external tools are built and Experimental; conflict analysis and measured comparisons are not built/);
    assert.match(field(form(host, "Add a mod"), "Game").textContent ?? "", /Helldivers 2.*Alpha/, "the games' names, not their ids");
    assert.match(one(host, "[aria-label='Mods']").textContent ?? "", /louder.*Helldivers 2/);
    const text = host.textContent ?? "";
    assert.match(text, /Not built yet/);
    assert.doesNotMatch(text, /external tools are not built/, "external tools are built now, so the note must not say they are not");
    assert.match(text, /Conflict analysis and measured comparisons are not built at all/);
    assert.ok(text.indexOf("Mods and profiles") < text.indexOf("Not built yet"));
    unmount();
  });

  test("given fake backends and nothing else, it does not reach for the real mods backend", async () => {
    const calls = installTauri(() => {
      throw "the real mods backend must not be used here";
    });
    const { host, unmount } = await mount(<ModsView libraryApi={libraryApi()} />);
    await settle();
    assert.match(panel(host).textContent ?? "", /Browser preview/);
    assert.deepEqual(calls.calls.filter((c) => c.cmd.startsWith("mods_") || c.cmd.startsWith("builder_") || c.cmd.startsWith("tools_")), []);
    calls.remove();
    unmount();
  });

  test("opened from the sidebar in a plain browser it shows the preview note and the not-built note, and nothing to press in the panel", async () => {
    const { host, unmount } = await mount(<ModsView api={null} libraryApi={null} />);
    await settle();
    assert.match(panel(host).textContent ?? "", /Browser preview: mods and profiles need the desktop app/);
    absent(panel(host).querySelector("button, input, select"), "nothing to operate without a backend");
    assert.match(host.textContent ?? "", /Not built yet/);
    unmount();
  });
});
