// Display density: how much detail a panel of measurements shows. Compact is one line per item with a
// "Details" button that opens the full panel in place; Detailed shows the full panel straight away.
//
// One global setting (Settings > Appearance) and, per module, an override: Inherit Global, Always Compact or
// Always Detailed. The module's own strip and the Settings card edit the same stored value, so there are two ways to
// reach one setting. The view a module shows is its own choice, unless that is Inherit, then the global one.
//
// Pure functions only (no React, no DOM); `densityContext.tsx` holds the state.

import type { Status } from "./types";

export type Density = "compact" | "detailed";
export type DensityChoice = "inherit" | Density;

export interface DensityModule {
  /** A stable id, kept in storage. It is not a page id: the engine's settings are not the Capture & Clips page. */
  id: string;
  /** What the screen calls it. */
  label: string;
  /** What the capability matrix says about it. */
  status: Status;
}

/**
 * The modules that can have their own density: the ones that show measurements. A module is added here when it
 * gets something to measure; only these ids are read back from storage.
 */
export const DENSITY_MODULES: readonly DensityModule[] = [
  { id: "clipping-engine", label: "Media & Clipping Engine", status: "not_investigated" },
];

export interface DensitySettings {
  global: Density;
  /** Only real overrides. A module that inherits has no entry. */
  overrides: Record<string, Density>;
}

export const DEFAULT_DENSITY: DensitySettings = { global: "compact", overrides: {} };

export const DENSITY_LABEL: Record<Density, string> = { compact: "Compact", detailed: "Detailed" };
export const CHOICES: readonly DensityChoice[] = ["inherit", "compact", "detailed"];
export const CHOICE_LABEL: Record<DensityChoice, string> = {
  inherit: "Inherit Global",
  compact: "Always Compact",
  detailed: "Always Detailed",
};

export const isDensity = (value: unknown): value is Density => value === "compact" || value === "detailed";
export const isDensityModule = (id: string): boolean => DENSITY_MODULES.some((m) => m.id === id);

const has = (record: object, key: string): boolean => Object.prototype.hasOwnProperty.call(record, key);

/** What the module's choice is: its override, or Inherit when it has none. */
export function choiceFor(settings: DensitySettings, moduleId: string): DensityChoice {
  return has(settings.overrides, moduleId) ? settings.overrides[moduleId]! : "inherit";
}

/** The view the module actually shows. */
export function effectiveDensity(settings: DensitySettings, moduleId: string): Density {
  return has(settings.overrides, moduleId) ? settings.overrides[moduleId]! : settings.global;
}

/** "Compact (from Global)" or "Detailed (set for this module)": where the view the module shows comes from. */
export function describeEffective(settings: DensitySettings, moduleId: string): string {
  const density = DENSITY_LABEL[effectiveDensity(settings, moduleId)];
  return `${density} (${choiceFor(settings, moduleId) === "inherit" ? "from Global" : "set for this module"})`;
}

export function withGlobal(settings: DensitySettings, global: Density): DensitySettings {
  return { ...settings, global };
}

/** Set a module's choice. Inherit removes its override. An id that is not a density module changes nothing. */
export function withChoice(settings: DensitySettings, moduleId: string, choice: DensityChoice): DensitySettings {
  if (!isDensityModule(moduleId)) return settings;
  const overrides = { ...settings.overrides };
  if (choice === "inherit") delete overrides[moduleId];
  else overrides[moduleId] = choice;
  return { ...settings, overrides };
}

export function withoutOverrides(settings: DensitySettings): DensitySettings {
  return { ...settings, overrides: {} };
}

export interface OverrideInUse {
  id: string;
  label: string;
  density: Density;
}

/** The overrides that are set, in the order of `DENSITY_MODULES`. */
export function overridesInUse(settings: DensitySettings): OverrideInUse[] {
  return DENSITY_MODULES.filter((m) => has(settings.overrides, m.id)).map((m) => ({
    id: m.id,
    label: m.label,
    density: settings.overrides[m.id]!,
  }));
}

// ---- storage (localStorage; may be missing or blocked, in which case nothing persists) ----

const KEY = "sul.density";

/** Read the saved settings. Anything wrong or unknown in them falls back to the default for that part. */
export function loadDensity(): DensitySettings {
  try {
    const raw: unknown = JSON.parse(localStorage.getItem(KEY) ?? "null");
    if (typeof raw !== "object" || raw === null || Array.isArray(raw)) return { ...DEFAULT_DENSITY, overrides: {} };
    const saved = raw as { global?: unknown; overrides?: unknown };
    const overrides: Record<string, Density> = {};
    if (typeof saved.overrides === "object" && saved.overrides !== null && !Array.isArray(saved.overrides)) {
      for (const m of DENSITY_MODULES) {
        if (!has(saved.overrides, m.id)) continue;
        const value = (saved.overrides as Record<string, unknown>)[m.id];
        if (isDensity(value)) overrides[m.id] = value;
      }
    }
    return { global: isDensity(saved.global) ? saved.global : DEFAULT_DENSITY.global, overrides };
  } catch {
    return { ...DEFAULT_DENSITY, overrides: {} };
  }
}

export function saveDensity(settings: DensitySettings): void {
  try {
    localStorage.setItem(KEY, JSON.stringify(settings));
  } catch {
    /* storage unavailable: the choice holds for this session only */
  }
}
