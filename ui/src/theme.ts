export interface ThemeTokens {
  id: string;
  label: string;
  bg: string; panel: string; text: string; muted: string; accent: string; border: string; onAccent: string;
}

export const THEMES: ThemeTokens[] = [
  { id: "dark-purple", label: "Dark purple", bg: "#10121a", panel: "#1b1e2b", text: "#f1f2f8", muted: "#a6a9bd", accent: "#8c7cff", border: "#2d3144", onAccent: "#10121a" },
  { id: "dark-green", label: "Dark green", bg: "#0f1512", panel: "#18221d", text: "#eef6f1", muted: "#a2b5aa", accent: "#4ed18f", border: "#2a3a31", onAccent: "#0f1512" },
  { id: "light", label: "Light", bg: "#f5f6fa", panel: "#ffffff", text: "#1a1c28", muted: "#555a72", accent: "#5b48e0", border: "#d5d8e6", onAccent: "#ffffff" },
  { id: "high-contrast", label: "High contrast", bg: "#000000", panel: "#0d0d0d", text: "#ffffff", muted: "#d0d0d0", accent: "#ffd400", border: "#ffffff", onAccent: "#000000" },
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

export function applyTheme(t: ThemeTokens, root: HTMLElement = document.documentElement): void {
  root.dataset.theme = t.id;
  const m: Record<string, string> = {
    "--bg": t.bg, "--panel": t.panel, "--text": t.text, "--muted": t.muted,
    "--accent": t.accent, "--border": t.border, "--on-accent": t.onAccent,
  };
  for (const [k, v] of Object.entries(m)) root.style.setProperty(k, v);
}

const KEY = "sul.theme";
export function loadThemeId(): string {
  try { return localStorage.getItem(KEY) ?? THEMES[0]!.id; } catch { return THEMES[0]!.id; }
}
export function saveThemeId(id: string): void {
  try { localStorage.setItem(KEY, id); } catch { /* storage unavailable: theme just won't persist */ }
}
