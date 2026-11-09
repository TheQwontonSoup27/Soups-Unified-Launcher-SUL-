import { absent, act, choose, click, installTauri, key, mount, one, resetBrowser, settle, type, waitFor, withText, type Mounted } from "./dom";
import assert from "node:assert/strict";
import { afterEach, beforeEach, describe, test } from "node:test";
import type { ReactElement } from "react";
import { ToolsPanel } from "../src/components/ToolsPanel";
import { tauriToolsApi } from "../src/lib/tools";
import ModsView from "../src/views/ModsView";
import type { LibraryApi } from "../src/lib/library";
import { fakeMods } from "./mods-fake";
import { fakeBuilder } from "./builder-fake";
import { HEX, OTHER_HEX, aKnown, aRunRecord, aTool, aWorkspace, fakeTools, modRoot } from "./tools-fake";

beforeEach(() => resetBrowser());

// A panel that polls keeps a timer. A test that fails before it unmounts its panel would leave that timer running and the
// whole test process would never end, so every panel mounted here is unmounted after its test, passed or failed.
const mounted: Mounted[] = [];
async function mountTracked(ui: ReactElement): Promise<Mounted> {
  const m = await mount(ui);
  let gone = false;
  const tracked = {
    host: m.host,
    unmount() {
      if (gone) return;
      gone = true;
      m.unmount();
    },
  };
  mounted.push(tracked);
  return tracked;
}
afterEach(() => {
  for (const m of mounted.splice(0)) m.unmount();
});

const panel = (host: HTMLElement) => one(host, "section[aria-label='External tools']");
const toolCard = (host: HTMLElement, name = "Audio") => one(host, `li[aria-label='Tool ${name}']`);
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
const form = (host: HTMLElement, label: string) => one<HTMLFormElement>(host, `form[aria-label='${label}']`);
const dialog = (host: HTMLElement) => one(host, "[role=dialog]");
const noDialog = (host: HTMLElement) => absent(host.querySelector("[role=dialog]"), "no question should be open");
const alertText = (host: HTMLElement) => [...host.querySelectorAll("[role=alert]")].map((e) => e.textContent).join(" | ");
const status = (host: HTMLElement) => one(panel(host), "[role=status]").textContent ?? "";

const WS = { recipe_id: "loud", root_id: "modlib", run_id: "run-1" };

/** One recorded tool, one finished recipe run to work in, a mod library and one known tool. */
function world(extra: Parameters<typeof fakeTools>[0] = {}) {
  return {
    tools: [aTool("audio", { display_name: "Audio", version: "1.0", license_note: "mine, I think", source_note: "a forum post" })],
    workspaces: [aWorkspace()],
    mod_roots: [modRoot()],
    known: [aKnown("hd2-audio-modder", { name: "Helldivers 2 Audio Modder" })],
    ...extra,
  };
}

async function start(initial: Parameters<typeof fakeTools>[0] = world(), pollMs = 60_000) {
  const fake = fakeTools(initial);
  const m = await mountTracked(<ToolsPanel api={fake.api} pollMs={pollMs} />);
  await settle();
  return { fake, ...m };
}

/** Preview a run of the one tool and answer the question with "Start the tool". */
async function startRun(host: HTMLElement, args = "") {
  if (args !== "") await typeInto(field(form(host, "Run a tool form"), "Arguments, one per line"), args);
  await click(withText(host, "button", "Preview the run…"));
  await click(withText(dialog(host), "button", "Start the tool"));
}

describe("the tools panel, clicked through", () => {
  test("in a plain browser it says it needs the desktop app and offers nothing to press", async () => {
    const { host, unmount } = await mountTracked(<ToolsPanel api={null} />);
    assert.match(panel(host).textContent ?? "", /Browser preview: external tools need the desktop app/);
    absent(host.querySelector("button, input, select"), "nothing to operate without a backend");
    assert.equal(one(panel(host), ".badge").textContent, "Experimental");
    unmount();
  });

  test("it is labelled Experimental and says what SUL cannot do: confine a program, or vouch for a tool that was never run", async () => {
    const { host, unmount } = await start();
    const badge = one(panel(host), ".game-card-head .badge");
    assert.equal(badge.textContent, "Experimental");
    assert.ok(badge.classList.contains("badge-experimental"));
    const text = panel(host).textContent ?? "";
    assert.match(text, /tested with made-up programs only/);
    assert.match(text, /downloads, copies and bundles nothing/);
    assert.match(text, /will not start a tool whose file is no longer the recorded one/);
    assert.match(text, /SUL cannot confine a program/);
    assert.match(text, /keeps running if this window is closed/);
    assert.match(text, /SUL has never run any real tool/);
    unmount();
  });

  test("the tools the spec names are listed with their label, what was read, and what is not established", async () => {
    const { host, unmount } = await start();
    const known = one(host, "div[aria-label='Tools the spec names']");
    assert.match(known.textContent ?? "", /Helldivers 2 Audio Modder/);
    assert.match(known.textContent ?? "", /Not established: That it works\./);
    const badge = one(known, ".badge");
    assert.equal(badge.textContent, "Not investigated");
    assert.ok(badge.classList.contains("badge-not_investigated"));
    assert.match(known.textContent ?? "", /Nothing here was run, so none of it says a tool works/);
    unmount();
  });

  test("a recorded tool is shown with its path and hash, and the owner's notes are marked as unchecked", async () => {
    const { host, unmount } = await start();
    const text = toolCard(host).textContent ?? "";
    assert.match(text, /Audio.*1\.0/);
    assert.match(text, /D:\\Tools\\audio\\audio\.exe/);
    assert.match(text, /2\.0 KB/);
    assert.match(text, /abababababab…/);
    assert.match(text, /Redistribution: Not known \(SUL assumes nothing\)\. SUL bundles nothing whatever this says\./);
    assert.match(text, /Licence note \(your words, not checked\): mine, I think\./);
    assert.match(text, /From: a forum post\./);
    unmount();
  });

  test("with nothing recorded it says so and the run form has nothing to run", async () => {
    const { host, unmount } = await start(world({ tools: [], workspaces: [] }));
    assert.match(one(host, "div[aria-label='Recorded tools']").textContent ?? "", /No tool is recorded yet/);
    assert.match(field(form(host, "Run a tool form"), "Tool").textContent ?? "", /record a tool first/);
    assert.match(field(form(host, "Run a tool form"), "Work in the files of").textContent ?? "", /no finished recipe run yet/);
    unmount();
  });

  test("recording a tool sends the typed request, clears the form, lists the tool and says nothing was started", async () => {
    const { fake, host, unmount } = await start();
    const f = form(host, "Record a tool form");
    await type(field(f, "Name"), "Wave Tool");
    await type(field(f, "Version you installed"), "2.1");
    await type(field(f, "Full path of the program"), "D:\\Tools\\wave.exe");
    await choose(field(f, "May it be redistributed?") as HTMLSelectElement, "not_permitted");
    await type(field(f, "Licence or permission"), "MIT, per the readme");
    await type(field(f, "Where you got it"), "the author's site");
    await click(withText(f, "button", "Record the tool"));
    assert.deepEqual(fake.changes(), [
      { op: "register", args: [{ id: "wave-tool", display_name: "Wave Tool", version: "2.1", exe_path: "D:\\Tools\\wave.exe", redistribution: "not_permitted", license_note: "MIT, per the readme", source_note: "the author's site" }] },
    ]);
    assert.match(status(host), /Recorded “Wave Tool” 2\.1\. SUL read the file once to hash it\. Nothing was copied, downloaded or started\./);
    assert.match(toolCard(host, "Wave Tool").textContent ?? "", /Redistribution: Not permitted/);
    assert.equal(field(f, "Name").value, "", "the form is empty again");
    unmount();
  });

  test("a form that is wrong shows what to fix and sends nothing; a refusal from the core keeps what was typed", async () => {
    const { fake, host, unmount } = await start();
    const f = form(host, "Record a tool form");
    await type(field(f, "Name"), "Wave Tool");
    await click(withText(f, "button", "Record the tool"));
    assert.match(alertText(host), /Write the version you installed/);
    assert.deepEqual(fake.changes(), []);
    await type(field(f, "Version you installed"), "2.1");
    await type(field(f, "Full path of the program"), "D:\\Tools\\wave.exe");
    fake.failNext("register", "SUL will not record that path: it is inside a game's folder");
    await click(withText(f, "button", "Record the tool"));
    assert.match(alertText(host), /inside a game's folder/);
    assert.equal(field(f, "Name").value, "Wave Tool", "the person's text is still there to correct");
    unmount();
  });

  test("Check file on a file that matches says so and offers nothing to accept", async () => {
    const { fake, host, unmount } = await start();
    await click(withText(toolCard(host), "button", "Check file"));
    assert.match(toolCard(host).textContent ?? "", /The file is the one that was recorded\./);
    absent(toolCard(host).querySelector("form"), "nothing to accept");
    assert.deepEqual(fake.changes(), []);
    unmount();
  });

  test("a missing file is shown as a problem and offers nothing to accept", async () => {
    const { fake, host, unmount } = await start();
    fake.setFile("audio", { state: "missing" });
    await click(withText(toolCard(host), "button", "Check file"));
    assert.match(alertText(host), /There is no file at the recorded path any more\./);
    absent(toolCard(host).querySelector("form"), "nothing to accept");
    unmount();
  });

  test("a changed file is shown as a problem; accepting asks first, and Cancel sends nothing", async () => {
    const { fake, host, unmount } = await start();
    fake.setFile("audio", { state: "changed", exe_sha256: OTHER_HEX, exe_size: 4096 });
    await click(withText(toolCard(host), "button", "Check file"));
    assert.match(alertText(host), /NOT the one that was recorded.*will not run it until you accept it/);
    const accept = form(toolCard(host), "Accept the changed file of Audio");
    await type(field(accept, "Version of the file you are accepting"), "2.0");
    await click(withText(accept, "button", "Accept this file…"));
    const text = dialog(host).textContent ?? "";
    assert.match(text, /Accept the changed file as Audio\?/);
    assert.match(text, /cdcdcdcdcdcd….*4\.1 KB.*Audio 2\.0/);
    assert.match(text, /SUL cannot tell whether the change is\s+wanted/);
    await click(withText(dialog(host), "button", "Cancel"));
    noDialog(host);
    assert.deepEqual(fake.changes(), [], "nothing was accepted");
    unmount();
  });

  test("confirming the acceptance sends the version and the hash the check showed, and the tool is listed at that version", async () => {
    const { fake, host, unmount } = await start();
    fake.setFile("audio", { state: "changed", exe_sha256: OTHER_HEX, exe_size: 4096 });
    await click(withText(toolCard(host), "button", "Check file"));
    const accept = form(toolCard(host), "Accept the changed file of Audio");
    await type(field(accept, "Version of the file you are accepting"), "2.0");
    await click(withText(accept, "button", "Accept this file…"));
    await click(withText(dialog(host), "button", "Accept it"));
    assert.deepEqual(fake.changes(), [{ op: "acceptChange", args: ["audio", "2.0", OTHER_HEX] }]);
    noDialog(host);
    assert.match(toolCard(host).textContent ?? "", /Audio.*2\.0/);
    assert.match(toolCard(host).textContent ?? "", /1 earlier version is kept in its history/);
    assert.match(status(host), /version 2\.0/);
    absent(toolCard(host).querySelector("form"), "the check is stale after accepting, so the form is gone");
    unmount();
  });

  test("accepting without a version is refused with what to write and sends nothing", async () => {
    const { fake, host, unmount } = await start();
    fake.setFile("audio", { state: "changed", exe_sha256: OTHER_HEX, exe_size: 4096 });
    await click(withText(toolCard(host), "button", "Check file"));
    await click(withText(form(toolCard(host), "Accept the changed file of Audio"), "button", "Accept this file…"));
    assert.match(dialog(host).textContent ?? "", /write its version first/);
    await click(withText(dialog(host), "button", "Accept it"));
    assert.match(alertText(host), /Write the version of the file you are accepting/);
    assert.deepEqual(fake.changes(), []);
    unmount();
  });

  test("Preview the run shows exactly what would start, and Cancel and Escape start nothing", async () => {
    const { fake, host, unmount } = await start();
    await typeInto(field(form(host, "Run a tool form"), "Arguments, one per line"), "--in\ndata");
    await click(withText(host, "button", "Preview the run…"));
    const text = dialog(host).textContent ?? "";
    assert.match(text, /Start “Audio”\?/);
    assert.match(text, /D:\\Tools\\audio\\audio\.exe.*version 1\.0.*abababababab….*D:\\SUL\\workspaces\\run-1\\files/);
    assert.match(text, /Loud, run run-1 in “Mod library”/);
    assert.deepEqual([...dialog(host).querySelectorAll("ul[aria-label='Arguments it will be given'] li")].map((l) => l.textContent), ["--in", "data"]);
    assert.match(text, /SUL cannot confine a program.*cannot cancel it from here/);
    assert.match(text, /checks the program's file again when you confirm/);
    await click(withText(dialog(host), "button", "Cancel"));
    noDialog(host);
    await click(withText(host, "button", "Preview the run…"));
    await key("Escape");
    noDialog(host);
    assert.deepEqual(fake.changes(), [], "nothing was started");
    assert.deepEqual(fake.calls.filter((c) => c.op === "planRun").length, 2, "each preview only planned");
    unmount();
  });

  test("with no arguments the preview says so", async () => {
    const { host, unmount } = await start();
    await click(withText(host, "button", "Preview the run…"));
    assert.match(dialog(host).textContent ?? "", /It will be given no arguments\./);
    unmount();
  });

  test("Start the tool starts exactly what was previewed, lists it as running and says it cannot be cancelled from here", async () => {
    const { fake, host, unmount } = await start();
    await typeInto(field(form(host, "Run a tool form"), "Settings to write in the record, as name = value, one per line"), "bank = music");
    await startRun(host, "--go");
    assert.deepEqual(fake.changes(), [{ op: "startRun", args: [{ tool_id: "audio", args: ["--go"], settings: { bank: "music" }, workspace: WS }] }]);
    noDialog(host);
    assert.match(status(host), /Started “Audio” \(process 4001\)\. It runs on its own: SUL cannot cancel it from here/);
    const running = one(host, "div[aria-label='Tools running now']").textContent ?? "";
    assert.match(running, /Audio · process 4001 · on Loud, run run-1 in “Mod library”/);
    unmount();
  });

  test("a plan with problems shows them and the Start button cannot be pressed", async () => {
    const fake = fakeTools(world());
    fake.setPlanProblems(["the workspace folder holds a link, so the before-and-after comparison would be wrong"]);
    const { host, unmount } = await mountTracked(<ToolsPanel api={fake.api} pollMs={60_000} />);
    await settle();
    await click(withText(host, "button", "Preview the run…"));
    assert.match(one(dialog(host), "ul[aria-label='Problems that block this run']").textContent ?? "", /holds a link/);
    const start = withText<HTMLButtonElement>(dialog(host), "button", "Start the tool");
    assert.equal(start.disabled, true);
    await click(start);
    assert.deepEqual(fake.changes(), []);
    unmount();
  });

  test("a tool whose file changed cannot be started: the preview names the problem and nothing is sent", async () => {
    const { fake, host, unmount } = await start();
    fake.setFile("audio", { state: "changed", exe_sha256: OTHER_HEX, exe_size: 1 });
    await click(withText(host, "button", "Preview the run…"));
    assert.match(dialog(host).textContent ?? "", /not the one that was recorded, so it will not be started/);
    assert.equal(withText<HTMLButtonElement>(dialog(host), "button", "Start the tool").disabled, true);
    assert.deepEqual(fake.changes(), []);
    unmount();
  });

  test("with no finished recipe run to work in, previewing says what to do first and sends nothing", async () => {
    const { fake, host, unmount } = await start(world({ workspaces: [] }));
    await click(withText(host, "button", "Preview the run…"));
    assert.match(alertText(host), /Pick the finished run.*A recipe has to be run first/);
    noDialog(host);
    assert.equal(fake.calls.filter((c) => c.op === "planRun").length, 0);
    unmount();
  });

  test("a settings line that is wrong is shown before anything is planned", async () => {
    const { fake, host, unmount } = await start();
    await typeInto(field(form(host, "Run a tool form"), "Settings to write in the record, as name = value, one per line"), "no equals sign");
    await click(withText(host, "button", "Preview the run…"));
    assert.match(alertText(host), /Line 1: write a name, an equals sign and a value/);
    assert.equal(fake.calls.filter((c) => c.op === "planRun").length, 0);
    unmount();
  });

  test("a running tool is asked about on a timer; when it finishes its record is shown with what it did and what it cannot see", async () => {
    const { fake, host, unmount } = await start(world(), 5);
    await startRun(host);
    fake.finish(
      1,
      aRunRecord({
        added: [{ path: "data/new.bin", sha256: OTHER_HEX, size: 12 }],
        changed: [{ path: "data/a.bin", before_sha256: HEX, after_sha256: OTHER_HEX }],
        removed: ["data/old.bin"],
        settings: { bank: "music" },
      }),
    );
    await waitFor(() => host.querySelector("div[aria-label='Runs finished in this window']") !== null, "the finished run to be listed");
    absent(host.querySelector("div[aria-label='Tools running now']"), "it is not running any more");
    const done = one(host, "div[aria-label='Runs finished in this window']");
    const text = done.textContent ?? "";
    assert.match(text, /Finished with success\. In the workspace folder: 1 file added, 1 file changed, 1 file removed\./);
    assert.deepEqual([...done.querySelectorAll("ul[aria-label='Files added'] li")].map((l) => l.textContent), ["data/new.bin (12 B)"]);
    assert.deepEqual([...done.querySelectorAll("ul[aria-label='Files changed'] li")].map((l) => l.textContent), ["data/a.bin"]);
    assert.deepEqual([...done.querySelectorAll("ul[aria-label='Files removed'] li")].map((l) => l.textContent), ["data/old.bin"]);
    assert.match(text, /Settings you wrote down \(only recorded, not passed to the tool\): bank = music/);
    assert.match(text, /SUL cannot confine a program, so anything it wrote elsewhere is not in this record\./);
    assert.match(text, /Record kept as run-1\.json beside the run\./);
    assert.match(status(host), /“Audio” finished\./);
    unmount();
  });

  test("a failed run is shown as failed with its exit code, and a folder that could not be compared is not shown as 'nothing changed'", async () => {
    const { fake, host, unmount } = await start(world(), 5);
    await startRun(host);
    fake.finish(1, aRunRecord({ success: false, exit_code: 3, added: [{ path: "data/ghost.bin", sha256: OTHER_HEX, size: 1 }], changed: [{ path: "data/ghost2.bin", before_sha256: HEX, after_sha256: OTHER_HEX }], removed: ["data/ghost3.bin"], comparison_error: "a link inside the folder" }));
    await waitFor(() => host.querySelector("div[aria-label='Runs finished in this window']") !== null, "the finished run to be listed");
    const done = one(host, "div[aria-label='Runs finished in this window']");
    assert.match(done.textContent ?? "", /Failed \(exit code 3\)\./);
    assert.match(done.textContent ?? "", /could not be compared, so this record does not say what changed/);
    assert.match(one(done, "[role=alert]").textContent ?? "", /The lists of added, changed and removed files are empty because nothing could be seen, not because nothing happened/);
    absent(done.querySelector("ul[aria-label='Files added']"), "no file list is shown when nothing could be compared");
    absent(done.querySelector("ul[aria-label='Files changed']"), "no file list is shown when nothing could be compared");
    absent(done.querySelector("ul[aria-label='Files removed']"), "no file list is shown when nothing could be compared");
    assert.doesNotMatch(done.textContent ?? "", /ghost/);
    unmount();
  });

  test("a record with thousands of files lists the first 200 and says how many more there are", async () => {
    const { fake, host, unmount } = await start(world(), 5);
    await startRun(host);
    const added = Array.from({ length: 205 }, (_, i) => ({ path: `data/f${String(i).padStart(3, "0")}.bin`, sha256: OTHER_HEX, size: 1 }));
    fake.finish(1, aRunRecord({ added }));
    await waitFor(() => host.querySelector("div[aria-label='Runs finished in this window']") !== null, "the finished run to be listed");
    const done = one(host, "div[aria-label='Runs finished in this window']");
    assert.equal(done.querySelectorAll("ul[aria-label='Files added'] li").length, 200);
    assert.match(done.textContent ?? "", /…and 5 more\./);
    assert.match(done.textContent ?? "", /205 files added/, "the count in the sentence is the whole count");
    unmount();
  });

  test("once the panel is closed it stops asking about running tools", async () => {
    const { fake, host, unmount } = await start(world(), 5);
    await startRun(host);
    await waitFor(() => fake.calls.some((c) => c.op === "pollRun"), "the first poll");
    unmount();
    await settle();
    const after = fake.calls.filter((c) => c.op === "pollRun").length;
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 60));
    });
    assert.equal(fake.calls.filter((c) => c.op === "pollRun").length, after, "no poll after the panel was closed");
  });

  test("each tool shows its own check: one changed, one missing", async () => {
    const { fake, host, unmount } = await start(world({ tools: [aTool("audio", { display_name: "Audio" }), aTool("wave", { display_name: "Wave" })] }));
    fake.setFile("audio", { state: "changed", exe_sha256: OTHER_HEX, exe_size: 4096 });
    fake.setFile("wave", { state: "missing" });
    await click(withText(toolCard(host, "Audio"), "button", "Check file"));
    await click(withText(toolCard(host, "Wave"), "button", "Check file"));
    assert.match(toolCard(host, "Audio").textContent ?? "", /NOT the one that was recorded/);
    assert.doesNotMatch(toolCard(host, "Audio").textContent ?? "", /no file at the recorded path/);
    assert.match(toolCard(host, "Wave").textContent ?? "", /There is no file at the recorded path any more\./);
    assert.doesNotMatch(toolCard(host, "Wave").textContent ?? "", /NOT the one that was recorded/);
    unmount();
  });

  test("a start the core refuses leaves the question open with the reason and nothing running", async () => {
    const { fake, host, unmount } = await start();
    await click(withText(host, "button", "Preview the run…"));
    fake.failNext("startRun", "the program could not be started: access is denied");
    await click(withText(dialog(host), "button", "Start the tool"));
    assert.match(alertText(host), /access is denied/);
    assert.ok(dialog(host), "the question is still open");
    absent(host.querySelector("div[aria-label='Tools running now']"), "nothing is running");
    unmount();
  });

  test("while a request is open every button that sends one is disabled, and the page says it is working", async () => {
    const { fake, host, unmount } = await start();
    const release = fake.hold();
    // The click handler does not wait for the backend, so awaiting the click does not wait for the held call.
    await click(withText(toolCard(host), "button", "Check file"));
    assert.match(status(host), /Working…/);
    for (const label of ["Preview the run…", "Record the tool", "Check file", "Show the records kept for this run"]) assert.equal(withText<HTMLButtonElement>(host, "button", label).disabled, true, label);
    release();
    await settle();
    assert.equal(withText<HTMLButtonElement>(host, "button", "Record the tool").disabled, false);
    unmount();
  });

  test("while a start is in flight the question's Start button is disabled, so a second press cannot start a second program", async () => {
    const { fake, host, unmount } = await start();
    await click(withText(host, "button", "Preview the run…"));
    const release = fake.hold();
    const startButton = withText<HTMLButtonElement>(dialog(host), "button", "Start the tool");
    assert.equal(startButton.disabled, false, "before it is pressed it can be pressed");
    await click(startButton);
    assert.match(status(host), /Working…/);
    assert.equal(withText<HTMLButtonElement>(dialog(host), "button", "Start the tool").disabled, true, "pressing it again while the first start is open does nothing");
    await click(withText<HTMLButtonElement>(dialog(host), "button", "Start the tool"));
    release();
    await settle();
    assert.equal(fake.changes().filter((c) => c.op === "startRun").length, 1, "exactly one start was sent");
    noDialog(host);
    unmount();
  });

  test("the records kept for a run are listed with what each did, and records that could not be read are named", async () => {
    const fake = fakeTools(world());
    fake.setStored({
      records: [
        { name: "tool-run-2.json", record: aRunRecord({ success: false, exit_code: 1, added: [] }) },
        { name: "tool-run-1.json", record: aRunRecord() },
      ],
      total: 3,
      unreadable: ["tool-run-0.json: not a record"],
    });
    const { host, unmount } = await mountTracked(<ToolsPanel api={fake.api} pollMs={60_000} />);
    await settle();
    await click(withText(host, "button", "Show the records kept for this run"));
    assert.deepEqual(fake.calls.at(-1), { op: "runRecords", args: [WS] });
    const text = dialog(host).textContent ?? "";
    assert.match(text, /Loud, run run-1 in “Mod library”: 3 records are kept; the newest 2 are shown\./);
    assert.match(text, /tool-run-2\.json.*Failed \(exit code 1\)/);
    assert.match(text, /tool-run-1\.json.*Finished with success/);
    assert.match(one(dialog(host), "ul[aria-label='Record files that could not be read']").textContent ?? "", /tool-run-0\.json: not a record/);
    await click(withText(dialog(host), "button", "Close"));
    noDialog(host);
    unmount();
  });

  test("a run with no records kept says so", async () => {
    const { host, unmount } = await start();
    await click(withText(host, "button", "Show the records kept for this run"));
    assert.match(dialog(host).textContent ?? "", /no record is kept yet\./);
    unmount();
  });

  test("things that could not be listed are shown as problems", async () => {
    const { host, unmount } = await start(world({ run_errors: ["run run-9 of loud could not be read: damaged"] }));
    assert.match(one(host, "ul[aria-label='Things that could not be listed']").textContent ?? "", /run run-9 of loud could not be read: damaged/);
    unmount();
  });

  test("a failed first load shows why instead of an empty page", async () => {
    const fake = fakeTools(world());
    fake.failNext("view", "the tool list is damaged");
    const { host, unmount } = await mountTracked(<ToolsPanel api={fake.api} pollMs={60_000} />);
    await settle();
    assert.match(alertText(host), /tool list is damaged/);
    absent(host.querySelector("form"), "no forms without a view");
    unmount();
  });
});

describe("the tools panel on the Mods page and against the real commands", () => {
  const libraryApi = (): LibraryApi => ({
    list: async () => [],
    scan: async () => ({ games: [], added: 0, updated: 0, returned: 0, missing: 0 }),
    matrix: async () => {
      throw "not used here";
    },
    tick: async () => {
      throw "not used here";
    },
    play: async () => {
      throw "not used here";
    },
  });

  test("it sits after the builder and before the not-built note, and the note no longer names external tools", async () => {
    const tools = fakeTools(world());
    const { host, unmount } = await mountTracked(<ModsView api={fakeMods().api} builderApi={fakeBuilder().api} toolsApi={tools.api} libraryApi={libraryApi()} />);
    await settle();
    const text = host.textContent ?? "";
    assert.ok(text.indexOf("Mod builder and patches") < text.indexOf("External tools"));
    assert.ok(text.indexOf("External tools") < text.indexOf("Not built yet"));
    assert.doesNotMatch(text, /external tools are not built/);
    assert.match(text, /Conflict analysis and measured comparisons are not built at all/);
    unmount();
  });

  test("given fake backends and nothing else, it does not reach for the real tools backend", async () => {
    const calls = installTauri(() => {
      throw "the real tools backend must not be used here";
    });
    const { host, unmount } = await mountTracked(<ModsView libraryApi={libraryApi()} />);
    await settle();
    assert.match(panel(host).textContent ?? "", /Browser preview: external tools need the desktop app/);
    assert.deepEqual(calls.calls.filter((c) => c.cmd.startsWith("tools_")), []);
    calls.remove();
    unmount();
  });

  test("every command name and argument is the one the Rust commands take (camelCase top-level arguments, snake_case objects)", async () => {
    const empty = { tools: [], known: [], workspaces: [], mod_roots: [], running: [], run_errors: [] };
    const record = aRunRecord();
    const answer = (cmd: string) => {
      switch (cmd) {
        case "tools_check":
          return { id: "t", status: { state: "matches" } };
        case "tools_plan_run":
          return { exe: "D:\\t.exe", args: [], cwd: "D:\\w", tool_version: "1", tool_sha256: HEX, problems: [] };
        case "tools_start_run":
          return { run: { run: 1, tool_id: "t", pid: 9, started_unix: 1, args: [], workspace: WS }, view: empty };
        case "tools_poll_run":
          return { state: "finished", record, saved_as: "r.json", save_error: null, view: empty };
        case "tools_run_records":
          return { records: [], total: 0, unreadable: [] };
        default:
          return empty;
      }
    };
    const tauri = installTauri(answer);
    const api = tauriToolsApi();
    assert.ok(api, "inside the webview there is a backend");
    const register = { id: "t", display_name: "T", version: "1", exe_path: "D:\\t.exe", redistribution: "unknown" as const, license_note: "", source_note: "" };
    const run = { tool_id: "t", args: ["-x"], settings: { a: "b" }, workspace: WS };
    await api.view();
    await api.register(register);
    await api.check("t");
    await api.acceptChange("t", "2", HEX);
    await api.planRun(run);
    await api.startRun(run);
    await api.pollRun(1);
    await api.runRecords(WS);
    assert.deepEqual(tauri.calls, [
      { cmd: "tools_view", args: {} },
      { cmd: "tools_register", args: { request: register } },
      { cmd: "tools_check", args: { id: "t" } },
      { cmd: "tools_accept_change", args: { id: "t", newVersion: "2", seenSha256: HEX } },
      { cmd: "tools_plan_run", args: { request: run } },
      { cmd: "tools_start_run", args: { request: run } },
      { cmd: "tools_poll_run", args: { run: 1 } },
      { cmd: "tools_run_records", args: { workspace: WS } },
    ]);
    tauri.remove();
  });

  test("in a plain browser there is no backend", () => {
    assert.equal(tauriToolsApi(), null);
  });

  test("data in the wrong shape is refused with a plain message instead of reaching the page", async () => {
    for (const junk of [[], null, "text", { tools: [] }, { state: "weird" }, { status: { state: "changed" } }]) {
      const tauri = installTauri(() => junk);
      const api = tauriToolsApi()!;
      await assert.rejects(api.view(), /does not understand/);
      await assert.rejects(api.register({ id: "t", display_name: "T", version: "1", exe_path: "x", redistribution: "unknown", license_note: "", source_note: "" }), /does not understand/);
      await assert.rejects(api.check("t"), /does not understand/);
      await assert.rejects(api.acceptChange("t", "2", HEX), /does not understand/);
      await assert.rejects(api.planRun({ tool_id: "t", args: [], settings: {}, workspace: WS }), /does not understand/);
      await assert.rejects(api.startRun({ tool_id: "t", args: [], settings: {}, workspace: WS }), /does not understand/);
      await assert.rejects(api.pollRun(1), /does not understand/);
      await assert.rejects(api.runRecords(WS), /does not understand/);
      tauri.remove();
    }
  });

  test("a poll answer is accepted only in one of its two shapes, each with what the page reads", async () => {
    const empty = { tools: [], known: [], workspaces: [], mod_roots: [], running: [], run_errors: [] };
    const record = aRunRecord();
    const bad: unknown[] = [
      { state: "running" },
      { state: "running", pid: "9" },
      { state: "finished", record, saved_as: null, save_error: null },
      { state: "finished", view: empty, saved_as: null, save_error: null },
      { state: "finished", record: { ...record, added: "none" }, saved_as: null, save_error: null, view: empty },
      { state: "finished", record: { ...record, success: "yes" }, saved_as: null, save_error: null, view: empty },
    ];
    for (const junk of bad) {
      const tauri = installTauri(() => junk);
      await assert.rejects(tauriToolsApi()!.pollRun(1), /does not understand/, JSON.stringify(junk).slice(0, 80));
      tauri.remove();
    }
    const ok = installTauri(() => ({ state: "running", pid: 9 }));
    assert.deepEqual(await tauriToolsApi()!.pollRun(1), { state: "running", pid: 9 });
    ok.remove();
  });

  test("a failing command reaches the page as plain text", async () => {
    const tauri = installTauri(() => {
      throw "the tool list is damaged";
    });
    const { host, unmount } = await mountTracked(<ToolsPanel />);
    await settle();
    assert.match(alertText(host), /tool list is damaged/);
    tauri.remove();
    unmount();
  });
});
