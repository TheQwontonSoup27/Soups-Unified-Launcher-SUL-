// The Security & Trust page's statements: shaped so that none of them can say more than was checked.
import assert from "node:assert/strict";
import { describe, test } from "node:test";
import { STATUS_LABEL } from "../src/capabilities";
import { ALL_ROWS, CHECKED_ON, CREDENTIALS, DATA_SHARING, PROTECTIONS, PROTECTIONS_NOTE, RULES, RULE_TAG, SENDERS, headlineStatus } from "../src/security";

const everyText = () => [
  ...RULES.flatMap((r) => [r.title, r.statement, r.evidence]),
  ...ALL_ROWS.flatMap((r) => [r.title, r.text, r.limitation ?? ""]),
  RULE_TAG,
  PROTECTIONS_NOTE,
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

  test("the three protections are Experimental, say they are code and not in the app, and state what limits them", () => {
    for (const row of PROTECTIONS) {
      assert.equal(row.status, "experimental", row.id);
      assert.match(row.text, /^In the code, not in the app\./, row.id);
      assert.ok((row.limitation ?? "").length > 60, `${row.id}: a real limitation`);
      assert.match(row.limitation ?? "", /Tested on made-up|Tested on temporary/, `${row.id}: says what it was tested on`);
    }
    assert.match(PROTECTIONS_NOTE, /Nothing in the app calls them yet/);
    assert.match(PROTECTIONS_NOTE, /does not scan, isolate or check anything/);
    assert.match(PROTECTIONS_NOTE, /never a verdict/);
  });

  test("package scanning reads names and sizes only, and says what it cannot do", () => {
    const row = PROTECTIONS.find((r) => r.id === "package-scanning")!;
    assert.match(row.text, /only the names and sizes/);
    assert.match(row.text, /never unpacks or opens anything/);
    assert.match(row.text, /Confirmed problems are facts about the package's structure/);
    assert.match(row.text, /Suspicious signals need a person/);
    assert.match(row.limitation ?? "", /cannot detect malware/);
    assert.match(row.limitation ?? "", /legitimate mods often contain program files/);
  });

  test("quarantine copies, checks, then removes; restores without overwriting; refuses original installs; has no purge", () => {
    const row = PROTECTIONS.find((r) => r.id === "quarantine")!;
    assert.match(row.text, /copies the file, checks the copy matches, writes down where it came from, and only then removes the original/);
    assert.match(row.text, /never overwrites anything/);
    assert.match(row.text, /original game install is refused, in both directions/);
    assert.match(row.text, /no purge or empty operation/);
    assert.match(row.limitation ?? "", /power cut on a real disk was not tested/);
  });

  test("integrity checks keep a confirmed violation and a suspicious signal apart, and change nothing", () => {
    const row = PROTECTIONS.find((r) => r.id === "integrity-checks")!;
    assert.match(row.text, /changes nothing/);
    assert.match(row.text, /changed or missing file is a confirmed integrity violation/);
    assert.match(row.text, /reported as a change, not a violation/);
    assert.match(row.text, /suspicious signal is never reported as a confirmed violation/);
    assert.match(row.limitation ?? "", /not cheat or malware detection/);
  });

  test("the webhook row is not built, off by default if built, and keeps its address in Windows Credential Manager", () => {
    const row = SENDERS.find((r) => r.id === "webhooks")!;
    assert.equal(row.status, "not_investigated");
    assert.match(row.text, /Only the part that checks an address exists/);
    assert.match(row.limitation ?? "", /needs a network library, which SUL does not have/i);
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
  test("it is the weakest label on the page: Not investigated, and only the webhook row is the reason", () => {
    assert.equal(headlineStatus(), "not_investigated");
    assert.deepEqual(ALL_ROWS.filter((r) => r.status === "not_investigated").map((r) => r.id), ["webhooks"]);
    assert.equal(headlineStatus(ALL_ROWS.filter((r) => r.id !== "webhooks")), "experimental");
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
    assert.match(CREDENTIALS.evidence, /a test fails if a new one appears/);
    assert.match(CREDENTIALS.evidence, /Not checked: what Steam, Discord or Windows keep on their own/);
  });

  test("data sharing: nothing by default, and the exceptions and the unchecked parts are named", () => {
    assert.match(DATA_SHARING.statement, /^By default SUL sends nothing off this PC\./);
    assert.match(DATA_SHARING.statement, /Two things could send data/);
    assert.match(DATA_SHARING.evidence, /traffic was never captured/);
    assert.match(DATA_SHARING.evidence, /WebView2/);
    assert.match(DATA_SHARING.evidence, /not read line by line/);
    // The Windows build has no HTTP client; an earlier version of this page wrongly said the framework brought one in.
    assert.match(DATA_SHARING.evidence, /no HTTP, socket, TLS or WebSocket library/);
    assert.doesNotMatch(DATA_SHARING.evidence, /reqwest/);
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
