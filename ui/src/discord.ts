export type Invoker = (cmd: string, args: Record<string, unknown>) => Promise<unknown>;

const KEY = "sul.discord";

export function getInvoker(): Invoker | null {
  try {
    const t = (globalThis as unknown as { __TAURI__?: { core?: { invoke?: Invoker } } }).__TAURI__;
    return t?.core?.invoke ?? null;
  } catch { return null; }
}

/** Opt-in: off unless the user switched it on. */
export function loadPresenceWanted(): boolean {
  try { return localStorage.getItem(KEY) === "1"; } catch { return false; }
}
export function savePresenceWanted(on: boolean): void {
  try { localStorage.setItem(KEY, on ? "1" : "0"); } catch { /* ignore */ }
}

export interface PresenceResult { on: boolean; message: string }

/** Turn Discord Rich Presence on or off. In a plain browser there is no Discord bridge (preview only). */
export async function setPresence(enabled: boolean, invoke: Invoker | null): Promise<PresenceResult> {
  if (!invoke) return { on: enabled, message: enabled ? "Preview only: Discord is only reachable from the desktop app." : "" };
  try {
    const r = await invoke("discord_set_enabled", { enabled });
    return { on: r === "on", message: "" };
  } catch (e) {
    return { on: false, message: typeof e === "string" ? e : "Could not reach Discord." };
  }
}
