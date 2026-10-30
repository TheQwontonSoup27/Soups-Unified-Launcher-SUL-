export type BgMode = "solid" | "gallery" | "custom";
export interface BgSettings { mode: BgMode; galleryId: string; overlay: number }
export interface Wallpaper { id: string; name: string; file: string; thumb: string; lowRes: boolean }
export interface CustomBg { blob: Blob; type: string; name: string }

const KEY = "sul.bg";
export const DEFAULT_BG: BgSettings = { mode: "solid", galleryId: "", overlay: 0.35 };
export const MAX_CUSTOM_BYTES = 300 * 1024 * 1024;
export const ACCEPTED = "image/png,image/jpeg,image/webp,image/gif,video/mp4,video/webm";

export function loadBg(): BgSettings {
  try {
    const r = JSON.parse(localStorage.getItem(KEY) ?? "{}") as Partial<BgSettings>;
    const mode: BgMode = r.mode === "gallery" || r.mode === "custom" ? r.mode : "solid";
    const overlay = typeof r.overlay === "number" && r.overlay >= 0 && r.overlay <= 0.9 ? r.overlay : DEFAULT_BG.overlay;
    return { mode, galleryId: typeof r.galleryId === "string" ? r.galleryId : "", overlay };
  } catch { return { ...DEFAULT_BG }; }
}
export function saveBg(s: BgSettings): void {
  try { localStorage.setItem(KEY, JSON.stringify(s)); } catch { /* ignore */ }
}

export function isVideoType(type: string): boolean { return type.startsWith("video/"); }
export function isAcceptedType(type: string): boolean { return ACCEPTED.split(",").includes(type); }

// ---- custom file storage (IndexedDB; localStorage is far too small for video) ----
const DB = "sul", STORE = "files";
function open(): Promise<IDBDatabase> {
  return new Promise((res, rej) => {
    const r = indexedDB.open(DB, 1);
    r.onupgradeneeded = () => r.result.createObjectStore(STORE);
    r.onsuccess = () => res(r.result);
    r.onerror = () => rej(r.error);
  });
}
export async function saveCustomBg(v: CustomBg): Promise<boolean> {
  try {
    const db = await open();
    return await new Promise<boolean>((res) => {
      const tx = db.transaction(STORE, "readwrite");
      tx.objectStore(STORE).put(v, "background");
      tx.oncomplete = () => res(true);
      tx.onerror = () => res(false);
    });
  } catch { return false; }
}
export async function loadCustomBg(): Promise<CustomBg | null> {
  try {
    const db = await open();
    return await new Promise<CustomBg | null>((res) => {
      const rq = db.transaction(STORE).objectStore(STORE).get("background");
      rq.onsuccess = () => res((rq.result as CustomBg | undefined) ?? null);
      rq.onerror = () => res(null);
    });
  } catch { return null; }
}
export async function clearCustomBg(): Promise<void> {
  try {
    const db = await open();
    await new Promise<void>((res) => {
      const tx = db.transaction(STORE, "readwrite");
      tx.objectStore(STORE).delete("background");
      tx.oncomplete = () => res();
      tx.onerror = () => res();
    });
  } catch { /* ignore */ }
}

/**
 * Whether the browser can load and show the picture at `url`. A fetch status would not tell: for a
 * missing file the dev server and the packaged app both answer with the app's own page.
 */
export function imageLoads(url: string): Promise<boolean> {
  return new Promise((resolve) => {
    const img = new Image();
    img.onload = () => resolve(true);
    img.onerror = () => resolve(false);
    img.src = url;
  });
}

/**
 * The wallpapers of the manifest whose thumbnail is really there. The pictures are git-ignored
 * (licences unconfirmed), so a fresh clone lists wallpapers it does not have; those are left out
 * instead of shown as broken tiles. `loads` is the check, replaceable for tests.
 */
export async function installedWallpapers(
  list: Wallpaper[],
  loads: (url: string) => Promise<boolean> = imageLoads,
): Promise<Wallpaper[]> {
  const present = await Promise.all(list.map((w) => loads(w.thumb)));
  return list.filter((_, i) => present[i]);
}

export async function loadManifest(): Promise<Wallpaper[]> {
  try {
    const r = await fetch("wallpapers/manifest.json");
    if (!r.ok) return [];
    const j = (await r.json()) as unknown;
    return Array.isArray(j) ? (j as Wallpaper[]) : [];
  } catch { return []; }
}
