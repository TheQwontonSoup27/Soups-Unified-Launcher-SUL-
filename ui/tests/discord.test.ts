import { test } from "node:test";
import assert from "node:assert/strict";
import { MAX_ASSET_KEY, loadPresenceWanted, loadSmallImage, normalizeAssetKey, setPresence } from "../src/discord";

test("presence is opt-in: nothing is wanted until the user switches it on", () => {
  assert.equal(loadPresenceWanted(), false);
  assert.equal(loadSmallImage(), "");
});

test("setPresence: preview mode, success, and error paths", async () => {
  assert.deepEqual(await setPresence(false, null), { on: false, message: "" });
  assert.equal((await setPresence(true, null)).on, true);
  assert.match((await setPresence(true, null)).message, /Preview only/);
  assert.deepEqual(await setPresence(true, async () => "on"), { on: true, message: "" });
  assert.deepEqual(await setPresence(false, async () => "off"), { on: false, message: "" });
  const failed = await setPresence(true, async () => { throw "Discord does not appear to be running"; });
  assert.deepEqual(failed, { on: false, message: "Discord does not appear to be running" });
  const weird = await setPresence(true, async () => { throw new Error("x"); });
  assert.equal(weird.on, false);
});

test("setPresence sends the command name and the small-image asset key", async () => {
  const calls: Array<[string, Record<string, unknown>]> = [];
  const invoke = async (cmd: string, args: Record<string, unknown>) => { calls.push([cmd, args]); return "on"; };
  await setPresence(true, invoke, "  team27_corner ");
  await setPresence(true, invoke);
  await setPresence(true, invoke, "bad\nkey");
  assert.deepEqual(calls[0], ["discord_set_enabled", { enabled: true, smallImage: "team27_corner" }]);
  assert.deepEqual(calls[1], ["discord_set_enabled", { enabled: true, smallImage: undefined }]);
  assert.deepEqual(calls[2], ["discord_set_enabled", { enabled: true, smallImage: undefined }], "an unusable key is dropped, not sent");
});

test("asset keys are trimmed, printable and bounded (same rules as sul-core)", () => {
  assert.equal(normalizeAssetKey("  corner  "), "corner");
  assert.equal(normalizeAssetKey(""), "");
  assert.equal(normalizeAssetKey("a\u0007b"), "");
  assert.equal(normalizeAssetKey("k".repeat(MAX_ASSET_KEY)).length, MAX_ASSET_KEY);
  assert.equal(normalizeAssetKey("k".repeat(MAX_ASSET_KEY + 1)), "");
});
