// Display density: the pure rules (which view a module shows, how a choice changes the settings) and the storage
// that must survive a damaged or blocked localStorage. No DOM here: the storage is a small stand-in.
import assert from "node:assert/strict";
import { afterEach, beforeEach, describe, test } from "node:test";
import {
  CHOICES, CHOICE_LABEL, DEFAULT_DENSITY, DENSITY_MODULES, choiceFor, describeEffective, effectiveDensity, isDensity,
  isDensityModule, loadDensity, overridesInUse, saveDensity, withChoice, withGlobal, withoutOverrides,
  type DensitySettings,
} from "../src/density";

const ENGINE = "clipping-engine";
const settings = (global: DensitySettings["global"], overrides: DensitySettings["overrides"] = {}): DensitySettings => ({ global, overrides });

describe("which view a module shows", () => {
  test("the default is Compact for everything, with no overrides", () => {
    assert.deepEqual(DEFAULT_DENSITY, { global: "compact", overrides: {} });
    assert.equal(effectiveDensity(DEFAULT_DENSITY, ENGINE), "compact");
    assert.equal(choiceFor(DEFAULT_DENSITY, ENGINE), "inherit");
  });

  test("a module that inherits follows the global setting", () => {
    assert.equal(effectiveDensity(settings("detailed"), ENGINE), "detailed");
    assert.equal(effectiveDensity(settings("compact"), ENGINE), "compact");
  });

  test("a module's own choice wins over the global one, in both directions", () => {
    assert.equal(effectiveDensity(settings("compact", { [ENGINE]: "detailed" }), ENGINE), "detailed");
    assert.equal(effectiveDensity(settings("detailed", { [ENGINE]: "compact" }), ENGINE), "compact");
    assert.equal(choiceFor(settings("detailed", { [ENGINE]: "compact" }), ENGINE), "compact");
  });

  test("a module the settings know nothing about inherits", () => {
    assert.equal(choiceFor(settings("detailed"), "anything-else"), "inherit");
    assert.equal(effectiveDensity(settings("detailed"), "anything-else"), "detailed");
  });

  test("an id that is an Object.prototype name is never read as an override", () => {
    for (const id of ["constructor", "toString", "__proto__", "hasOwnProperty"]) {
      assert.equal(choiceFor(DEFAULT_DENSITY, id), "inherit", id);
      assert.equal(effectiveDensity(settings("detailed"), id), "detailed", id);
    }
  });

  test("the words say where the view comes from", () => {
    assert.equal(describeEffective(DEFAULT_DENSITY, ENGINE), "Compact (from Global)");
    assert.equal(describeEffective(settings("compact", { [ENGINE]: "detailed" }), ENGINE), "Detailed (set for this module)");
    assert.equal(describeEffective(settings("detailed"), ENGINE), "Detailed (from Global)");
    assert.equal(describeEffective(settings("detailed", { [ENGINE]: "compact" }), ENGINE), "Compact (set for this module)");
  });

  test("there are three choices, labelled as the wireframe labels them", () => {
    assert.deepEqual([...CHOICES], ["inherit", "compact", "detailed"]);
    assert.deepEqual(CHOICES.map((c) => CHOICE_LABEL[c]), ["Inherit Global", "Always Compact", "Always Detailed"]);
  });
});

describe("changing the settings", () => {
  test("withGlobal changes only the global view", () => {
    const before = settings("compact", { [ENGINE]: "detailed" });
    assert.deepEqual(withGlobal(before, "detailed"), settings("detailed", { [ENGINE]: "detailed" }));
  });

  test("withChoice sets an override, and Inherit removes it so none is left behind", () => {
    const set = withChoice(DEFAULT_DENSITY, ENGINE, "detailed");
    assert.deepEqual(set.overrides, { [ENGINE]: "detailed" });
    const cleared = withChoice(set, ENGINE, "inherit");
    assert.deepEqual(cleared.overrides, {});
    assert.equal(Object.keys(cleared.overrides).length, 0);
    // Always Compact is a real override even when the global view is Compact: it holds if the global one changes.
    const pinned = withChoice(DEFAULT_DENSITY, ENGINE, "compact");
    assert.deepEqual(pinned.overrides, { [ENGINE]: "compact" });
    assert.equal(effectiveDensity(withGlobal(pinned, "detailed"), ENGINE), "compact");
  });

  test("nothing is changed in place", () => {
    const before = settings("compact", { [ENGINE]: "detailed" });
    const snapshot = JSON.stringify(before);
    withGlobal(before, "detailed");
    withChoice(before, ENGINE, "inherit");
    withChoice(before, ENGINE, "compact");
    withoutOverrides(before);
    assert.equal(JSON.stringify(before), snapshot);
    assert.equal(Object.isFrozen(DEFAULT_DENSITY.overrides), false);
    assert.deepEqual(DEFAULT_DENSITY, { global: "compact", overrides: {} });
  });

  test("an id that is not a density module changes nothing, and gives back the same object", () => {
    const before = settings("compact");
    for (const id of ["nope", "capture", "__proto__", "constructor", ""]) {
      assert.equal(withChoice(before, id, "detailed"), before, id);
    }
    assert.equal(Object.prototype.hasOwnProperty.call(withChoice(before, "__proto__", "detailed").overrides, "__proto__"), false);
  });

  test("withoutOverrides clears every override and keeps the global view", () => {
    assert.deepEqual(withoutOverrides(settings("detailed", { [ENGINE]: "compact" })), settings("detailed"));
  });

  test("overridesInUse lists real overrides with their label, and nothing for inheriting modules", () => {
    assert.deepEqual(overridesInUse(DEFAULT_DENSITY), []);
    assert.deepEqual(overridesInUse(settings("compact", { [ENGINE]: "detailed" })), [
      { id: ENGINE, label: "Media & Clipping Engine", density: "detailed" },
    ]);
    // An entry for a module that does not exist is not listed.
    assert.deepEqual(overridesInUse(settings("compact", { ghost: "detailed" })), []);
  });

  test("the module list is what the screens and storage rely on", () => {
    assert.deepEqual(DENSITY_MODULES.map((m) => m.id), [ENGINE]);
    assert.equal(DENSITY_MODULES[0]!.label, "Media & Clipping Engine");
    assert.equal(DENSITY_MODULES[0]!.status, "not_investigated");
    assert.equal(new Set(DENSITY_MODULES.map((m) => m.id)).size, DENSITY_MODULES.length);
    assert.ok(isDensityModule(ENGINE));
    assert.ok(!isDensityModule("constructor") && !isDensityModule(""));
    assert.ok(isDensity("compact") && isDensity("detailed"));
    assert.ok(!isDensity("inherit") && !isDensity("Compact") && !isDensity(null) && !isDensity(undefined) && !isDensity(1));
  });
});

describe("storage", () => {
  const original = Object.getOwnPropertyDescriptor(globalThis, "localStorage");
  let data: Map<string, string>;
  beforeEach(() => {
    data = new Map();
    Object.defineProperty(globalThis, "localStorage", {
      configurable: true,
      value: {
        getItem: (k: string) => data.get(k) ?? null,
        setItem: (k: string, v: string) => void data.set(k, v),
        removeItem: (k: string) => void data.delete(k),
        clear: () => data.clear(),
      },
    });
  });
  afterEach(() => {
    if (original) Object.defineProperty(globalThis, "localStorage", original);
    else delete (globalThis as { localStorage?: unknown }).localStorage;
  });

  const KEY = "sul.density";

  test("nothing saved gives the default, and each call gives its own copy", () => {
    const a = loadDensity();
    assert.deepEqual(a, DEFAULT_DENSITY);
    a.overrides[ENGINE] = "detailed"; // must not leak into the default or the next load
    assert.deepEqual(DEFAULT_DENSITY, { global: "compact", overrides: {} });
    assert.deepEqual(loadDensity(), DEFAULT_DENSITY);
  });

  test("what is saved is what is loaded", () => {
    const s = settings("detailed", { [ENGINE]: "compact" });
    saveDensity(s);
    assert.deepEqual(JSON.parse(data.get(KEY)!), s);
    assert.deepEqual(loadDensity(), s);
  });

  test("damaged storage falls back to the default instead of failing", () => {
    for (const raw of ["", "{", "null", "[]", "5", "\"detailed\"", "true", "{\"global\":", "undefined"]) {
      data.set(KEY, raw);
      assert.deepEqual(loadDensity(), DEFAULT_DENSITY, JSON.stringify(raw));
    }
  });

  test("each wrong part is replaced on its own, and the good parts are kept", () => {
    data.set(KEY, JSON.stringify({ global: "huge", overrides: { [ENGINE]: "detailed" } }));
    assert.deepEqual(loadDensity(), settings("compact", { [ENGINE]: "detailed" }));
    data.set(KEY, JSON.stringify({ global: "detailed", overrides: "all" }));
    assert.deepEqual(loadDensity(), settings("detailed"));
    data.set(KEY, JSON.stringify({ global: "detailed", overrides: [ENGINE] }));
    assert.deepEqual(loadDensity(), settings("detailed"));
    data.set(KEY, JSON.stringify({ global: "detailed", overrides: { [ENGINE]: "inherit" } }));
    assert.deepEqual(loadDensity(), settings("detailed"), "Inherit is not stored; a stored one is dropped");
    data.set(KEY, JSON.stringify({ global: 3, overrides: { [ENGINE]: 7 } }));
    assert.deepEqual(loadDensity(), DEFAULT_DENSITY);
  });

  test("a module that no longer exists, or a prototype name, is dropped on load", () => {
    data.set(KEY, `{"global":"detailed","overrides":{"gone":"detailed","__proto__":"detailed","constructor":"compact","${ENGINE}":"compact"}}`);
    const loaded = loadDensity();
    assert.deepEqual(loaded, settings("detailed", { [ENGINE]: "compact" }));
    assert.equal(Object.getPrototypeOf(loaded.overrides), Object.prototype);
    assert.equal(({} as Record<string, unknown>)["detailed"], undefined, "Object.prototype was not polluted");
  });

  test("blocked storage neither throws nor stops the settings working for the session", () => {
    const blocked = {
      getItem: () => { throw new Error("blocked"); },
      setItem: () => { throw new Error("blocked"); },
    };
    Object.defineProperty(globalThis, "localStorage", { configurable: true, value: blocked });
    assert.deepEqual(loadDensity(), DEFAULT_DENSITY);
    assert.doesNotThrow(() => saveDensity(settings("detailed")));
  });

  test("no storage at all behaves the same", () => {
    delete (globalThis as { localStorage?: unknown }).localStorage;
    assert.deepEqual(loadDensity(), DEFAULT_DENSITY);
    assert.doesNotThrow(() => saveDensity(settings("detailed")));
  });
});
