import { invoke, isTauri } from "@tauri-apps/api/core";
import type { Status } from "../types";

// ---------------------------------------------------------------------------
// Wire types. They mirror `sul_core::service` (serde, snake_case); the Rust side has a test
// that pins these names.
// ---------------------------------------------------------------------------

export type GameSource = "steam" | "gog" | "registry" | "drive_scan" | "emulated" | "manual";

export type Compatibility = "unverified" | "compatible" | "incompatible" | "not_applicable";

export type FeatureId =
  | "save_isolation"
  | "config_override"
  | "steam_headless"
  | "controller_remap"
  | "overlay"
  | "discord_rpc"
  | "mods"
  | "diagnostics";

export interface GameView {
  id: string;
  name: string;
  install_dir: string;
  source: GameSource;
  store_id: string | null;
  publisher: string | null;
  /** The app id the Play button hands to Steam. Null for every game that is not from Steam. */
  steam_app_id: number | null;
  /** Whether the most recent scan found the game. */
  present: boolean;
  headline: Status;
}

export interface ScanView {
  games: GameView[];
  added: number;
  updated: number;
  returned: number;
  missing: number;
}

export interface FeatureRowView {
  feature: FeatureId;
  label: string;
  /** The user's tick. */
  enabled: boolean;
  compatibility: Compatibility;
  /** The capability label, derived from the verdict by the core so it cannot overclaim. */
  status: Status;
  /** Whether ticking is allowed. Unticking always is. */
  can_enable: boolean;
  note: string | null;
}

export interface MatrixView {
  game_id: string;
  game_name: string;
  headline: Status;
  rows: FeatureRowView[];
}

// ---------------------------------------------------------------------------
// Backend
// ---------------------------------------------------------------------------

/** The commands of `src-tauri/src/commands.rs`. */
export interface LibraryApi {
  list(): Promise<GameView[]>;
  scan(): Promise<ScanView>;
  matrix(gameId: string): Promise<MatrixView>;
  tick(gameId: string, feature: FeatureId, enabled: boolean): Promise<MatrixView>;
  play(appId: number): Promise<void>;
}

/** The real backend, or null in a plain browser (`npm run dev`) where there is none. */
export function tauriLibraryApi(): LibraryApi | null {
  if (!isTauri()) return null;
  return {
    list: () => invoke<GameView[]>("list_games"),
    scan: () => invoke<ScanView>("scan_games_command"),
    matrix: (gameId) => invoke<MatrixView>("get_game_matrix", { gameId }),
    tick: (gameId, feature, enabled) => invoke<MatrixView>("update_feature_tick", { gameId, feature, enabled }),
    play: (appId) => invoke<void>("launch_game_command", { appId }),
  };
}

/** Errors from `invoke` are plain strings; anything else is described as well as it can be. */
export function messageOf(error: unknown): string {
  if (typeof error === "string") return error;
  if (error instanceof Error) return error.message;
  return "Something went wrong.";
}

// ---------------------------------------------------------------------------
// Labels
// ---------------------------------------------------------------------------

export const COMPAT_LABEL: Record<Compatibility, string> = {
  unverified: "Unverified",
  compatible: "Compatible",
  incompatible: "Incompatible",
  not_applicable: "Not applicable",
};

export const COMPAT_HELP: Record<Compatibility, string> = {
  unverified: "Nothing has checked this game yet. You can still tick the feature.",
  compatible: "Checked and expected to work with this game.",
  incompatible: "Known not to work with this game. It cannot be ticked.",
  not_applicable: "Can never apply to this game. It cannot be ticked.",
};

export const SOURCE_LABEL: Record<GameSource, string> = {
  steam: "Steam",
  gog: "GOG",
  registry: "Installed program",
  drive_scan: "Found on a drive",
  emulated: "Emulated",
  manual: "Added manually",
};

// ---------------------------------------------------------------------------
// Rules the UI shows (the core enforces them; this only explains them)
// ---------------------------------------------------------------------------

/** Why Play is unavailable for a game, or null when it can be pressed. */
export function playBlockedReason(game: GameView): string | null {
  if (game.steam_app_id === null) return "Only Steam games can be started from here for now.";
  if (!game.present) return "Not found on the last scan.";
  return null;
}

export function describeScan(scan: ScanView): string {
  const found = scan.games.filter((g) => g.present).length;
  const parts: string[] = [];
  if (scan.added > 0) parts.push(`${scan.added} new`);
  if (scan.returned > 0) parts.push(`${scan.returned} back`);
  if (scan.missing > 0) parts.push(`${scan.missing} no longer found`);
  const noun = found === 1 ? "game" : "games";
  return parts.length > 0 ? `Found ${found} ${noun} (${parts.join(", ")}).` : `Found ${found} ${noun}.`;
}

/**
 * The games that match a search. Every word typed must appear, in any order and in any case, in the
 * game's name, its publisher or the label of where it was found ("Steam", "GOG"...). An empty
 * search matches everything.
 */
export function filterGames(games: GameView[], query: string): GameView[] {
  const words = query.toLowerCase().split(/\s+/).filter((word) => word !== "");
  if (words.length === 0) return games;
  return games.filter((game) => {
    const text = `${game.name} ${game.publisher ?? ""} ${SOURCE_LABEL[game.source]}`.toLowerCase();
    return words.every((word) => text.includes(word));
  });
}

// ---------------------------------------------------------------------------
// Controller: all state and logic behind the library page, without any React or DOM.
// ---------------------------------------------------------------------------

export interface LibraryState {
  /** False until the first read of the saved library finished. */
  loaded: boolean;
  games: GameView[];
  /** Matrices read so far, by game id. */
  matrices: Record<string, MatrixView>;
  expanded: Record<string, true>;
  scanning: boolean;
  scanSummary: string | null;
  /** A problem with the library as a whole (reading it, scanning it). */
  error: string | null;
  /** A problem with one game's matrix, tick or launch. */
  gameErrors: Record<string, string>;
  /** Confirmations for one game, such as "Asked Steam to start it". */
  notices: Record<string, string>;
  /** Operations in flight: `${gameId}:${feature}`, `${gameId}:matrix` and `${gameId}:play`. */
  pending: Record<string, true>;
}

export const EMPTY_LIBRARY: LibraryState = {
  loaded: false,
  games: [],
  matrices: {},
  expanded: {},
  scanning: false,
  scanSummary: null,
  error: null,
  gameErrors: {},
  notices: {},
  pending: {},
};

/** What the grid and its cards may ask for. */
export interface LibraryActions {
  toggleExpanded(gameId: string): void;
  setTick(gameId: string, feature: FeatureId, enabled: boolean): void;
  play(game: GameView): void;
}

function without<T>(record: Record<string, T>, key: string): Record<string, T> {
  const { [key]: _dropped, ...rest } = record;
  return rest;
}

export function pendingKey(gameId: string, what: string): string {
  return `${gameId}:${what}`;
}

/**
 * Holds the library page's state. Works with `useSyncExternalStore`: `getSnapshot` returns the same
 * object until something changes. Every operation that fails leaves the data as it was and says so.
 */
export class LibraryController implements LibraryActions {
  private state: LibraryState = EMPTY_LIBRARY;
  private readonly listeners = new Set<() => void>();

  /** `api` is null when there is no backend (a plain browser). */
  constructor(private readonly api: LibraryApi | null) {}

  get hasBackend(): boolean {
    return this.api !== null;
  }

  subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  };

  getSnapshot = (): LibraryState => this.state;

  private set(patch: Partial<LibraryState>): void {
    this.state = { ...this.state, ...patch };
    for (const listener of this.listeners) listener();
  }

  private begin(key: string): boolean {
    if (this.state.pending[key]) return false;
    this.set({ pending: { ...this.state.pending, [key]: true } });
    return true;
  }

  private end(key: string): void {
    this.set({ pending: without(this.state.pending, key) });
  }

  /** Show a problem for one game. It replaces any confirmation the game showed. */
  private showError(gameId: string, message: string): void {
    this.set({
      gameErrors: { ...this.state.gameErrors, [gameId]: message },
      notices: without(this.state.notices, gameId),
    });
  }

  /** Show a confirmation for one game. It replaces any problem the game showed. */
  private showNotice(gameId: string, message: string): void {
    this.set({
      notices: { ...this.state.notices, [gameId]: message },
      gameErrors: without(this.state.gameErrors, gameId),
    });
  }

  private clearError(gameId: string): void {
    this.set({ gameErrors: without(this.state.gameErrors, gameId) });
  }

  /** Read the saved library. No scan: the games found last time are shown straight away. */
  async load(): Promise<void> {
    if (!this.api) {
      this.set({ loaded: true });
      return;
    }
    try {
      this.set({ games: await this.api.list(), loaded: true, error: null });
    } catch (error) {
      this.set({ loaded: true, error: messageOf(error) });
    }
  }

  /** Scan this machine, then refresh the matrices that are open. */
  async scan(): Promise<void> {
    if (!this.api || this.state.scanning) return;
    this.set({ scanning: true, error: null, scanSummary: null });
    try {
      const result = await this.api.scan();
      this.set({ games: result.games, scanning: false, scanSummary: describeScan(result) });
    } catch (error) {
      // The previous list stays; the scan simply did not happen.
      this.set({ scanning: false, error: messageOf(error) });
      return;
    }
    const known = new Set(this.state.games.map((g) => g.id));
    await Promise.all(Object.keys(this.state.expanded).filter((id) => known.has(id)).map((id) => this.loadMatrix(id)));
  }

  async loadMatrix(gameId: string): Promise<void> {
    if (!this.api) return;
    const key = pendingKey(gameId, "matrix");
    if (!this.begin(key)) return;
    try {
      const matrix = await this.api.matrix(gameId);
      this.set({ matrices: { ...this.state.matrices, [gameId]: matrix } });
      this.clearError(gameId);
    } catch (error) {
      this.showError(gameId, messageOf(error));
    } finally {
      this.end(key);
    }
  }

  toggleExpanded = (gameId: string): void => {
    if (this.state.expanded[gameId]) {
      this.set({ expanded: without(this.state.expanded, gameId) });
      return;
    }
    this.set({ expanded: { ...this.state.expanded, [gameId]: true } });
    if (!this.state.matrices[gameId]) void this.loadMatrix(gameId);
  };

  /** Tick or untick a feature. The backend decides whether that is allowed; the matrix shown is its answer. */
  setTick = (gameId: string, feature: FeatureId, enabled: boolean): void => {
    void this.tickAsync(gameId, feature, enabled);
  };

  async tickAsync(gameId: string, feature: FeatureId, enabled: boolean): Promise<void> {
    if (!this.api) return;
    const key = pendingKey(gameId, feature);
    if (!this.begin(key)) return;
    try {
      const matrix = await this.api.tick(gameId, feature, enabled);
      this.set({
        matrices: { ...this.state.matrices, [gameId]: matrix },
        games: this.state.games.map((g) => (g.id === gameId ? { ...g, headline: matrix.headline } : g)),
      });
      this.clearError(gameId);
    } catch (error) {
      this.showError(gameId, messageOf(error));
    } finally {
      this.end(key);
    }
  }

  play = (game: GameView): void => {
    void this.playAsync(game);
  };

  async playAsync(game: GameView): Promise<void> {
    if (!this.api) return;
    const blocked = playBlockedReason(game);
    if (blocked !== null || game.steam_app_id === null) {
      this.showError(game.id, blocked ?? "Only Steam games can be started from here for now.");
      return;
    }
    const key = pendingKey(game.id, "play");
    if (!this.begin(key)) return;
    try {
      await this.api.play(game.steam_app_id);
      this.showNotice(game.id, `Asked Steam to start ${game.name}.`);
    } catch (error) {
      this.showError(game.id, messageOf(error));
    } finally {
      this.end(key);
    }
  }
}
