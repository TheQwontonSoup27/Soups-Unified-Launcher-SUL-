// A real DOM for tests that click through the UI.
//
// Import this module FIRST in a test file. Each test file runs in its own process under `node --test`,
// so the globals registered here cannot leak into the server-rendering tests.
import "./dom-env";
import { act, type ReactElement } from "react";
import { createRoot } from "react-dom/client";
import assert from "node:assert/strict";

export { act };

export interface Mounted {
  host: HTMLElement;
  unmount(): void;
}

/** Render `ui` into the document, the way the app does (client rendering, effects run). */
export async function mount(ui: ReactElement): Promise<Mounted> {
  const host = document.createElement("div");
  document.body.appendChild(host);
  const root = createRoot(host);
  await act(async () => {
    root.render(ui);
  });
  return {
    host,
    unmount() {
      void act(() => root.unmount());
      host.remove();
    },
  };
}

/**
 * Assert that nothing matched. Do not pass a DOM element to `assert.equal`: when that assertion fails
 * Node builds the message by printing the element, which in happy-dom is a graph so large that the test
 * process appears to hang and is then killed for lack of memory, and the real failure is never shown.
 */
export function absent(el: Element | null, what = "expected nothing here"): void {
  if (el !== null) {
    throw new assert.AssertionError({ message: `${what}, but found <${el.tagName.toLowerCase()}>: ${el.outerHTML.slice(0, 200)}` });
  }
}

/** Let pending promises and timers settle (state updates after an awaited call). */
export async function settle(): Promise<void> {
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 0));
  });
}

/**
 * Wait, turn by turn, until `done()` is true, and fail saying `what` was awaited if it takes longer than
 * `ms`. For work that needs several turns, such as the fake IndexedDB: one `settle()` is not enough for
 * it every time, so a test that asserts after a single `settle()` passes or fails by luck.
 */
export async function waitFor(done: () => boolean, what: string, ms = 3000): Promise<void> {
  const end = Date.now() + ms;
  for (;;) {
    await settle();
    if (done()) return;
    if (Date.now() > end) throw new Error(`still waiting after ${ms} ms for ${what}`);
  }
}

export async function click(el: Element): Promise<void> {
  await act(async () => {
    el.dispatchEvent(new window.MouseEvent("click", { bubbles: true, cancelable: true }));
  });
  await settle();
}

/** Type into a controlled input the way a user does (React listens for the native `input` event). */
export async function type(input: HTMLInputElement, value: string): Promise<void> {
  await act(async () => {
    const setter = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, "value")?.set;
    if (!setter) throw new Error("no value setter on HTMLInputElement");
    setter.call(input, value);
    input.dispatchEvent(new window.Event("input", { bubbles: true }));
  });
}

export async function submit(form: HTMLFormElement): Promise<void> {
  await act(async () => {
    form.dispatchEvent(new window.Event("submit", { bubbles: true, cancelable: true }));
  });
  await settle();
}

export async function key(name: string): Promise<void> {
  await act(async () => {
    window.dispatchEvent(new window.KeyboardEvent("keydown", { key: name, bubbles: true }));
  });
}

/** A key pressed while `el` has the focus (the event starts at `el` and bubbles up). */
export async function keyOn(el: Element, name: string): Promise<void> {
  await act(async () => {
    el.dispatchEvent(new window.KeyboardEvent("keydown", { key: name, bubbles: true, cancelable: true }));
  });
}

/** The one element matching `selector`, or a failure that says what was on the page. */
export function one<T extends Element = HTMLElement>(root: ParentNode, selector: string): T {
  const found = root.querySelectorAll<T>(selector);
  if (found.length !== 1) {
    throw new Error(`expected exactly one ${selector}, found ${found.length}`);
  }
  return found[0]!;
}

/** The element matching `selector` whose text is exactly `text`. */
export function withText<T extends Element = HTMLElement>(root: ParentNode, selector: string, text: string): T {
  const found = [...root.querySelectorAll<T>(selector)].filter((e) => e.textContent?.trim() === text);
  if (found.length !== 1) {
    const seen = [...root.querySelectorAll(selector)].map((e) => JSON.stringify(e.textContent?.trim()));
    throw new Error(`expected exactly one ${selector} with text ${JSON.stringify(text)}, found ${found.length} of: ${seen.join(", ")}`);
  }
  return found[0]!;
}

export interface Call {
  cmd: string;
  args: Record<string, unknown>;
}

/**
 * Pretend to be the Tauri webview: `isTauri()` turns true and `invoke` reaches `handler`. This is the
 * same seam `@tauri-apps/api` uses, so the real `invoke`, `isTauri` and window-control code all run.
 * A handler that throws a string behaves like a failing Rust command (`Result<_, String>`).
 */
export function installTauri(handler: (cmd: string, args: Record<string, unknown>) => unknown): { calls: Call[]; remove(): void } {
  const calls: Call[] = [];
  const g = globalThis as unknown as Record<string, unknown>;
  const w = window as unknown as Record<string, unknown>;
  g.isTauri = true;
  w.__TAURI_INTERNALS__ = {
    invoke: async (cmd: string, args: Record<string, unknown> = {}) => {
      calls.push({ cmd, args });
      return handler(cmd, args);
    },
    metadata: { currentWindow: { label: "main" }, currentWebview: { windowLabel: "main", label: "main" } },
  };
  return {
    calls,
    remove() {
      delete g.isTauri;
      delete w.__TAURI_INTERNALS__;
    },
  };
}

/** Back to a clean browser between tests. */
export function resetBrowser(): void {
  document.body.innerHTML = "";
  document.documentElement.removeAttribute("data-theme");
  document.documentElement.removeAttribute("style");
  localStorage.clear();
}

/** Answer every `fetch` with `body` as JSON (or with a failure when `ok` is false). Returns the undo. */
export function stubFetch(body: unknown, ok = true): () => void {
  const original = globalThis.fetch;
  globalThis.fetch = (async () => ({ ok, json: async () => body })) as unknown as typeof fetch;
  return () => {
    globalThis.fetch = original;
  };
}

/**
 * `new Image()` for tests: happy-dom never loads pictures, so this one answers `onload` when
 * `loads(url)` is true and `onerror` otherwise, as soon as `src` is set (in a microtask, so a
 * `settle()` is enough). Returns the undo.
 */
export function stubImages(loads: (url: string) => boolean): () => void {
  const original = globalThis.Image;
  class FakeImage {
    onload: (() => void) | null = null;
    onerror: (() => void) | null = null;
    set src(url: string) {
      queueMicrotask(() => (loads(url) ? this.onload : this.onerror)?.());
    }
  }
  globalThis.Image = FakeImage as unknown as typeof Image;
  return () => {
    globalThis.Image = original;
  };
}

/** `URL.createObjectURL` for tests: a predictable `blob:` address per call. Returns the undo. */
export function stubObjectUrls(): { revoked: string[]; restore(): void } {
  const revoked: string[] = [];
  const original = { create: URL.createObjectURL, revoke: URL.revokeObjectURL };
  let n = 0;
  URL.createObjectURL = () => `blob:test/${++n}`;
  URL.revokeObjectURL = (u: string) => {
    revoked.push(u);
  };
  return {
    revoked,
    restore() {
      URL.createObjectURL = original.create;
      URL.revokeObjectURL = original.revoke;
    },
  };
}
