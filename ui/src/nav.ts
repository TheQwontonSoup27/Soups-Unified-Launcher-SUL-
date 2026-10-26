import { SIDEBAR_MODULES } from "./modules/registry";

export type ViewId = "library" | "settings" | "playtime" | `module:${string}`;

export interface NavItem {
  id: ViewId;
  label: string;
}

// Spec section 2, top to bottom: Game Library first, then (after the spacer) Settings
// second from the bottom, and General Playtime Tracker as the absolute bottom row.
export const TOP_NAV: NavItem[] = [{ id: "library", label: "Game Library" }];

/** Roadmap modules that have their own page (long-term spec, section 3). Placeholders until built. */
export const MODULE_NAV: NavItem[] = SIDEBAR_MODULES.map((m) => ({ id: `module:${m.id}`, label: m.title }));

export const BOTTOM_NAV: NavItem[] = [
  { id: "settings", label: "Settings" },
  { id: "playtime", label: "General Playtime Tracker" },
];
