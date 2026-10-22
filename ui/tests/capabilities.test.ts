import { test } from "node:test";
import assert from "node:assert/strict";
import { MOCK_CAPABILITIES, STATUS_LABEL, validateCapability } from "../src/capabilities";
import type { CapabilityEntry } from "../src/types";

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
