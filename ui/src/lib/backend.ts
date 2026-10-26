import { invoke, isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";

export type InstallMode = "standard" | "portable";

/** Mirrors `AppInfo` in src-tauri/src/lib.rs. */
export interface AppInfo {
  name: string;
  version: string;
  install_mode: InstallMode;
  app_root: string;
}

/** Returns null when running in a plain browser (e.g. `npm run dev`) with no Tauri backend. */
export async function getAppInfo(): Promise<AppInfo | null> {
  if (!isTauri()) return null;
  return invoke<AppInfo>("app_info");
}

export type Invoker = (cmd: string, args: Record<string, unknown>) => Promise<unknown>;

/** The backend command bridge, or null in a plain browser where there is no backend. */
export function getInvoker(): Invoker | null {
  if (!isTauri()) return null;
  return (cmd, args) => invoke(cmd, args);
}

export interface WindowControls {
  minimize(): Promise<void>;
  toggleMaximize(): Promise<void>;
  close(): Promise<void>;
}

/**
 * Controls for the borderless window (the window is created with `decorations: false`).
 * Null in a plain browser. Needs the window permissions in `src-tauri/capabilities`.
 */
export function getWindowControls(): WindowControls | null {
  if (!isTauri()) return null;
  try {
    const win = getCurrentWindow();
    return {
      minimize: () => win.minimize(),
      toggleMaximize: () => win.toggleMaximize(),
      close: () => win.close(),
    };
  } catch {
    return null;
  }
}
