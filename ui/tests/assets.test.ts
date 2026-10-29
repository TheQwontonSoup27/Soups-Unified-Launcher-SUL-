// The files the app and its installer depend on: present, really images, and the size their names claim.
import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";
import { dirname, isAbsolute, join, normalize, resolve } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..", "..");
const read = (rel: string) => readFileSync(join(ROOT, rel));
const json = (rel: string) => JSON.parse(read(rel).toString("utf8")) as Record<string, unknown>;

const PNG_MAGIC = Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);

function pngSize(rel: string): [number, number] {
  const b = read(rel);
  assert.ok(b.subarray(0, 8).equals(PNG_MAGIC), `${rel} is not a PNG file`);
  assert.equal(b.subarray(12, 16).toString("ascii"), "IHDR", `${rel}: no PNG header block`);
  return [b.readUInt32BE(16), b.readUInt32BE(20)];
}

test("the logo is a square PNG at least 256 px wide (it is the splash, the title bar and the sidebar icon)", () => {
  const [w, h] = pngSize("ui/public/logo.png");
  assert.equal(w, h, "square");
  assert.ok(w >= 256, `${w}px is too small for the splash`);
});

test("the page icon the HTML points at exists", () => {
  const html = read("ui/index.html").toString("utf8");
  const href = /rel="icon"[^>]*href="([^"]+)"/.exec(html)?.[1];
  assert.equal(href, "/favicon.png");
  const [w, h] = pngSize("ui/public/favicon.png");
  assert.equal(w, h);
});

test("every icon the installer is configured with exists", () => {
  const conf = json("src-tauri/tauri.conf.json") as { bundle: { icon: string[] } };
  assert.ok(conf.bundle.icon.length >= 4);
  for (const icon of conf.bundle.icon) assert.ok(existsSync(join(ROOT, "src-tauri", icon)), `missing ${icon}`);
});

test("icon files are real images of the size their names say", () => {
  const dir = "src-tauri/icons";
  const sized = [
    ["32x32.png", 32], ["64x64.png", 64], ["128x128.png", 128], ["128x128@2x.png", 256], ["icon.png", 512],
    ["StoreLogo.png", 50],
    ...[30, 44, 71, 89, 107, 142, 150, 284, 310].map((n): [string, number] => [`Square${n}x${n}Logo.png`, n]),
  ] as Array<[string, number]>;
  for (const [file, size] of sized) assert.deepEqual(pngSize(`${dir}/${file}`), [size, size], file);
});

test("the Windows icon holds every size Windows asks for, from 16 to 256", () => {
  const b = read("src-tauri/icons/icon.ico");
  assert.equal(b.readUInt16LE(0), 0, "reserved");
  assert.equal(b.readUInt16LE(2), 1, "type: icon");
  const count = b.readUInt16LE(4);
  const sizes = Array.from({ length: count }, (_, i) => b[6 + 16 * i] || 256);
  for (const need of [16, 32, 48, 256]) assert.ok(sizes.includes(need), `no ${need}px image in ${sizes}`);
});

test("the macOS icon is an icns file", () => {
  assert.equal(read("src-tauri/icons/icon.icns").subarray(0, 4).toString("ascii"), "icns");
});

interface Wallpaper { id: string; name: string; file: string; thumb: string; lowRes: boolean }
const wallpapers = (): Wallpaper[] => JSON.parse(read("ui/public/wallpapers/manifest.json").toString("utf8")) as Wallpaper[];

test("the wallpaper list is well formed: unique ids, every field present, files only inside the wallpapers folder", () => {
  const list = wallpapers();
  assert.ok(Array.isArray(list) && list.length > 0);
  assert.equal(new Set(list.map((w) => w.id)).size, list.length, "ids are unique");
  for (const w of list) {
    assert.deepEqual(Object.keys(w).sort(), ["file", "id", "lowRes", "name", "thumb"], w.id);
    assert.equal(typeof w.lowRes, "boolean", w.id);
    for (const path of [w.file, w.thumb]) {
      assert.ok(!isAbsolute(path) && !normalize(path).startsWith(".."), `${w.id}: ${path} leaves the folder`);
      assert.ok(path.startsWith("wallpapers/"), `${w.id}: ${path}`);
      assert.match(path, /\.(jpe?g|png|webp)$/i);
    }
    assert.notEqual(w.file, w.thumb);
  }
});

// Known Section 1 defect, kept visible rather than hidden. The manifest is committed but the
// images are git-ignored until their licences are confirmed, so a fresh clone shows the gallery
// with every thumbnail missing. This stays a `todo` (reported, not failing) until the images are
// added or the manifest is generated from what is present.
test("every wallpaper in the list has its image and thumbnail on disk", { todo: "images are git-ignored until licences are confirmed; the gallery shows broken tiles on a fresh clone" }, () => {
  const missing = wallpapers().flatMap((w) => [w.file, w.thumb]).filter((p) => !existsSync(join(ROOT, "ui/public", p)));
  assert.deepEqual(missing, []);
});
