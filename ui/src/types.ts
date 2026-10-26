export type Status = "supported_tested" | "partial" | "experimental" | "unsupported" | "not_investigated";

export interface CapabilityEntry {
  feature: string;
  game: string;
  build: string;
  os: string;
  adapterVersion: string;
  status: Status;
  testDate?: string;
  evidence?: string;
  limitations: string[];
}

export interface ModuleDef {
  id: string;
  title: string;
  glyph: string;
  summary: string;
  /** Status of the module as a whole. Must be truthful: nothing is tested yet. */
  status: Status;
  specRef: string;
  /** Where the module lives in the shell: the Game Library page, its own sidebar entry, or a Settings pane. */
  home: "library" | "sidebar" | "settings";
}
