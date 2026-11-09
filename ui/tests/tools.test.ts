import { test } from "node:test";
import assert from "node:assert/strict";
import {
  EMPTY_REGISTER,
  MAX_FINISHED_SHOWN,
  REDISTRIBUTION_LABEL,
  ToolsController,
  describeRecord,
  describeToolStatus,
  isToolsView,
  parseArgs,
  parseSettings,
  requestOf,
  shortHash,
  toolName,
  workspaceKey,
  workspaceLabel,
  type RegisterForm,
} from "../src/lib/tools";
import { HEX, OTHER_HEX, aKnown, aRunRecord, aTool, aWorkspace, fakeTools, modRoot } from "./tools-fake";

/** A view with one recorded tool, one finished recipe run to work in, a mod library and one known tool. */
function world(extra: Parameters<typeof fakeTools>[0] = {}) {
  return fakeTools({
    tools: [aTool("audio", { display_name: "Audio", version: "1.0" })],
    workspaces: [aWorkspace()],
    mod_roots: [modRoot()],
    known: [aKnown()],
    ...extra,
  });
}

const WS = { recipe_id: "loud", root_id: "modlib", run_id: "run-1" };
const form = (extra: Partial<RegisterForm> = {}): RegisterForm => ({ ...EMPTY_REGISTER, display_name: "Wave Tool", version: "2.1", exe_path: "D:\\Tools\\wave.exe", ...extra });

async function loaded(fake = world()) {
  const c = new ToolsController(fake.api);
  await c.load();
  return { fake, c, state: () => c.getSnapshot() };
}

// ---------------------------------------------------------------------------
// Typed text
// ---------------------------------------------------------------------------

test("arguments are one per line, blank lines dropped, and a line is kept exactly as typed", () => {
  assert.deepEqual(parseArgs("--in\n  data dir  \n\n\r\n--flag\r\n"), ["--in", "  data dir  ", "--flag"]);
  assert.deepEqual(parseArgs(""), []);
  assert.deepEqual(parseArgs("   \n\t"), []);
});

test("settings are name = value lines, and a line that is wrong says which one and why", () => {
  assert.deepEqual(parseSettings("bank = music\n\nrate=48000\nnote = a = b"), { settings: { bank: "music", rate: "48000", note: "a = b" } });
  assert.deepEqual(parseSettings(""), { settings: {} });
  const bad = (t: string) => {
    const r = parseSettings(t);
    assert.ok("bad" in r, `expected a refusal for ${JSON.stringify(t)}`);
    return r.bad;
  };
  assert.match(bad("bank music"), /Line 1: write a name, an equals sign and a value/);
  assert.match(bad("a = 1\n = 2"), /Line 2: the name before the equals sign is empty/);
  assert.match(bad("a = 1\na = 2"), /Line 2: “a” is written twice/);
});

test("a settings line named like an object property cannot slip past the duplicate check or poison the result", () => {
  const r = parseSettings("constructor = x\n__proto__ = y\ntoString = z");
  assert.ok("settings" in r);
  assert.deepEqual(Object.keys(r.settings).sort(), ["__proto__", "constructor", "toString"].sort());
  assert.equal(Object.getPrototypeOf(r.settings), Object.prototype, "no setting changed the prototype of the result");
  assert.ok("bad" in parseSettings("constructor = x\nconstructor = y"));
});

// ---------------------------------------------------------------------------
// Words
// ---------------------------------------------------------------------------

test("a tool's file status is said in plain words, and a changed file says it will not be run", () => {
  assert.equal(describeToolStatus({ state: "matches" }), "The file is the one that was recorded.");
  assert.equal(describeToolStatus({ state: "missing" }), "There is no file at the recorded path any more.");
  assert.match(describeToolStatus({ state: "changed", exe_sha256: OTHER_HEX, exe_size: 1500 }), /NOT the one that was recorded.*1\.5 KB.*cdcdcdcdcdcd….*will not run it until you accept it/);
});

test("a finished run is described by how it ended and what it did to the workspace folder; a folder that could not be compared says so", () => {
  assert.equal(describeRecord(aRunRecord()), "Finished with success. In the workspace folder: 1 file added, 0 files changed, 0 files removed.");
  assert.match(describeRecord(aRunRecord({ success: false, exit_code: 3 })), /^Failed \(exit code 3\)\./);
  assert.match(describeRecord(aRunRecord({ success: false, exit_code: null })), /^Ended without an exit code\./);
  const unseen = describeRecord(aRunRecord({ comparison_error: "a link inside the folder" }));
  assert.match(unseen, /could not be compared, so this record does not say what changed/);
  assert.doesNotMatch(unseen, /added/, "no counts are claimed when nothing could be seen");
});

test("names and places: a tool by its display name, a run by its recipe, number and library, with the id as a fallback", () => {
  const view = world().current();
  assert.equal(toolName(view, "audio"), "Audio");
  assert.equal(toolName(view, "gone"), "gone");
  assert.equal(workspaceLabel(view, WS), "Loud, run run-1 in “Mod library”");
  assert.equal(workspaceLabel(view, { recipe_id: "x", root_id: "r", run_id: "q" }), "x, run q in “r”");
  assert.notEqual(workspaceKey({ recipe_id: "a b", root_id: "", run_id: "c" }), workspaceKey({ recipe_id: "a", root_id: "b", run_id: "c" }));
  assert.equal(shortHash("abc"), "abc");
  assert.equal(shortHash(HEX), "abababababab…");
  assert.match(REDISTRIBUTION_LABEL.unknown, /assumes nothing/);
  assert.match(REDISTRIBUTION_LABEL.permitted, /your word; SUL does not check/);
});

// ---------------------------------------------------------------------------
// The registration form
// ---------------------------------------------------------------------------

test("a form becomes a request with the id made from the name and every text trimmed", () => {
  const r = requestOf(form({ display_name: "  Wave  Tool! ", version: " 2.1 ", exe_path: "  D:\\Tools\\wave.exe ", license_note: " MIT? ", source_note: " a site ", redistribution: "not_permitted" }));
  assert.deepEqual(r, {
    request: { id: "wave-tool", display_name: "Wave  Tool!", version: "2.1", exe_path: "D:\\Tools\\wave.exe", redistribution: "not_permitted", license_note: "MIT?", source_note: "a site" },
  });
});

test("a form with something missing is refused with what to fix", () => {
  const bad = (f: RegisterForm) => {
    const r = requestOf(f);
    assert.ok("bad" in r);
    return r.bad;
  };
  assert.match(bad(form({ display_name: " " })), /Give the tool a name/);
  assert.match(bad(form({ display_name: "!!!" })), /some letters or digits/);
  assert.match(bad(form({ version: "" })), /Write the version you installed/);
  assert.match(bad(form({ exe_path: " " })), /Write the full path of the program/);
});

test("only data in the shape of a tools view is accepted", () => {
  assert.equal(isToolsView(world().current()), true);
  for (const junk of [null, [], "x", {}, { tools: [], known: [], workspaces: [], mod_roots: [], running: [] }, { tools: {}, known: [], workspaces: [], mod_roots: [], running: [], run_errors: [] }]) {
    assert.equal(isToolsView(junk), false, JSON.stringify(junk));
  }
});

// ---------------------------------------------------------------------------
// The controller
// ---------------------------------------------------------------------------

test("with no backend the controller is loaded, empty, and does nothing", async () => {
  const c = new ToolsController(null);
  assert.equal(c.hasBackend, false);
  await c.load();
  assert.equal(c.getSnapshot().loaded, true);
  assert.equal(c.getSnapshot().view, null);
  assert.equal(await c.registerTool(form()), false);
  assert.equal(await c.checkTool("audio"), false);
  assert.equal(await c.previewRun("audio", WS, "", ""), false);
  assert.equal(await c.confirmRun(), false);
  await c.pollRunning();
});

test("loading shows the tools and what is known, and a failed load says why and keeps the page usable", async () => {
  const { state } = await loaded();
  assert.equal(state().view?.tools.length, 1);
  assert.equal(state().view?.known.length, 1);
  const fake = world();
  fake.failNext("view", "the tool list is damaged");
  const c = new ToolsController(fake.api);
  await c.load();
  assert.equal(c.getSnapshot().loaded, true);
  assert.equal(c.getSnapshot().view, null);
  assert.match(c.getSnapshot().error ?? "", /tool list is damaged/);
});

test("recording a tool sends exactly the typed request and says that nothing was copied, downloaded or started", async () => {
  const { fake, c, state } = await loaded();
  assert.equal(await c.registerTool(form({ license_note: "mine", redistribution: "permitted" })), true);
  assert.deepEqual(fake.changes(), [
    { op: "register", args: [{ id: "wave-tool", display_name: "Wave Tool", version: "2.1", exe_path: "D:\\Tools\\wave.exe", redistribution: "permitted", license_note: "mine", source_note: "" }] },
  ]);
  assert.equal(state().view?.tools.length, 2);
  assert.match(state().notice ?? "", /Recorded “Wave Tool” 2\.1\. SUL read the file once to hash it\. Nothing was copied, downloaded or started\./);
});

test("a form the page can see is wrong sends nothing; one the core refuses shows its reason and changes nothing", async () => {
  const { fake, c, state } = await loaded();
  const before = fake.calls.length;
  assert.equal(await c.registerTool(form({ version: "" })), false);
  assert.equal(fake.calls.length, before);
  assert.match(state().error ?? "", /Write the version you installed/);
  assert.equal(await c.registerTool(form({ display_name: "Audio" })), false);
  assert.match(state().error ?? "", /already recorded/);
  assert.equal(state().view?.tools.length, 1);
  assert.equal(state().busy, null);
});

test("a check is kept per tool, and a changed file shows the hash it found", async () => {
  const { fake, c, state } = await loaded();
  fake.setFile("audio", { state: "changed", exe_sha256: OTHER_HEX, exe_size: 99 });
  assert.equal(await c.checkTool("audio"), true);
  assert.deepEqual(state().checks.audio, { state: "changed", exe_sha256: OTHER_HEX, exe_size: 99 });
  assert.deepEqual(fake.changes(), [], "a check changes nothing");
});

test("the check of each tool is kept under that tool, so two tools never share one", async () => {
  const fake = world({ tools: [aTool("audio", { display_name: "Audio" }), aTool("wave", { display_name: "Wave" })] });
  const { c, state } = await loaded(fake);
  fake.setFile("audio", { state: "changed", exe_sha256: OTHER_HEX, exe_size: 99 });
  fake.setFile("wave", { state: "missing" });
  await c.checkTool("audio");
  await c.checkTool("wave");
  assert.deepEqual(state().checks, { audio: { state: "changed", exe_sha256: OTHER_HEX, exe_size: 99 }, wave: { state: "missing" } });
});

test("a change cannot be accepted before a check has shown it, or without a version: nothing is sent", async () => {
  const { fake, c, state } = await loaded();
  const before = fake.calls.length;
  assert.equal(await c.acceptChange("audio", "2.0"), false);
  assert.match(state().error ?? "", /Check the tool first/);
  fake.setFile("audio", { state: "changed", exe_sha256: OTHER_HEX, exe_size: 99 });
  await c.checkTool("audio");
  const afterCheck = fake.calls.length;
  assert.equal(await c.acceptChange("audio", "  "), false);
  assert.match(state().error ?? "", /Write the version/);
  assert.equal(fake.calls.length, afterCheck, "no accept was sent");
  assert.ok(afterCheck > before);
  fake.setFile("audio", { state: "matches" });
  await c.checkTool("audio");
  assert.equal(await c.acceptChange("audio", "2.0"), false, "a file that matches has nothing to accept");
  assert.deepEqual(fake.changes(), []);
});

test("accepting names the hash the check showed, records the version, and drops the stale check", async () => {
  const { fake, c, state } = await loaded();
  fake.setFile("audio", { state: "changed", exe_sha256: OTHER_HEX, exe_size: 99 });
  await c.checkTool("audio");
  assert.equal(await c.acceptChange("audio", " 2.0 "), true);
  assert.deepEqual(fake.changes(), [{ op: "acceptChange", args: ["audio", "2.0", OTHER_HEX] }]);
  assert.equal(state().view?.tools[0]?.version, "2.0");
  assert.equal(state().view?.tools[0]?.history.length, 1, "the earlier file is kept in the history");
  assert.equal("audio" in state().checks, false);
  assert.match(state().notice ?? "", /version 2\.0.*earlier one stays/);
});

test("a file that changed again after the check is refused by the core and the page keeps its old view", async () => {
  const { fake, c, state } = await loaded();
  fake.setFile("audio", { state: "changed", exe_sha256: OTHER_HEX, exe_size: 99 });
  await c.checkTool("audio");
  fake.setFile("audio", { state: "changed", exe_sha256: "ef".repeat(32), exe_size: 100 });
  assert.equal(await c.acceptChange("audio", "2.0"), false);
  assert.match(state().error ?? "", /no longer the one you were shown/);
  assert.equal(state().view?.tools[0]?.version, "1.0");
  assert.equal(state().checks.audio?.state, "changed", "the check stays so the person can look again");
});

test("a preview sends only the plan request, with no environment and the exact workspace: nothing starts", async () => {
  const { fake, c, state } = await loaded();
  assert.equal(await c.previewRun("audio", WS, "--in\ndata", "bank = music"), true);
  assert.deepEqual(fake.calls.map((x) => x.op), ["view", "planRun"]);
  assert.deepEqual(fake.calls[1]!.args, [{ tool_id: "audio", args: ["--in", "data"], settings: { bank: "music" }, workspace: WS }]);
  assert.deepEqual(Object.keys(fake.calls[1]!.args[0] as object).sort(), ["args", "settings", "tool_id", "workspace"], "the page cannot send an environment");
  assert.deepEqual(fake.changes(), []);
  assert.equal(state().preview?.plan.exe, "D:\\Tools\\audio\\audio.exe");
  c.closePreview();
  assert.equal(state().preview, null);
  assert.deepEqual(fake.changes(), [], "cancelling starts nothing");
});

test("a preview the page can see is wrong sends nothing: no tool, no finished run, a bad settings line", async () => {
  const { fake, c, state } = await loaded();
  const before = fake.calls.length;
  assert.equal(await c.previewRun("", WS, "", ""), false);
  assert.match(state().error ?? "", /Record a tool first/);
  assert.equal(await c.previewRun("audio", null, "", ""), false);
  assert.match(state().error ?? "", /Pick the finished run/);
  assert.equal(await c.previewRun("audio", WS, "", "no equals sign"), false);
  assert.match(state().error ?? "", /Line 1/);
  assert.equal(fake.calls.length, before);
});

test("confirming starts exactly the previewed run, once, and says it cannot be cancelled from here", async () => {
  const { fake, c, state } = await loaded();
  await c.previewRun("audio", WS, "--go", "");
  assert.equal(await c.confirmRun(), true);
  assert.deepEqual(fake.changes(), [{ op: "startRun", args: [{ tool_id: "audio", args: ["--go"], settings: {}, workspace: WS }] }]);
  assert.equal(state().preview, null);
  assert.equal(state().view?.running.length, 1);
  assert.match(state().notice ?? "", /Started “Audio” \(process 4001\)\. It runs on its own: SUL cannot cancel it from here, and it keeps going if this window is closed\./);
  assert.equal(await c.confirmRun(), false, "there is no preview left to confirm");
  assert.equal(fake.changes().length, 1);
});

test("a blocked plan cannot be started from the page: no start is sent and the preview stays open to read", async () => {
  const fake = world();
  fake.setPlanProblems(["the workspace folder holds a link"]);
  const { c, state } = await loaded(fake);
  await c.previewRun("audio", WS, "", "");
  assert.deepEqual(state().preview?.plan.problems, ["the workspace folder holds a link"]);
  assert.equal(await c.confirmRun(), false);
  assert.deepEqual(fake.changes(), []);
  assert.match(state().error ?? "", /run is blocked, so nothing was started/);
  assert.ok(state().preview);
});

test("a tool whose file changed is shown as blocked in the preview and is not started even if the page sends the start", async () => {
  const { fake, c, state } = await loaded();
  fake.setFile("audio", { state: "changed", exe_sha256: OTHER_HEX, exe_size: 99 });
  await c.previewRun("audio", WS, "", "");
  assert.match(state().preview?.plan.problems.join(" ") ?? "", /not the one that was recorded/);
  assert.equal(await c.confirmRun(), false);
  assert.deepEqual(fake.changes(), []);
});

test("a start the core refuses leaves the preview open and nothing running, with the reason", async () => {
  const { fake, c, state } = await loaded();
  await c.previewRun("audio", WS, "", "");
  fake.failNext("startRun", "the program could not be started: access is denied");
  assert.equal(await c.confirmRun(), false);
  assert.match(state().error ?? "", /access is denied/);
  assert.equal(state().view?.running.length, 0);
  assert.ok(state().preview);
  assert.equal(state().busy, null);
});

test("one operation at a time: a second request while one is running is ignored", async () => {
  const { fake, c, state } = await loaded();
  const release = fake.hold();
  const first = c.previewRun("audio", WS, "", "");
  assert.equal(state().busy, "preview");
  assert.equal(await c.previewRun("audio", WS, "", ""), false);
  assert.equal(await c.checkTool("audio"), false);
  release();
  assert.equal(await first, true);
  assert.equal(fake.calls.filter((x) => x.op === "planRun").length, 1);
});

async function started(args = "") {
  const ctx = await loaded();
  await ctx.c.previewRun("audio", WS, args, "");
  await ctx.c.confirmRun();
  return ctx;
}

test("polling a tool that is still running changes nothing and asks only about that run", async () => {
  const { fake, c, state } = await started();
  await c.pollRunning();
  assert.deepEqual(fake.calls.at(-1), { op: "pollRun", args: [1] });
  assert.equal(state().view?.running.length, 1);
  assert.deepEqual(state().finished, []);
});

test("a tool that finished is listed with its record, leaves the running list and is reported", async () => {
  const { fake, c, state } = await started();
  fake.finish(1, aRunRecord({ added: [], changed: [{ path: "data/a.bin", before_sha256: HEX, after_sha256: OTHER_HEX }] }));
  await c.pollRunning();
  assert.equal(state().view?.running.length, 0);
  assert.equal(state().finished.length, 1);
  assert.equal(state().finished[0]?.saved_as, "run-1.json");
  assert.deepEqual(state().finished[0]?.workspace, WS);
  assert.match(state().notice ?? "", /“Audio” finished\. Finished with success\. In the workspace folder: 0 files added, 1 file changed, 0 files removed\./);
});

test("a record that could not be kept is reported as an error while the true record is still shown", async () => {
  const { fake, c, state } = await started();
  fake.finish(1);
  const poll = fake.api.pollRun.bind(fake.api);
  const api = {
    ...fake.api,
    pollRun: async (run: number) => {
      const r = await poll(run);
      return r.state === "finished" ? { ...r, saved_as: null, save_error: "the disk is full" } : r;
    },
  };
  const c2 = new ToolsController(api);
  await c2.load();
  await c2.pollRunning();
  assert.match(c2.getSnapshot().error ?? "", /could not be kept: the disk is full/);
  assert.equal(c2.getSnapshot().finished[0]?.save_error, "the disk is full");
  assert.equal(c2.getSnapshot().finished[0]?.record.success, true);
  void c;
  void state;
});

test("a run the core no longer follows is explained, and the list of what is still followed is refreshed", async () => {
  const { fake, c, state } = await started();
  fake.failNext("pollRun", "SUL lost track of tool run 1: the process handle is gone. The program may still be running, and nothing was recorded");
  await c.pollRunning();
  assert.match(state().error ?? "", /lost track of tool run 1/);
  assert.equal(fake.calls.at(-1)?.op, "view", "the view is read again so it matches the core");
});

test("polling does nothing while another request is open, while an earlier poll is open, or when nothing is running", async () => {
  const idle = await loaded();
  await idle.c.pollRunning();
  assert.deepEqual(idle.fake.calls.map((x) => x.op), ["view"], "nothing running, nothing asked");

  const { fake, c } = await started();
  const release = fake.hold();
  const check = c.checkTool("audio");
  await c.pollRunning();
  assert.equal(fake.calls.filter((x) => x.op === "pollRun").length, 0, "a request is open");
  release();
  await check;

  const release2 = fake.hold();
  const first = c.pollRunning();
  await c.pollRunning();
  release2();
  await first;
  assert.equal(fake.calls.filter((x) => x.op === "pollRun").length, 1, "a second poll does not start while the first is open");
});

test("the list of runs finished in this window is bounded", async () => {
  const { fake, c, state } = await loaded();
  for (let i = 0; i < MAX_FINISHED_SHOWN + 3; i += 1) {
    await c.previewRun("audio", WS, "", "");
    await c.confirmRun();
    fake.finish(i + 1);
    await c.pollRunning();
  }
  assert.equal(state().finished.length, MAX_FINISHED_SHOWN);
});

test("the records kept for a run are read on request and can be closed; a run that is not a finished one is refused", async () => {
  const fake = world();
  fake.setStored({ records: [{ name: "run-1.json", record: aRunRecord() }], total: 1, unreadable: [] });
  const { c, state } = await loaded(fake);
  assert.equal(await c.loadRecords(WS), true);
  assert.deepEqual(fake.calls.at(-1), { op: "runRecords", args: [WS] });
  assert.equal(state().records?.list.total, 1);
  c.closeRecords();
  assert.equal(state().records, null);
  assert.equal(await c.loadRecords({ ...WS, run_id: "nope" }), false);
  assert.match(state().error ?? "", /not a finished run/);
});
