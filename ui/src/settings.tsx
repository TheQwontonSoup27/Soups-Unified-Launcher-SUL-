import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import {
  DEFAULT_BG, MAX_CUSTOM_BYTES, clearCustomBg, isAcceptedType, loadBg, loadCustomBg, loadManifest, saveBg, saveCustomBg,
  type BgSettings, type Wallpaper,
} from "./background";
import {
  TOKEN_KEYS, THEMES, applyTheme, effectiveTheme, loadOverrides, loadThemeId, saveOverrides, saveThemeId,
  type ThemeTokens, type TokenKey,
} from "./theme";

export interface CustomMedia { url: string; type: string; name: string }

export interface SettingsApi {
  themeId: string;
  overrides: Partial<Record<TokenKey, string>>;
  theme: ThemeTokens;
  setThemeId: (id: string) => void;
  setColor: (k: TokenKey, hex: string) => void;
  resetColors: () => void;
  bg: BgSettings;
  setBg: (p: Partial<BgSettings>) => void;
  wallpapers: Wallpaper[];
  custom: CustomMedia | null;
  /** Returns an error message, or null on success. */
  setCustomFile: (f: File) => Promise<string | null>;
  removeCustom: () => Promise<void>;
}

const noop = () => {};
const DEFAULT: SettingsApi = {
  themeId: THEMES[0]!.id, overrides: {}, theme: THEMES[0]!, setThemeId: noop, setColor: noop, resetColors: noop,
  bg: DEFAULT_BG, setBg: noop, wallpapers: [], custom: null, setCustomFile: async () => "unavailable", removeCustom: async () => {},
};
const Ctx = createContext<SettingsApi>(DEFAULT);
export const useSettings = () => useContext(Ctx);

export function SettingsProvider({ children }: { children: ReactNode }) {
  const [themeId, setThemeIdState] = useState(loadThemeId);
  const [overrides, setOverrides] = useState(loadOverrides);
  const [bg, setBgState] = useState(loadBg);
  const [wallpapers, setWallpapers] = useState<Wallpaper[]>([]);
  const [custom, setCustom] = useState<CustomMedia | null>(null);
  const urlRef = useRef<string | null>(null);

  const theme = useMemo(() => effectiveTheme(themeId, overrides), [themeId, overrides]);
  useEffect(() => { applyTheme(theme); }, [theme]);

  useEffect(() => {
    void loadManifest().then(setWallpapers);
    void loadCustomBg().then((c) => { if (c) show(c.blob, c.type, c.name); });
    return () => { if (urlRef.current) URL.revokeObjectURL(urlRef.current); };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  function show(blob: Blob, type: string, name: string) {
    if (urlRef.current) URL.revokeObjectURL(urlRef.current);
    const url = URL.createObjectURL(blob);
    urlRef.current = url;
    setCustom({ url, type, name });
  }

  const setThemeId = useCallback((id: string) => { saveThemeId(id); setThemeIdState(id); }, []);
  const setColor = useCallback((k: TokenKey, hex: string) => {
    setOverrides((o) => { const n = { ...o, [k]: hex }; saveOverrides(n); return n; });
  }, []);
  const resetColors = useCallback(() => { saveOverrides({}); setOverrides({}); }, []);
  const setBg = useCallback((p: Partial<BgSettings>) => {
    setBgState((b) => { const n = { ...b, ...p }; saveBg(n); return n; });
  }, []);

  const setCustomFile = useCallback(async (f: File): Promise<string | null> => {
    if (!isAcceptedType(f.type)) return "Unsupported file type. Use PNG, JPG, WebP, GIF, MP4 or WebM.";
    if (f.size > MAX_CUSTOM_BYTES) return "File is larger than 300 MB.";
    const ok = await saveCustomBg({ blob: f, type: f.type, name: f.name });
    show(f, f.type, f.name);
    setBg({ mode: "custom" });
    return ok ? null : "Shown for this session only: the browser would not store the file.";
  }, [setBg]);

  const removeCustom = useCallback(async () => {
    await clearCustomBg();
    if (urlRef.current) URL.revokeObjectURL(urlRef.current);
    urlRef.current = null;
    setCustom(null);
    setBg({ mode: "solid" });
  }, [setBg]);

  const api: SettingsApi = { themeId, overrides, theme, setThemeId, setColor, resetColors, bg, setBg, wallpapers, custom, setCustomFile, removeCustom };
  return <Ctx.Provider value={api}>{children}</Ctx.Provider>;
}

export { TOKEN_KEYS };
