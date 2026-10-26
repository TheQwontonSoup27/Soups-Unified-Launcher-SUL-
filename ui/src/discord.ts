import { getInvoker, type Invoker } from "./lib/backend";

export { getInvoker, type Invoker };

const KEY = "sul.discord";
const SMALL_KEY = "sul.discord.small";

/** Longest asset key Discord accepts (mirrors `MAX_ASSET_KEY` in sul-core). */
export const MAX_ASSET_KEY = 256;

/** Opt-in: off unless the user switched it on. */
export function loadPresenceWanted(): boolean {
  try { return localStorage.getItem(KEY) === "1"; } catch { return false; }
}
export function savePresenceWanted(on: boolean): void {
  try { localStorage.setItem(KEY, on ? "1" : "0"); } catch { /* ignore */ }
}

/**
 * Asset key of the small corner image, as uploaded for the application in the Discord developer
 * portal. Empty means none. Discord draws that image at a fixed size: the activity payload has no
 * scale control, so only *which* image is used can be chosen.
 */
export function normalizeAssetKey(raw: string): string {
  const key = raw.trim();
  if (key.length > MAX_ASSET_KEY || /[\u0000-\u001f\u007f]/.test(key)) return "";
  return key;
}
export function loadSmallImage(): string {
  try { return normalizeAssetKey(localStorage.getItem(SMALL_KEY) ?? ""); } catch { return ""; }
}
export function saveSmallImage(key: string): void {
  try { localStorage.setItem(SMALL_KEY, normalizeAssetKey(key)); } catch { /* ignore */ }
}

export interface PresenceResult { on: boolean; message: string }

/** Turn Discord Rich Presence on or off. In a plain browser there is no Discord bridge (preview only). */
export async function setPresence(enabled: boolean, invoke: Invoker | null, smallImage?: string): Promise<PresenceResult> {
  if (!invoke) return { on: enabled, message: enabled ? "Preview only: Discord is only reachable from the desktop app." : "" };
  try {
    const key = normalizeAssetKey(smallImage ?? "");
    const r = await invoke("discord_set_enabled", { enabled, smallImage: key || undefined });
    return { on: r === "on", message: "" };
  } catch (e) {
    return { on: false, message: typeof e === "string" ? e : "Could not reach Discord." };
  }
}
