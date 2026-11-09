import { test } from "node:test";
import assert from "node:assert/strict";
import {
  PlayController,
  describeLaunchState,
  describeRecord,
  formatUtc,
  holdReason,
  isAnythingRunning,
  isLaunchRecord,
  isLaunchView,
  MAX_FINISHED_SHOWN,
  type LaunchRecord,
} from "../src/lib/playing";
import { aBuild, aRefusal, aView, fakePlay } from "./play-fake";

const BUILD = "gog-1-experimental-1";

async function loaded(view = aView()) {
  const fake = fakePlay(view);
  const c = new PlayController(fake.api);
  await c.load();
  return { fake, c };
}

// ---------------------------------------------------------------------------
// Words and rules
// ---------------------------------------------------------------------------

test("a time is written in UTC to the minute", () => {
  assert.equal(formatUtc(1_700_000_000), "2023-11-14 22:13 UTC");
});

test("a game's state is described in plain words and a failed exit never claims a cause", () => {
  assert.equal(describeLaunchState({ state: "not_running" }), "Not running");
  assert.equal(describeLaunchState({ state: "running", pid: 7, build_id: "b", since_unix: 1_700_000_000 }), "Running (process 7) since 2023-11-14 22:13 UTC");
  assert.match(describeLaunchState({ state: "last_exited", code: 0, success: true, ended_unix: 1_700_000_000 }), /^Last run ended normally at/);
  assert.match(describeLaunchState({ state: "last_exited", code: 3, success: false, ended_unix: 1_700_000_000 }), /^Last run ended with exit code 3 at/);
  assert.match(describeLaunchState({ state: "last_exited", code: null, success: false, ended_unix: 1_700_000_000 }), /^Last run ended with no exit code at/);
  assert.equal(describeLaunchState({ state: "last_not_started", explanation: "x" }), "Last start was refused");
});

test("a record is described in one line, and a refusal gives the core's own sentence", () => {
  const running: LaunchRecord = { ...aRefusal("b", { reason: "no_adapter" }), outcome: { kind: "running", pid: 9 } };
  assert.equal(describeRecord(running), "Started build b (process 9).");
  const ok: LaunchRecord = { ...running, outcome: { kind: "exited", code: 0, success: true, explanation: null } };
  assert.equal(describeRecord(ok), "Build b ended normally.");
  const bad: LaunchRecord = { ...running, outcome: { kind: "exited", code: 3, success: false, explanation: "SUL cannot tell why." } };
  assert.equal(describeRecord(bad), "Build b ended with exit code 3. SUL cannot tell why.");
  const none: LaunchRecord = { ...running, outcome: { kind: "exited", code: null, success: false, explanation: null } };
  assert.equal(describeRecord(none), "Build b ended with no exit code.");
  assert.match(describeRecord(aRefusal("b", { reason: "no_adapter" })), /^Build b was not started: There is no adapter registered/);
});

test("the Play button is held, with the reason, for a game with no adapter, a build that is not ready and a game that already runs", () => {
  const view = aView([aBuild("a"), aBuild("b", { has_adapter: false }), aBuild("c", { state: "failed" }), aBuild("d", { game_id: "gog-2", slot: "original" })]);
  const by = (id: string) => view.builds.find((b) => b.build_id === id)!;
  assert.equal(holdReason(by("a"), view), null);
  assert.match(holdReason(by("b"), view)!, /no adapter for this game/);
  assert.match(holdReason(by("c"), view)!, /Not ready: does not match its manifest/);

  const running = { ...view, games: view.games.map((g) => (g.game_id === "gog-1" ? { ...g, state: { state: "running" as const, pid: 1, build_id: "a", since_unix: 1 } } : g)) };
  assert.equal(holdReason(by("a"), running), "Running.");
  const other = aBuild("e");
  assert.match(holdReason(other, running)!, /Build a of this game is already running/);
  assert.equal(holdReason(by("d"), running), null, "another game is not held by this one");
  assert.equal(isAnythingRunning(running), true);
  assert.equal(isAnythingRunning(view), false);
  assert.equal(isAnythingRunning(null), false);
});

test("data of the wrong shape is not accepted as a view or a record", () => {
  assert.equal(isLaunchView(aView()), true);
  for (const junk of [null, [], "x", {}, { builds: [], games: [] }]) assert.equal(isLaunchView(junk), false);
  assert.equal(isLaunchRecord(aRefusal("b", { reason: "no_adapter" })), true);
  for (const junk of [null, {}, { launch_id: "x" }, { launch_id: "x", build_id: "b", outcome: {}, warnings: [] }]) assert.equal(isLaunchRecord(junk), false);
});

// ---------------------------------------------------------------------------
// The controller
// ---------------------------------------------------------------------------

test("with no backend the page is loaded, empty, and starts nothing", async () => {
  const c = new PlayController(null);
  assert.equal(c.hasBackend, false);
  await c.load();
  assert.equal(c.getSnapshot().loaded, true);
  assert.equal(c.getSnapshot().view, null);
  assert.equal(await c.start(BUILD, false), false);
  await c.pollRunning();
});

test("a view that cannot be read says why and leaves the page loaded with no data", async () => {
  const fake = fakePlay();
  fake.failNext("view", "the launch manager is damaged");
  const c = new PlayController(fake.api);
  await c.load();
  assert.equal(c.getSnapshot().loaded, true);
  assert.equal(c.getSnapshot().view, null);
  assert.match(c.getSnapshot().error ?? "", /damaged/);
});

test("a blank build is refused on the page and nothing is sent", async () => {
  const { fake, c } = await loaded();
  assert.equal(await c.start("  ", false), false);
  assert.match(c.getSnapshot().error ?? "", /Pick a build to start/);
  assert.equal(fake.starts().length, 0);
});

test("a start sends the build and the output choice exactly, and the view says the game runs", async () => {
  const { fake, c } = await loaded();
  assert.equal(await c.start(BUILD, true), true);
  assert.deepEqual(fake.starts(), [{ op: "start", args: [BUILD, true] }]);
  assert.equal(c.getSnapshot().view!.games[0]!.state.state, "running");
  assert.equal(c.getSnapshot().notice, `Started build ${BUILD} (process 4242).`);
  assert.equal(c.getSnapshot().error, null);
  assert.equal(c.getSnapshot().busy, null);
});

test("the warnings of a start that went ahead are shown, and are cleared by the next start", async () => {
  const { fake, c } = await loaded(aView([aBuild(BUILD, { managed: false })]));
  fake.warnWith(["The program differs from the one recorded."]);
  await c.start(BUILD, false);
  assert.deepEqual(c.getSnapshot().warnings, ["The program differs from the one recorded."]);
  fake.endsAfter("gog-1", 0);
  await c.pollRunning();
  // The next start fails: the warnings of the earlier start must not stay beside this new failure.
  fake.failNext("start", "the launcher stopped working; restart the app");
  assert.equal(await c.start(BUILD, false), false);
  assert.deepEqual(c.getSnapshot().warnings, []);
  // And one that goes ahead with nothing to warn about shows none.
  fake.warnWith([]);
  assert.equal(await c.start(BUILD, false), true);
  assert.deepEqual(c.getSnapshot().warnings, []);
});

test("a refusal is shown as the error in the core's own words, kept in the history, and is not a failure of the page", async () => {
  const { c } = await loaded(aView([aBuild(BUILD, { has_adapter: false })]));
  assert.equal(await c.start(BUILD, false), false);
  assert.match(c.getSnapshot().error ?? "", /There is no adapter registered for this game/);
  assert.equal(c.getSnapshot().notice, null);
  assert.equal(c.getSnapshot().view!.history.length, 1, "the refusal is in the history");
  assert.equal(c.getSnapshot().view!.games[0]!.state.state, "last_not_started");
  assert.equal(c.getSnapshot().busy, null);
});

test("an unexpected failure says why and leaves the view as it was", async () => {
  const { fake, c } = await loaded();
  const before = JSON.stringify(c.getSnapshot().view);
  fake.failNext("start", "the launcher stopped working; restart the app");
  assert.equal(await c.start(BUILD, false), false);
  assert.match(c.getSnapshot().error ?? "", /launcher stopped working/);
  assert.equal(JSON.stringify(c.getSnapshot().view), before);
  assert.equal(c.getSnapshot().busy, null);
});

test("only one start runs at a time: a second request while one is working is ignored", async () => {
  const { fake, c } = await loaded();
  const release = fake.hold();
  const first = c.start(BUILD, false);
  assert.equal(c.getSnapshot().busy, "start");
  assert.equal(await c.start(BUILD, false), false);
  release();
  assert.equal(await first, true);
  assert.equal(fake.starts().length, 1);
});

test("polling asks only while something runs, and reports the launch that ended once", async () => {
  const { fake, c } = await loaded();
  await c.pollRunning();
  assert.equal(fake.calls.filter((x) => x.op === "refresh").length, 0, "nothing runs, so nothing is asked");

  await c.start(BUILD, false);
  fake.endsAfter("gog-1", 1, 0);
  await c.pollRunning();
  assert.equal(c.getSnapshot().finished.length, 0, "not ended yet");
  assert.equal(c.getSnapshot().view!.games[0]!.state.state, "running");
  await c.pollRunning();
  assert.equal(c.getSnapshot().finished.length, 1);
  assert.equal(c.getSnapshot().view!.games[0]!.state.state, "last_exited");
  assert.match(c.getSnapshot().notice ?? "", /ended normally/);
  const asked = fake.calls.filter((x) => x.op === "refresh").length;
  await c.pollRunning();
  assert.equal(fake.calls.filter((x) => x.op === "refresh").length, asked, "an ended game is not asked about again");
});

test("a poll is skipped while an earlier poll is still open", async () => {
  const { fake, c } = await loaded();
  await c.start(BUILD, false);
  fake.endsAfter("gog-1", 100);
  const release = fake.hold();
  const first = c.pollRunning();
  await c.pollRunning(); // the earlier poll is still open: skipped
  release();
  await first;
  assert.equal(fake.calls.filter((x) => x.op === "refresh").length, 1);
});

test("a poll is skipped while a start is working", async () => {
  const { fake, c } = await loaded(aView([aBuild(BUILD), aBuild("gog-2-original-1", { game_id: "gog-2", slot: "original" })]));
  await c.start(BUILD, false);
  fake.endsAfter("gog-1", 100);
  const release = fake.hold();
  const starting = c.start("gog-2-original-1", false);
  assert.equal(c.getSnapshot().busy, "start");
  await c.pollRunning();
  release();
  await starting;
  assert.equal(fake.calls.filter((x) => x.op === "refresh").length, 0, "the poll waited for its turn and was dropped");
});

test("a failed poll is shown without discarding the view", async () => {
  const { fake, c } = await loaded();
  await c.start(BUILD, false);
  fake.failNext("refresh", "the launch manager could not ask");
  await c.pollRunning();
  assert.match(c.getSnapshot().error ?? "", /could not ask/);
  assert.equal(c.getSnapshot().view!.games[0]!.state.state, "running");
});

test("only the newest finished launches are kept for the page", async () => {
  const { fake, c } = await loaded();
  for (let i = 0; i < MAX_FINISHED_SHOWN + 3; i += 1) {
    fake.endsAfter("gog-1", 0);
    await c.start(BUILD, false);
    await c.pollRunning();
  }
  assert.equal(c.getSnapshot().finished.length, MAX_FINISHED_SHOWN);
});

test("listeners hear every change and stop hearing after they unsubscribe", async () => {
  const fake = fakePlay();
  const c = new PlayController(fake.api);
  let heard = 0;
  const off = c.subscribe(() => (heard += 1));
  await c.load();
  assert.ok(heard >= 1);
  const after = heard;
  off();
  await c.start(BUILD, false);
  assert.equal(heard, after);
});
