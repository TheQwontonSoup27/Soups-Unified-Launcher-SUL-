import type { CapabilityEntry, Status } from "./types";

export const STATUS_LABEL: Record<Status, string> = {
  supported_tested: "Supported and tested",
  partial: "Partially supported",
  experimental: "Experimental",
  unsupported: "Unsupported",
  not_investigated: "Not investigated",
};

export const STATUS_HELP: Record<Status, string> = {
  supported_tested: "Test evidence exists for the stated environment/build.",
  partial: "A defined subset works; limitations are listed.",
  experimental: "Implemented or under test; reliability not established.",
  unsupported: "Known not to work or intentionally not provided.",
  not_investigated: "No adequate feasibility assessment yet.",
};

/** Mirrors sul-core's rule: "Supported and tested" needs a test date and evidence. */
export function validateCapability(c: CapabilityEntry): string | null {
  if (c.status === "supported_tested" && (!c.testDate?.trim() || !c.evidence?.trim())) {
    return `capability '${c.feature}' for '${c.game}' claims supported_tested without testDate and evidence`;
  }
  return null;
}

const base = (feature: string, game: string, status: Status, limitations: string[] = []): CapabilityEntry => ({
  feature, game, build: "*", os: "*", adapterVersion: "0", status, limitations,
});

/**
 * MOCK DATA for browser development only. Nothing here has been tested.
 * Real data will come from sul-core adapters through the Tauri bridge.
 */
export const MOCK_CAPABILITIES: CapabilityEntry[] = [
  base("Build isolation", "Helldivers 2", "not_investigated", ["Target build 7.1.1; separate adapter track"]),
  base("Offline launch", "Helldivers 2", "not_investigated", ["Feasibility unknown; no bypass of Steam auth, DRM or anti-cheat"]),
  base("tModLoader profiles", "Terraria", "not_investigated"),
  base("Mod profiles", "RimWorld", "not_investigated"),
  base("SMAPI profiles", "Stardew Valley", "not_investigated"),
];
