import { createContext, useCallback, useContext, useMemo, useState, type ReactNode } from "react";
import {
  DEFAULT_DENSITY, loadDensity, saveDensity, withChoice, withGlobal, withoutOverrides,
  type Density, type DensityChoice, type DensitySettings,
} from "./density";

export interface DensityApi {
  settings: DensitySettings;
  setGlobal: (density: Density) => void;
  setChoice: (moduleId: string, choice: DensityChoice) => void;
  clearOverrides: () => void;
}

const noop = () => {};
const DEFAULT: DensityApi = { settings: DEFAULT_DENSITY, setGlobal: noop, setChoice: noop, clearOverrides: noop };
const Ctx = createContext<DensityApi>(DEFAULT);
export const useDensity = () => useContext(Ctx);

/** Owns the display density: the global view and each module's override. Saved on every change. */
export function DensityProvider({ children }: { children: ReactNode }) {
  const [settings, setSettings] = useState(loadDensity);

  const update = useCallback((change: (current: DensitySettings) => DensitySettings) => {
    setSettings((current) => {
      const next = change(current);
      if (next !== current) saveDensity(next);
      return next;
    });
  }, []);

  const api = useMemo<DensityApi>(
    () => ({
      settings,
      setGlobal: (density) => update((s) => withGlobal(s, density)),
      setChoice: (moduleId, choice) => update((s) => withChoice(s, moduleId, choice)),
      clearOverrides: () => update(withoutOverrides),
    }),
    [settings, update],
  );
  return <Ctx.Provider value={api}>{children}</Ctx.Provider>;
}
