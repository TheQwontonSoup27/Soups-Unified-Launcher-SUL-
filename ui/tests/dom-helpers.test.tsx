// The test helpers that make the DOM tests trustworthy: a failing check must say what was found, quickly, and a
// check on slow work must wait for the work and not for luck.
import assert from "node:assert/strict";
import { describe, test } from "node:test";
import { absent, mount, resetBrowser, waitFor } from "./dom";

describe("absent", () => {
  test("passes when nothing matched", () => {
    absent(null);
    absent(null, "there should be no dialog");
  });

  test("fails with a short message that names the element, not with a dump of it", async () => {
    resetBrowser();
    const rows = Array.from({ length: 800 }, (_, i) => <p key={i}>row {i}</p>);
    const { host, unmount } = await mount(<div className="big">{rows}</div>);
    const started = Date.now();
    assert.throws(
      () => absent(host.querySelector(".big"), "no big block expected"),
      (error: Error) => {
        assert.equal(error.name, "AssertionError");
        assert.match(error.message, /^no big block expected, but found <div>: <div class="big">/);
        assert.ok(error.message.length < 400, `message is ${error.message.length} characters`);
        return true;
      },
    );
    assert.ok(Date.now() - started < 1000, "reporting the failure is quick");
    unmount();
  });
});

describe("waitFor", () => {
  test("returns as soon as the condition holds, even if it takes several turns", async () => {
    let turns = 0;
    setTimeout(function again() {
      turns += 1;
      if (turns < 5) setTimeout(again, 0);
    }, 0);
    await waitFor(() => turns >= 5, "five turns");
    assert.equal(turns, 5);
  });

  test("returns at once when the condition already holds", async () => {
    await waitFor(() => true, "nothing");
  });

  test("fails after the time limit, saying what it was waiting for", async () => {
    await assert.rejects(waitFor(() => false, "a thing that never happens", 30), /still waiting after 30 ms for a thing that never happens/);
  });
});
