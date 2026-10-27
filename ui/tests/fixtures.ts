import type { Compatibility, FeatureId, FeatureRowView, GameView, MatrixView } from "../src/lib/library";

export const steamGame: GameView = {
  id: "steam-553850",
  name: "Helldivers 2",
  install_dir: "D:\\SteamLibrary\\steamapps\\common\\Helldivers 2",
  source: "steam",
  store_id: "553850",
  publisher: null,
  steam_app_id: 553850,
  present: true,
  headline: "not_investigated",
};

export const gogGame: GameView = {
  id: "gog-1",
  name: "Alpha",
  install_dir: "D:\\GOG Games\\Alpha",
  source: "gog",
  store_id: "1",
  publisher: "Alpha Studio",
  steam_app_id: null,
  present: true,
  headline: "unsupported",
};

export const FEATURES: Array<[FeatureId, string]> = [
  ["save_isolation", "Save isolation"],
  ["config_override", "Config override"],
  ["steam_headless", "Steam headless launch"],
  ["controller_remap", "Controller remapping"],
  ["overlay", "Console overlay"],
  ["discord_rpc", "Discord Rich Presence"],
  ["mods", "Mods"],
  ["diagnostics", "Diagnostics"],
];

/** A matrix as the core produces it: Steam headless is not applicable to a game that is not from Steam. */
export function matrixFor(game: GameView, ticked: FeatureId[] = []): MatrixView {
  const rows: FeatureRowView[] = FEATURES.map(([feature, label]) => {
    const notApplicable = feature === "steam_headless" && game.source !== "steam";
    const compatibility: Compatibility = notApplicable ? "not_applicable" : "unverified";
    return {
      feature,
      label,
      enabled: ticked.includes(feature),
      compatibility,
      status: notApplicable ? "unsupported" : "not_investigated",
      can_enable: !notApplicable,
      note: notApplicable ? "Steam titles only" : null,
    };
  });
  return { game_id: game.id, game_name: game.name, headline: game.headline, rows };
}

