import { test } from "node:test";
import assert from "node:assert/strict";
import { MOCK_CAPABILITIES, STATUS_LABEL, STATUS_ORDER, validateCapability, weakestStatus } from "../src/capabilities";
import type { CapabilityEntry, Status } from "../src/types";

const entry = (over: Partial<CapabilityEntry>): CapabilityEntry => ({
  feature: "f", game: "g", build: "*", os: "*", adapterVersion: "0", status: "not_investigated", limitations: [], ...over,
});

test("supported_tested requires date and evidence", () => {
  assert.ok(validateCapability(entry({ status: "supported_tested" })));
  assert.ok(validateCapability(entry({ status: "supported_tested", testDate: "2026-10-22" })));
  assert.equal(validateCapability(entry({ status: "supported_tested", testDate: "2026-10-22", evidence: "logs/x" })), null);
});

test("mock data never overclaims", () => {
  for (const c of MOCK_CAPABILITIES) assert.equal(validateCapability(c), null);
  assert.ok(MOCK_CAPABILITIES.every((c) => c.status === "not_investigated"));
});

test("labels match the spec vocabulary", () => {
  assert.deepEqual(Object.values(STATUS_LABEL), ["Supported and tested", "Partially supported", "Experimental", "Unsupported", "Not investigated"]);
});

test("the legend lists every status once, strongest first", () => {
  assert.deepEqual(STATUS_ORDER, ["supported_tested", "partial", "experimental", "unsupported", "not_investigated"]);
  assert.deepEqual([...STATUS_ORDER].sort(), Object.keys(STATUS_LABEL).sort());
});

test("headline is the weakest status, so a summary never overclaims", () => {
  assert.equal(weakestStatus([]), "not_investigated");
  assert.equal(weakestStatus(["supported_tested"]), "supported_tested");
  assert.equal(weakestStatus(["supported_tested", "partial"]), "partial");
  assert.equal(weakestStatus(["supported_tested", "experimental", "partial"]), "experimental");
  assert.equal(weakestStatus(["supported_tested", "not_investigated"]), "not_investigated");
  // A known "no" is a stronger negative than an unanswered question.
  assert.equal(weakestStatus(["not_investigated", "unsupported", "supported_tested"]), "unsupported");
  // Order of the input never matters.
  const all: Status[] = [...STATUS_ORDER];
  assert.equal(weakestStatus(all), weakestStatus([...all].reverse()));
  assert.equal(weakestStatus(new Set<Status>(["partial", "experimental"])), "experimental");
});
