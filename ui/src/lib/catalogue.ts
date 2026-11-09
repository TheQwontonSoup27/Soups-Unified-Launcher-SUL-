import { invoke, isTauri } from "@tauri-apps/api/core";
import { messageOf, type GameView } from "./library";
import type { BuildState } from "./manager";
import type { LaunchState } from "./playing";
import type { Status } from "../types";

// ---------------------------------------------------------------------------
// Wire types. They mirror `sul_core::catalogue` (the detail) and the `cataloguing` half of `sul_core::manager` (serde).
// `src-tauri/src/ipc_tests.rs` pins the field names from the Rust side.
// ---------------------------------------------------------------------------

export interface CategoryView {
  /** The first spelling the owner gave; a repeat that differs only in ASCII case is the same category. */
  name: string;
  count: number;
  /** Game ids, in the library's order. */
  game_ids: string[];
}

export interface EntryView {
  game_id: string;
  categories: string[];
}

export interface CatalogueView {
  categories: CategoryView[];
  entries: EntryView[];
}

export interface BuildSummary {
  build_id: string;
  slot: string;
  state: BuildState;
  /** A copy SUL manages (true) or an install that stays where it is (false). */
  managed: boolean;
  active: boolean;
}

export interface AdapterInfo {
  id: string;
  version: string;
}

export interface GameDetail {
  game_id: string;
  display_name: string;
  categories: string[];
  /** Where the owner's image file is. A reference only: SUL never copies it. */
  artwork: string | null;
  builds: BuildSummary[];
  adapter: AdapterInfo | null;
  /** The weakest capability label across the game's adapter entries; Not investigated with no adapter. */
  capability: Status;
  capability_label: string;
  launch: LaunchState;
  /** Plain statements about what is missing, so an empty section is explained. */
  notes: string[];
}

export interface Edited {
  detail: GameDetail;
  view: CatalogueView;
}

export interface CatalogueApi {
  view(): Promise<CatalogueView>;
  detail(gameId: string): Promise<GameDetail>;
  /** Replaces the whole list. */
  setCategories(gameId: string, categories: string[]): Promise<Edited>;
  /** A path to an image file, or null to clear the reference. */
  setArtwork(gameId: string, path: string | null): Promise<Edited>;
}

const UNEXPECTED = "The app sent catalogue data in a shape this page does not understand, so it was not used.";

const isObject = (v: unknown): v is Record<string, unknown> => typeof v === "object" && v !== null;
const isStrings = (v: unknown): v is string[] => Array.isArray(v) && v.every((s) => typeof s === "string");

/** Whether the backend sent a view of the shape the page reads. A wrong shape would otherwise crash the page. */
export function isCatalogueView(v: unknown): v is CatalogueView {
  return (
    isObject(v) &&
    Array.isArray(v.categories) &&
    v.categories.every((c) => isObject(c) && typeof c.name === "string" && typeof c.count === "number" && isStrings(c.game_ids)) &&
    Array.isArray(v.entries) &&
    v.entries.every((e) => isObject(e) && typeof e.game_id === "string" && isStrings(e.categories))
  );
}

export const isGameDetail = (v: unknown): v is GameDetail =>
  isObject(v) &&
  typeof v.game_id === "string" &&
  typeof v.display_name === "string" &&
  isStrings(v.categories) &&
  (v.artwork === null || typeof v.artwork === "string") &&
  Array.isArray(v.builds) &&
  v.builds.every((b) => isObject(b) && typeof b.build_id === "string" && typeof b.state === "string") &&
  (v.adapter === null || (isObject(v.adapter) && typeof v.adapter.id === "string")) &&
  typeof v.capability === "string" &&
  typeof v.capability_label === "string" &&
  isObject(v.launch) &&
  typeof v.launch.state === "string" &&
  isStrings(v.notes);

function expectView(v: unknown): CatalogueView {
  if (!isCatalogueView(v)) throw new Error(UNEXPECTED);
  return v;
}

function expectDetail(v: unknown): GameDetail {
  if (!isGameDetail(v)) throw new Error(UNEXPECTED);
  return v;
}

function expectEdited(v: unknown): Edited {
  if (!isObject(v) || !isGameDetail(v.detail) || !isCatalogueView(v.view)) throw new Error(UNEXPECTED);
  return v as unknown as Edited;
}

/** The real backend, or null in a plain browser where there is none. */
export function tauriCatalogueApi(): CatalogueApi | null {
  if (!isTauri()) return null;
  return {
    view: async () => expectView(await invoke<unknown>("catalogue_view")),
    detail: async (gameId) => expectDetail(await invoke<unknown>("game_detail", { gameId })),
    setCategories: async (gameId, categories) => expectEdited(await invoke<unknown>("catalogue_set_categories", { gameId, categories })),
    setArtwork: async (gameId, path) => expectEdited(await invoke<unknown>("catalogue_set_artwork", { gameId, path })),
  };
}

// ---------------------------------------------------------------------------
// Small rules the page applies (the core enforces the real ones)
// ---------------------------------------------------------------------------

/** The core compares categories ignoring ASCII case only (`eq_ignore_ascii_case`), so the page does the same. */
export const sameCategory = (a: string, b: string): boolean => lowerAscii(a) === lowerAscii(b);
const lowerAscii = (s: string): string => s.replace(/[A-Z]/g, (c) => c.toLowerCase());

/** The view's own spelling of a category, found ignoring ASCII case, or null when no game carries it any more. */
export function findCategory(view: CatalogueView | null, name: string | null): CategoryView | null {
  if (view === null || name === null) return null;
  return view.categories.find((c) => sameCategory(c.name, name)) ?? null;
}

/** The games that carry the chosen category, in the order given. No choice shows every game. */
export function applyCategory(games: GameView[], view: CatalogueView | null, selected: string | null): GameView[] {
  if (selected === null) return games;
  const found = findCategory(view, selected);
  if (found === null) return games;
  const ids = new Set(found.game_ids);
  return games.filter((g) => ids.has(g.id));
}

/** The categories one game carries, from the view. */
export function categoriesOf(view: CatalogueView | null, gameId: string): string[] {
  return view?.entries.find((e) => e.game_id === gameId)?.categories ?? [];
}

/**
 * The sentence above the grid while a filter is on, or null when none is. The search-only wording is the one the page
 * always had.
 */
export function filterSummary(query: string, category: string | null, shown: number, total: number): string | null {
  const q = query.trim();
  if (q === "" && category === null) return null;
  if (shown === 0) {
    if (q === "") return `No games are in “${category}”.`;
    return category === null ? `No games match “${q}”.` : `No games match “${q}” in “${category}”.`;
  }
  return `Showing ${shown} of ${total} games${q === "" ? "" : ` matching “${q}”`}${category === null ? "" : ` in “${category}”`}.`;
}

/** Windows gives a local path as `\\?\C:\…` after it is resolved; the owner typed `C:\…`, so that is what is shown. */
export function plainPath(path: string): string {
  return path.replace(/^\\\\\?\\(?=[A-Za-z]:\\)/, "");
}

// ---------------------------------------------------------------------------
// The controller
// ---------------------------------------------------------------------------

export interface CatalogueState {
  loaded: boolean;
  view: CatalogueView | null;
  /** The category the grid is filtered by, or null for every game. */
  selected: string | null;
  /** The game whose details are open, or null. */
  openId: string | null;
  /** The detail of `openId`, once it has arrived. */
  detail: GameDetail | null;
  /** What is running, or null. */
  busy: string | null;
  /** A problem with the category list itself. */
  error: string | null;
  /** A problem with the open game's details or an edit of them. */
  detailError: string | null;
  notice: string | null;
}

export const EMPTY_CATALOGUE: CatalogueState = { loaded: false, view: null, selected: null, openId: null, detail: null, busy: null, error: null, detailError: null, notice: null };

export class CatalogueController {
  private state: CatalogueState = EMPTY_CATALOGUE;
  private readonly listeners = new Set<() => void>();

  /** `api` is null when there is no backend (a plain browser). */
  constructor(private readonly api: CatalogueApi | null) {}

  get hasBackend(): boolean {
    return this.api !== null;
  }

  subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  };

  getSnapshot = (): CatalogueState => this.state;

  private set(patch: Partial<CatalogueState>): void {
    this.state = { ...this.state, ...patch };
    for (const listener of this.listeners) listener();
  }

  /** Take a new view, keeping the chosen category chosen under the spelling the view has now, or dropping it when no game carries it. */
  private withView(view: CatalogueView): Partial<CatalogueState> {
    const found = findCategory(view, this.state.selected);
    return { view, selected: found === null ? null : found.name };
  }

  async load(): Promise<void> {
    if (!this.api) {
      this.set({ loaded: true });
      return;
    }
    try {
      this.set({ ...this.withView(await this.api.view()), loaded: true, error: null });
    } catch (error) {
      this.set({ loaded: true, error: messageOf(error) });
    }
  }

  /** Filter the grid by a category, or by none with null. A name no game carries selects nothing. */
  select(category: string | null): void {
    const found = findCategory(this.state.view, category);
    this.set({ selected: found === null ? null : found.name });
  }

  /** Open the details of a game (or close them when they are already open). The newest answer wins. */
  async open(gameId: string): Promise<void> {
    const api = this.api;
    if (!api) return;
    if (this.state.openId === gameId) {
      this.close();
      return;
    }
    this.set({ openId: gameId, detail: null, detailError: null, notice: null });
    await this.fetchDetail(api, gameId);
  }

  /** Ask for the open game's details again (what runs or which build is active may have changed). */
  async refreshDetail(): Promise<void> {
    const api = this.api;
    const id = this.state.openId;
    if (!api || id === null || this.state.busy !== null) return;
    this.set({ detailError: null, notice: null });
    await this.fetchDetail(api, id);
  }

  private async fetchDetail(api: CatalogueApi, gameId: string): Promise<void> {
    try {
      const detail = await api.detail(gameId);
      if (this.state.openId === gameId) this.set({ detail });
    } catch (error) {
      if (this.state.openId === gameId) this.set({ detailError: messageOf(error) });
    }
  }

  close(): void {
    this.set({ openId: null, detail: null, detailError: null, notice: null });
  }

  /** Add a category to the open game. A blank name or one the game already has is turned away here and nothing is sent. */
  async addCategory(text: string): Promise<boolean> {
    const detail = this.state.detail;
    const name = text.trim();
    if (name === "") {
      this.set({ detailError: "Type a category name first.", notice: null });
      return false;
    }
    if (detail === null) return false;
    if (detail.categories.some((c) => sameCategory(c, name))) {
      this.set({ detailError: null, notice: `${detail.display_name} already has “${name}”.` });
      return false;
    }
    return this.edit("categories", (api, id) => api.setCategories(id, [...detail.categories, name]), `Added “${name}”.`);
  }

  async removeCategory(name: string): Promise<boolean> {
    const detail = this.state.detail;
    if (detail === null) return false;
    return this.edit("categories", (api, id) => api.setCategories(id, detail.categories.filter((c) => !sameCategory(c, name))), `Removed “${name}”.`);
  }

  /** Point the open game at an image file. A blank path is turned away here and nothing is sent. */
  async setArtwork(path: string): Promise<boolean> {
    const p = path.trim();
    if (p === "") {
      this.set({ detailError: "Type the path of an image file first.", notice: null });
      return false;
    }
    return this.edit("artwork", (api, id) => api.setArtwork(id, p), "Artwork reference saved. The image is not copied.");
  }

  async clearArtwork(): Promise<boolean> {
    return this.edit("artwork", (api, id) => api.setArtwork(id, null), "Artwork reference cleared.");
  }

  /** One edit at a time. The answer carries the detail and the category view after it, so both are replaced together. */
  private async edit(what: string, run: (api: CatalogueApi, gameId: string) => Promise<Edited>, done: string): Promise<boolean> {
    const api = this.api;
    const id = this.state.openId;
    if (!api || id === null || this.state.busy !== null) return false;
    this.set({ busy: what, detailError: null, notice: null });
    try {
      const edited = await run(api, id);
      this.set({ ...this.withView(edited.view), detail: this.state.openId === id ? edited.detail : this.state.detail, busy: null, notice: done });
      return true;
    } catch (error) {
      this.set({ busy: null, detailError: messageOf(error) });
      return false;
    }
  }
}
