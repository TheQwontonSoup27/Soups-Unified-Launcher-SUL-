export interface ThemeTokens {
  id: string;
  label: string;
  bg: string; panel: string; text: string; muted: string; accent: string; border: string; onAccent: string;
  titlebar: string; titlebarText: string;
  sidebar: string; sidebarText: string;
  button: string; buttonText: string;
  popup: string; popupText: string;
}
export type TokenKey = Exclude<keyof ThemeTokens, "id" | "label">;

export const TOKEN_LABELS: Record<TokenKey, string> = {
  bg: "App background", panel: "Cards / panels", text: "Text", muted: "Secondary text", accent: "Accent / focus ring",
  border: "Borders", onAccent: "Text on accent", titlebar: "Title bar", titlebarText: "Title bar text",
  sidebar: "Sidebar", sidebarText: "Sidebar text", button: "Buttons", buttonText: "Button text",
  popup: "Pop-up windows", popupText: "Pop-up text",
};
export const TOKEN_KEYS = Object.keys(TOKEN_LABELS) as TokenKey[];

type Base = Omit<ThemeTokens, "titlebar" | "titlebarText" | "sidebar" | "sidebarText" | "button" | "buttonText" | "popup" | "popupText">;
const classic = (b: Base): ThemeTokens => ({
  ...b, titlebar: b.panel, titlebarText: b.text, sidebar: b.bg, sidebarText: b.text,
  button: b.accent, buttonText: b.onAccent, popup: b.panel, popupText: b.text,
});

/** Team27 colours are sampled from the logo: green ~#1EA40C, purple ~#4D10B6. */
export const THEMES: ThemeTokens[] = [
  {
    id: "team27", label: "Team27 (green & purple)",
    bg: "#1ea40c", panel: "#e3f9de", text: "#0a140a", muted: "#2c4a27", accent: "#4d10b6", border: "#0c5a04", onAccent: "#ffffff",
    titlebar: "#4d10b6", titlebarText: "#ffffff",
    sidebar: "#146b0a", sidebarText: "#ffffff",
    button: "#b9a2ff", buttonText: "#000000",
    popup: "#27b014", popupText: "#000000",
  },
  classic({ id: "dark-purple", label: "Dark purple", bg: "#10121a", panel: "#1b1e2b", text: "#f1f2f8", muted: "#a6a9bd", accent: "#8c7cff", border: "#2d3144", onAccent: "#10121a" }),
  classic({ id: "dark-green", label: "Dark green", bg: "#0f1512", panel: "#18221d", text: "#eef6f1", muted: "#a2b5aa", accent: "#4ed18f", border: "#2a3a31", onAccent: "#0f1512" }),
  classic({ id: "light", label: "Light", bg: "#f5f6fa", panel: "#ffffff", text: "#1a1c28", muted: "#555a72", accent: "#5b48e0", border: "#d5d8e6", onAccent: "#ffffff" }),
  classic({ id: "high-contrast", label: "High contrast", bg: "#000000", panel: "#0d0d0d", text: "#ffffff", muted: "#d0d0d0", accent: "#ffd400", border: "#ffffff", onAccent: "#000000" }),
];

/** Foreground/background pairs that must stay readable (WCAG AA, 4.5:1). */
export const CONTRAST_PAIRS: Array<[TokenKey, TokenKey, string]> = [
  ["text", "bg", "Text on app background"],
  ["text", "panel", "Text on cards"],
  ["muted", "panel", "Secondary text on cards"],
  ["titlebarText", "titlebar", "Title bar text"],
  ["sidebarText", "sidebar", "Sidebar text"],
  ["buttonText", "button", "Button text"],
  ["popupText", "popup", "Pop-up text"],
  ["onAccent", "accent", "Text on accent"],
];

function lum(hex: string): number {
  const n = parseInt(hex.slice(1), 16);
  const ch = [(n >> 16) & 255, (n >> 8) & 255, n & 255].map((v) => {
    const s = v / 255;
    return s <= 0.03928 ? s / 12.92 : Math.pow((s + 0.055) / 1.055, 2.4);
  });
  return 0.2126 * ch[0]! + 0.7152 * ch[1]! + 0.0722 * ch[2]!;
}

/** WCAG contrast ratio between two #rrggbb colours. */
export function contrast(a: string, b: string): number {
  const [hi, lo] = [lum(a), lum(b)].sort((x, y) => y - x) as [number, number];
  return (hi + 0.05) / (lo + 0.05);
}

export interface PairResult { label: string; ratio: number; ok: boolean }
export function checkPairs(t: ThemeTokens): PairResult[] {
  return CONTRAST_PAIRS.map(([fg, bg, label]) => {
    const ratio = contrast(t[fg], t[bg]);
    return { label, ratio, ok: ratio >= 4.5 };
  });
}

const kebab = (k: string) => k.replace(/[A-Z]/g, (m) => "-" + m.toLowerCase());

export function applyTheme(t: ThemeTokens, root: HTMLElement = document.documentElement): void {
  root.dataset.theme = t.id;
  for (const k of TOKEN_KEYS) root.style.setProperty("--" + kebab(k), t[k]);
}

const ID_KEY = "sul.theme";
const OVR_KEY = "sul.colors";
const HEX = /^#[0-9a-fA-F]{6}$/;

export function loadThemeId(): string {
  try { return localStorage.getItem(ID_KEY) ?? THEMES[0]!.id; } catch { return THEMES[0]!.id; }
}
export function saveThemeId(id: string): void {
  try { localStorage.setItem(ID_KEY, id); } catch { /* storage unavailable: just won't persist */ }
}
export function loadOverrides(): Partial<Record<TokenKey, string>> {
  try {
    const raw = JSON.parse(localStorage.getItem(OVR_KEY) ?? "{}") as Record<string, unknown>;
    const out: Partial<Record<TokenKey, string>> = {};
    for (const k of TOKEN_KEYS) { const v = raw[k]; if (typeof v === "string" && HEX.test(v)) out[k] = v; }
    return out;
  } catch { return {}; }
}
export function saveOverrides(o: Partial<Record<TokenKey, string>>): void {
  try { localStorage.setItem(OVR_KEY, JSON.stringify(o)); } catch { /* ignore */ }
}
export function effectiveTheme(id: string, overrides: Partial<Record<TokenKey, string>>): ThemeTokens {
  const base = THEMES.find((t) => t.id === id) ?? THEMES[0]!;
  const merged: ThemeTokens = { ...base };
  for (const k of TOKEN_KEYS) { const v = overrides[k]; if (v) merged[k] = v; }
  return merged;
}
