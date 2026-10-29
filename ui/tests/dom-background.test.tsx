import { mount, resetBrowser, settle, stubFetch, stubObjectUrls } from "./dom";
import assert from "node:assert/strict";
import { File as NodeFile } from "node:buffer";
import { afterEach, beforeEach, describe, test } from "node:test";
import {
  ACCEPTED, DEFAULT_BG, clearCustomBg, isAcceptedType, isVideoType, loadBg, loadCustomBg, loadManifest, saveBg, saveCustomBg,
} from "../src/background";
import { BackgroundLayer } from "../src/components/BackgroundLayer";
import { SettingsProvider } from "../src/settings";

const KEY = "sul.bg";
let undo: Array<() => void> = [];
beforeEach(() => resetBrowser());
afterEach(async () => {
  for (const f of undo.reverse()) f();
  undo = [];
  await clearCustomBg();
});

/** Make `localStorage` fail the way it does in a private window or with site data blocked. */
function breakStorage(): () => void {
  const proto = Object.getPrototypeOf(localStorage) as Storage;
  const original = { get: proto.getItem, set: proto.setItem };
  const fail = () => {
    throw new Error("storage blocked");
  };
  proto.getItem = fail;
  proto.setItem = fail;
  return () => {
    proto.getItem = original.get;
    proto.setItem = original.set;
  };
}

describe("background settings in localStorage", () => {
  test("a fresh browser gets the default: a solid colour, 35% dimming", () => {
    assert.deepEqual(loadBg(), { mode: "solid", galleryId: "", overlay: 0.35 });
    assert.deepEqual(DEFAULT_BG, loadBg());
  });

  test("what was saved is read back", () => {
    saveBg({ mode: "gallery", galleryId: "w07", overlay: 0.5 });
    assert.deepEqual(loadBg(), { mode: "gallery", galleryId: "w07", overlay: 0.5 });
    saveBg({ mode: "custom", galleryId: "", overlay: 0 });
    assert.deepEqual(loadBg(), { mode: "custom", galleryId: "", overlay: 0 }, "0% is a valid dimming, not a missing value");
    saveBg({ mode: "solid", galleryId: "", overlay: 0.9 });
    assert.equal(loadBg().overlay, 0.9, "90% is the most the slider offers");
  });

  test("damaged or hand-edited values fall back to safe ones, field by field", () => {
    localStorage.setItem(KEY, JSON.stringify({ mode: "hologram", galleryId: 7, overlay: 5 }));
    assert.deepEqual(loadBg(), { mode: "solid", galleryId: "", overlay: 0.35 });
    localStorage.setItem(KEY, JSON.stringify({ mode: "gallery", galleryId: "w02", overlay: -0.1 }));
    assert.deepEqual(loadBg(), { mode: "gallery", galleryId: "w02", overlay: 0.35 });
    localStorage.setItem(KEY, JSON.stringify({ overlay: "0.5" }));
    assert.equal(loadBg().overlay, 0.35, "a number written as text is not trusted");
    localStorage.setItem(KEY, "{not json");
    assert.deepEqual(loadBg(), DEFAULT_BG);
  });

  test("blocked storage never throws: reading gives the default and writing is silently skipped", () => {
    undo.push(breakStorage());
    assert.deepEqual(loadBg(), DEFAULT_BG);
    assert.doesNotThrow(() => saveBg({ mode: "gallery", galleryId: "w01", overlay: 0.2 }));
  });
});

describe("accepted file types", () => {
  test("only pictures, GIFs and short-video containers are accepted", () => {
    for (const ok of ["image/png", "image/jpeg", "image/webp", "image/gif", "video/mp4", "video/webm"]) assert.ok(isAcceptedType(ok), ok);
    for (const no of ["", "application/pdf", "image/svg+xml", "image/bmp", "video/quicktime", "text/html", "image/png; charset=x"]) {
      assert.ok(!isAcceptedType(no), JSON.stringify(no));
    }
    assert.equal(ACCEPTED.split(",").length, 6, "the picker's accept list matches the checked list");
  });

  test("videos are told apart from pictures", () => {
    assert.ok(isVideoType("video/mp4"));
    assert.ok(isVideoType("video/webm"));
    assert.ok(!isVideoType("image/gif"));
  });
});

describe("the stored custom file (IndexedDB)", () => {
  test("nothing is stored at first", async () => {
    assert.equal(await loadCustomBg(), null);
  });

  test("a saved file comes back whole, and clearing removes it", async () => {
    const blob = new NodeFile(["hello"], "pic.png", { type: "image/png" });
    assert.equal(await saveCustomBg({ blob: blob as unknown as Blob, type: "image/png", name: "pic.png" }), true);
    const back = await loadCustomBg();
    assert.equal(back?.name, "pic.png");
    assert.equal(back?.type, "image/png");
    assert.equal(await (back!.blob as Blob).text(), "hello");

    await clearCustomBg();
    assert.equal(await loadCustomBg(), null);
  });

  test("saving again replaces the single stored background", async () => {
    await saveCustomBg({ blob: new NodeFile(["a"], "a.png", { type: "image/png" }) as unknown as Blob, type: "image/png", name: "a.png" });
    await saveCustomBg({ blob: new NodeFile(["b"], "b.mp4", { type: "video/mp4" }) as unknown as Blob, type: "video/mp4", name: "b.mp4" });
    assert.equal((await loadCustomBg())?.name, "b.mp4");
  });

  test("when the browser has no usable database, saving says false and the others do not throw", async () => {
    const realOpen = indexedDB.open.bind(indexedDB);
    indexedDB.open = (() => {
      throw new Error("blocked");
    }) as typeof indexedDB.open;
    try {
      assert.equal(await saveCustomBg({ blob: new NodeFile(["a"], "a.png") as unknown as Blob, type: "image/png", name: "a.png" }), false);
      assert.equal(await loadCustomBg(), null);
      await assert.doesNotReject(clearCustomBg());
    } finally {
      indexedDB.open = realOpen;
    }
  });
});

describe("the gallery manifest", () => {
  const list = [{ id: "w01", name: "One", file: "wallpapers/w01.jpg", thumb: "wallpapers/thumbs/w01.jpg", lowRes: false }];

  test("a good manifest is returned as is", async () => {
    undo.push(stubFetch(list));
    assert.deepEqual(await loadManifest(), list);
  });

  test("a missing manifest, or one that is not a list, gives an empty gallery", async () => {
    undo.push(stubFetch(list, false));
    assert.deepEqual(await loadManifest(), []);
    undo.push(stubFetch({ wallpapers: list }));
    assert.deepEqual(await loadManifest(), []);
  });

  test("a network failure or unreadable JSON gives an empty gallery", async () => {
    const original = globalThis.fetch;
    undo.push(() => {
      globalThis.fetch = original;
    });
    globalThis.fetch = (async () => {
      throw new Error("offline");
    }) as typeof fetch;
    assert.deepEqual(await loadManifest(), []);
    globalThis.fetch = (async () => ({
      ok: true,
      json: async () => {
        throw new SyntaxError("bad json");
      },
    })) as unknown as typeof fetch;
    assert.deepEqual(await loadManifest(), []);
  });
});

describe("the background layer", () => {
  const manifest = [{ id: "w01", name: "One", file: "wallpapers/w01.jpg", thumb: "wallpapers/thumbs/w01.jpg", lowRes: false }];

  async function layer() {
    undo.push(stubFetch(manifest));
    const m = await mount(
      <SettingsProvider>
        <BackgroundLayer />
      </SettingsProvider>,
    );
    await settle();
    return m;
  }

  test("a solid background has no picture and no dimming layer, and is hidden from screen readers", async () => {
    const { host, unmount } = await layer();
    const el = host.querySelector(".bg-layer")!;
    assert.equal(el.getAttribute("aria-hidden"), "true");
    assert.equal(el.querySelector("img, video"), null);
    assert.equal(el.querySelector(".bg-dim"), null);
    unmount();
  });

  test("a gallery background shows that image, dimmed by the saved amount", async () => {
    localStorage.setItem(KEY, JSON.stringify({ mode: "gallery", galleryId: "w01", overlay: 0.5 }));
    const { host, unmount } = await layer();
    assert.equal(host.querySelector("img")?.getAttribute("src"), "wallpapers/w01.jpg");
    assert.equal((host.querySelector(".bg-dim") as HTMLElement).style.opacity, "0.5");
    unmount();
  });

  test("a gallery choice that no longer exists shows nothing rather than a broken image", async () => {
    localStorage.setItem(KEY, JSON.stringify({ mode: "gallery", galleryId: "gone", overlay: 0.5 }));
    const { host, unmount } = await layer();
    assert.equal(host.querySelector("img, video"), null);
    assert.equal(host.querySelector(".bg-dim"), null);
    unmount();
  });

  test("My file shows nothing until a file is actually stored", async () => {
    localStorage.setItem(KEY, JSON.stringify({ mode: "custom", galleryId: "", overlay: 0.3 }));
    const { host, unmount } = await layer();
    assert.equal(host.querySelector("img, video"), null);
    unmount();
  });

  test("a stored picture is shown as an image; a stored video plays muted and looped", async () => {
    const urls = stubObjectUrls();
    undo.push(() => urls.restore());
    localStorage.setItem(KEY, JSON.stringify({ mode: "custom", galleryId: "", overlay: 0.3 }));

    await saveCustomBg({ blob: new NodeFile(["x"], "p.png", { type: "image/png" }) as unknown as Blob, type: "image/png", name: "p.png" });
    let m = await layer();
    assert.match(m.host.querySelector("img")?.getAttribute("src") ?? "", /^blob:test\//);
    assert.equal(m.host.querySelector("video"), null);
    m.unmount();

    await saveCustomBg({ blob: new NodeFile(["x"], "v.mp4", { type: "video/mp4" }) as unknown as Blob, type: "video/mp4", name: "v.mp4" });
    m = await layer();
    const video = m.host.querySelector("video")!;
    assert.ok(video, "a video element");
    assert.ok(video.hasAttribute("autoplay") && video.hasAttribute("loop") && video.hasAttribute("muted") && video.hasAttribute("playsinline"));
    assert.equal(m.host.querySelector("img"), null);
    m.unmount();
  });
});
