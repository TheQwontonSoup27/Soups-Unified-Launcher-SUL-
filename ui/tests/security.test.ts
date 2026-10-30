// The Security & Trust page's statements: shaped so that none of them can say more than was checked.
import assert from "node:assert/strict";
import { describe, test } from "node:test";
import { STATUS_LABEL } from "../src/capabilities";
import { ALL_ROWS, CHECKED_ON, CREDENTIALS, DATA_SHARING, PROTECTIONS, RULES, RULE_TAG, SENDERS, headlineStatus } from "../src/security";

const everyText = () => [
  ...RULES.flatMap((r) => [r.title, r.statement, r.evidence]),
  ...ALL_ROWS.flatMap((r) => [r.title, r.text, r.limitation ?? ""]),
  RULE_TAG,
];

describe("the rows", () => {
  test("there are five rows with unique ids, each with a real capability label", () => {
    assert.equal(ALL_ROWS.length, 5);
    assert.equal(new Set([...ALL_ROWS, ...RULES].map((r) => r.id)).size, ALL_ROWS.length + RULES.length);
    for (const row of ALL_ROWS) assert.ok(row.status in STATUS_LABEL, `${row.id}: ${row.status}`);
    assert.deepEqual(SENDERS.map((r) => r.id), ["rich-presence", "webhooks"]);
    assert.deepEqual(PROTECTIONS.map((r) => r.id), ["package-scanning", "quarantine", "integrity-checks"]);
  });

  test("nothing claims Supported and tested, and a row stronger than Not investigated must say what limits it", () => {
    for (const row of ALL_ROWS) {
      assert.notEqual(row.status, "supported_tested", `${row.id} has no test evidence`);
      if (row.status !== "not_investigated") assert.ok(row.limitation?.trim(), `${row.id} is ${row.status} and states no limit`);
    }
  });

  test("the three protections are Not investigated and say they are not built", () => {
    for (const row of PROTECTIONS) {
      assert.equal(row.status, "not_investigated");
      assert.match(row.text, /^Not built\./);
    }
  });

  test("integrity checks keep a confirmed violation and a suspicious signal apart", () => {
    const text = PROTECTIONS.find((r) => r.id === "integrity-checks")!.text;
    assert.match(text, /confirmed integrity violation and a suspicious signal will be shown apart/);
    assert.match(text, /never be reported as a confirmed violation/);
  });

  test("the webhook row is not built, off by default if built, and keeps its address in Windows Credential Manager", () => {
    const row = SENDERS.find((r) => r.id === "webhooks")!;
    assert.equal(row.status, "not_investigated");
    assert.match(row.text, /^Not built, so nothing can be posted\./);
    assert.match(row.text, /off until you switch it on and give it a destination/);
    assert.match(row.text, /Windows Credential Manager, not in a settings file/);
  });

  test("Rich Presence is Experimental (as in the capability matrix), off until switched on, and says what it sends", () => {
    const row = SENDERS.find((r) => r.id === "rich-presence")!;
    assert.equal(row.status, "experimental");
    assert.match(row.text, /^Off until you switch it on/);
    assert.match(row.text, /Browsing the launcher/);
  });
});

describe("the headline", () => {
  test("it is the weakest label on the page: Not investigated", () => {
    assert.equal(headlineStatus(), "not_investigated");
  });

  test("it follows the rows it is given, and an empty set is Not investigated", () => {
    assert.equal(headlineStatus(SENDERS.filter((r) => r.status === "experimental")), "experimental");
    assert.equal(headlineStatus([]), "not_investigated");
    assert.equal(headlineStatus([{ id: "x", title: "x", status: "unsupported", text: "x" }, ...SENDERS]), "unsupported");
  });
});

describe("the rules", () => {
  test("each is a design rule with a date and says what was not checked", () => {
    assert.match(CHECKED_ON, /^\d{4}-\d{2}-\d{2}$/);
    assert.equal(RULE_TAG, "Design rule, not an audit");
    for (const rule of RULES) {
      assert.ok(rule.statement.trim() && rule.evidence.trim(), rule.id);
      assert.ok(rule.evidence.includes(CHECKED_ON), `${rule.id}: says when it was checked`);
      assert.match(rule.evidence, /not re-audited|Not checked/, `${rule.id}: says what was not checked`);
    }
  });

  test("credentials: SUL stores none and never handles the Steam login", () => {
    assert.match(CREDENTIALS.statement, /does not store passwords, tokens or API keys/);
    assert.match(CREDENTIALS.statement, /never handles your Steam login/);
  });

  test("data sharing: nothing by default, and the exceptions and the unchecked parts are named", () => {
    assert.match(DATA_SHARING.statement, /^By default SUL sends nothing off this PC\./);
    assert.match(DATA_SHARING.statement, /Two things could send data/);
    assert.match(DATA_SHARING.evidence, /traffic was never captured/);
    assert.match(DATA_SHARING.evidence, /reqwest/);
  });
});

describe("wording", () => {
  test("no line promises safety or security, or claims more than a design rule", () => {
    const overclaim = /\b(secure|safe|safely|guarantee[sd]?|protected|100%|impossible|unhackable|audited by|verified safe|encrypted)\b/i;
    for (const text of everyText()) assert.doesNotMatch(text, overclaim, text.slice(0, 60));
  });

  test("no sample figure or made-up measurement appears", () => {
    for (const text of everyText()) assert.doesNotMatch(text, /\d+(\.\d+)?\s*(MB|KB|GB|%|ms)\b/i, text.slice(0, 60));
  });
});
