import { invoke, isTauri } from "@tauri-apps/api/core";
import { messageOf, type GameView } from "./library";

// ---------------------------------------------------------------------------
// Wire types. They mirror `sul_core::manager` (serde, snake_case). `src-tauri/src/ipc_tests.rs` pins the
// field names from the Rust side.
// ---------------------------------------------------------------------------

export type RootKind = "builds" | "mods";

export interface RootView {
  id: string;
  label: string;
  path: string;
  kind: RootKind;
  /** The folder exists. */
  present: boolean;
  /** The folder holds this folder's marker. False while `present` means another folder or drive is there. */
  marker_ok: boolean;
  free_bytes: number | null;
}

export type Variant = "original" | "optimized" | "legacy" | "experimental" | { custom: string };

export type BuildState = "staging" | "unverified" | "ready" | "failed";

export type Location = { kind: "managed"; root_id: string; dir: string } | { kind: "external_install"; path: string };

export interface BuildView {
  id: string;
  game_id: string;
  slot: string;
  variant: Variant;
  location: Location;
  state: BuildState;
  active: boolean;
}

export interface ActiveSlot {
  game_id: string;
  slot: string;
  build_id: string;
  /** Earlier active builds of the slot, most recent last. */
  history: string[];
}

export interface RetiredCopy {
  build_id: string;
  game_id: string;
  location: Location;
  retired_unix: number;
  reason: string;
}

export interface InterruptedView {
  id: string;
  op: string;
  detail: string;
}

export interface ManagerView {
  roots: RootView[];
  builds: BuildView[];
  slots: ActiveSlot[];
  retired: RetiredCopy[];
  interrupted: InterruptedView[];
}

export interface VerifyReport {
  missing: string[];
  modified: string[];
  extra: string[];
}

export interface BuildCheck {
  state: BuildState;
  report: VerifyReport;
}

export interface VerifiedView {
  check: BuildCheck;
  view: ManagerView;
}

export interface BuiltView {
  build_id: string;
  view: ManagerView;
}

/** What an import or a record-in-place is asked to do. The folder is the library game's, found by the backend. */
export interface BuildRequest {
  gameId: string;
  variant: Variant;
  baseVersion: string;
  sourceDescription: string;
}

// ---------------------------------------------------------------------------
// Backend
// ---------------------------------------------------------------------------

/** The commands of `src-tauri/src/commands.rs` that belong to library folders and managed builds. */
export interface ManagerApi {
  view(): Promise<ManagerView>;
  addRoot(id: string, label: string, path: string, kind: RootKind): Promise<ManagerView>;
  relinkRoot(id: string, path: string): Promise<ManagerView>;
  forgetRoot(id: string): Promise<ManagerView>;
  importBuild(req: BuildRequest, rootId: string): Promise<BuiltView>;
  registerInPlace(req: BuildRequest): Promise<BuiltView>;
  verify(buildId: string): Promise<VerifiedView>;
  activate(buildId: string): Promise<ManagerView>;
  rollback(gameId: string, slot: string): Promise<ManagerView>;
}

/** Whether the backend sent a list of the shape the page reads. A wrong shape would otherwise crash the page. */
export function isManagerView(v: unknown): v is ManagerView {
  if (typeof v !== "object" || v === null) return false;
  const o = v as Record<string, unknown>;
  return ["roots", "builds", "slots", "retired", "interrupted"].every((k) => Array.isArray(o[k]));
}

const UNEXPECTED = "The app sent folders-and-builds data in a shape this page does not understand, so it was not used.";

function expectView(v: unknown): ManagerView {
  if (!isManagerView(v)) throw new Error(UNEXPECTED);
  return v;
}

function expectBuilt(v: unknown): BuiltView {
  const o = v as Record<string, unknown> | null;
  if (typeof o !== "object" || o === null || typeof o.build_id !== "string" || !isManagerView(o.view)) throw new Error(UNEXPECTED);
  return v as BuiltView;
}

function expectVerified(v: unknown): VerifiedView {
  const o = v as Record<string, unknown> | null;
  const check = o?.check as Record<string, unknown> | null | undefined;
  const report = check?.report as Record<string, unknown> | null | undefined;
  const lists = report !== null && typeof report === "object" && ["missing", "modified", "extra"].every((k) => Array.isArray(report[k]));
  if (typeof o !== "object" || o === null || !lists || !isManagerView(o.view)) throw new Error(UNEXPECTED);
  return v as VerifiedView;
}

/** The real backend, or null in a plain browser where there is none. */
export function tauriManagerApi(): ManagerApi | null {
  if (!isTauri()) return null;
  const view = async (cmd: string, args?: Record<string, unknown>) => expectView(await invoke<unknown>(cmd, args));
  return {
    view: () => view("manager_view"),
    addRoot: (id, label, path, kind) => view("manager_add_root", { id, label, path, kind }),
    relinkRoot: (id, path) => view("manager_relink_root", { id, path }),
    forgetRoot: (id) => view("manager_forget_root", { id }),
    importBuild: async (req, rootId) => expectBuilt(await invoke<unknown>("manager_import_build", { ...req, rootId })),
    registerInPlace: async (req) => expectBuilt(await invoke<unknown>("manager_register_in_place", { ...req })),
    verify: async (buildId) => expectVerified(await invoke<unknown>("manager_verify_build", { buildId })),
    activate: (buildId) => view("manager_activate_build", { buildId }),
    rollback: (gameId, slot) => view("manager_rollback_slot", { gameId, slot }),
  };
}

// ---------------------------------------------------------------------------
// Words and small rules the page shows (the core enforces the real ones)
// ---------------------------------------------------------------------------

export const ROOT_KIND_LABEL: Record<RootKind, string> = { builds: "Game builds", mods: "Mods" };

export const BUILD_STATE_LABEL: Record<BuildState, string> = {
  staging: "Being copied",
  unverified: "Not checked",
  ready: "Matches its manifest",
  failed: "Does not match its manifest",
};

export const BUILTIN_VARIANTS = ["original", "optimized", "legacy", "experimental"] as const;

export function variantLabel(v: Variant): string {
  if (typeof v === "string") return v.charAt(0).toUpperCase() + v.slice(1);
  return `Custom: ${v.custom}`;
}

/** A name with the letters, digits and dashes a folder id may hold. The core still checks the result. */
export function slugId(name: string): string {
  return name
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 32)
    .replace(/-+$/g, "");
}

export function formatBytes(n: number | null): string {
  if (n === null) return "unknown";
  const units = ["B", "KB", "MB", "GB", "TB", "PB"];
  let value = n;
  let i = 0;
  // 999.5 and over would print as "1000" in the unit below, so it counts as the next unit up.
  while (value >= 999.5 && i < units.length - 1) {
    value /= 1000;
    i += 1;
  }
  return `${i === 0 ? value : value.toFixed(value < 10 ? 1 : 0)} ${units[i]}`;
}

/** What is wrong with a folder right now, or null when it is fine. */
export function rootProblem(r: RootView): string | null {
  if (!r.present) return "The folder is not there. If a drive was unplugged, plug it in; if the folder moved, point SUL at its new place.";
  if (!r.marker_ok) return "A different folder or drive is here: it does not carry this folder's marker file, so SUL will not use it.";
  return null;
}

export function describeLocation(loc: Location, roots: RootView[]): string {
  if (loc.kind === "external_install") return "The original install, where it is (read only)";
  const root = roots.find((r) => r.id === loc.root_id);
  return `A copy in ${root ? root.label : loc.root_id}`;
}

export function describeCheck(check: BuildCheck): string {
  const { missing, modified, extra } = check.report;
  if (missing.length === 0 && modified.length === 0) {
    return extra.length === 0 ? "Matches its manifest." : `Matches its manifest (${extra.length} extra file${extra.length === 1 ? "" : "s"} not listed).`;
  }
  return `Does not match its manifest: ${missing.length} missing, ${modified.length} changed.`;
}

/** The builds of one game, original slot first. */
export function buildsOf(view: ManagerView, gameId: string): BuildView[] {
  const order = (b: BuildView) => {
    const i = (BUILTIN_VARIANTS as readonly string[]).indexOf(b.slot);
    return i === -1 ? BUILTIN_VARIANTS.length : i;
  };
  return view.builds.filter((b) => b.game_id === gameId).sort((a, b) => order(a) - order(b) || a.slot.localeCompare(b.slot) || a.id.localeCompare(b.id));
}

/** Whether a slot has an earlier active build to go back to. */
export function canRollBack(view: ManagerView, gameId: string, slot: string): boolean {
  return view.slots.some((s) => s.game_id === gameId && s.slot === slot && s.history.length > 0);
}

export function gamesWithBuilds(view: ManagerView, games: GameView[]): { gameId: string; name: string }[] {
  const names = new Map(games.map((g) => [g.id, g.name]));
  const ids = [...new Set(view.builds.map((b) => b.game_id))];
  return ids.map((gameId) => ({ gameId, name: names.get(gameId) ?? gameId })).sort((a, b) => a.name.localeCompare(b.name));
}

// ---------------------------------------------------------------------------
// Controller: all state and logic behind the folders-and-builds panel, without React or the DOM.
// ---------------------------------------------------------------------------

export interface ManagerState {
  loaded: boolean;
  view: ManagerView | null;
  /** What is running now. The backend does one thing at a time, so only one thing is started at a time. */
  busy: string | null;
  error: string | null;
  notice: string | null;
  /** The last check of each build, by build id. */
  checks: Record<string, BuildCheck>;
}

export const EMPTY_MANAGER: ManagerState = { loaded: false, view: null, busy: null, error: null, notice: null, checks: {} };

export class ManagerController {
  private state: ManagerState = EMPTY_MANAGER;
  private readonly listeners = new Set<() => void>();

  /** `api` is null when there is no backend (a plain browser). */
  constructor(private readonly api: ManagerApi | null) {}

  get hasBackend(): boolean {
    return this.api !== null;
  }

  subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  };

  getSnapshot = (): ManagerState => this.state;

  private set(patch: Partial<ManagerState>): void {
    this.state = { ...this.state, ...patch };
    for (const listener of this.listeners) listener();
  }

  /** Read the saved state. */
  async load(): Promise<void> {
    if (!this.api) {
      this.set({ loaded: true });
      return;
    }
    try {
      this.set({ view: await this.api.view(), loaded: true, error: null });
    } catch (error) {
      this.set({ loaded: true, error: messageOf(error) });
    }
  }

  /**
   * Run one backend operation. Only one runs at a time (a second request while one runs is ignored, as the
   * backend would make it wait anyway). A failure leaves the data as it was and says why.
   */
  private async run<T>(what: string, op: (api: ManagerApi) => Promise<T>, done: (result: T) => Partial<ManagerState>): Promise<boolean> {
    if (!this.api || this.state.busy !== null) return false;
    this.set({ busy: what, error: null, notice: null });
    try {
      const result = await op(this.api);
      this.set({ ...done(result), busy: null });
      return true;
    } catch (error) {
      this.set({ busy: null, error: messageOf(error) });
      return false;
    }
  }

  addRoot(name: string, path: string, kind: RootKind): Promise<boolean> {
    const id = slugId(name);
    const label = name.trim();
    if (id === "" || label === "") {
      this.set({ error: "Give the folder a name that has some letters or digits in it.", notice: null });
      return Promise.resolve(false);
    }
    if (path.trim() === "") {
      this.set({ error: "Type the path of the folder.", notice: null });
      return Promise.resolve(false);
    }
    return this.run("add-root", (api) => api.addRoot(id, label, path.trim(), kind), (view) => ({ view, notice: `Added the folder “${label}”.` }));
  }

  relinkRoot(id: string, path: string): Promise<boolean> {
    if (path.trim() === "") {
      this.set({ error: "Type the new path of the folder.", notice: null });
      return Promise.resolve(false);
    }
    return this.run("relink-root", (api) => api.relinkRoot(id, path.trim()), (view) => ({ view, notice: "Pointed the folder at its new place." }));
  }

  forgetRoot(root: RootView): Promise<boolean> {
    return this.run("forget-root", (api) => api.forgetRoot(root.id), (view) => ({ view, notice: `SUL no longer lists “${root.label}”. The folder and its files were not touched.` }));
  }

  importBuild(game: GameView, req: Omit<BuildRequest, "gameId">, root: RootView): Promise<boolean> {
    return this.run(
      "import",
      (api) => api.importBuild({ ...req, gameId: game.id }, root.id),
      (r) => ({ view: r.view, notice: `Copied ${game.name} into “${root.label}” and checked the copy. Your original install was only read.` }),
    );
  }

  registerInPlace(game: GameView, req: Omit<BuildRequest, "gameId">): Promise<boolean> {
    return this.run(
      "register",
      (api) => api.registerInPlace({ ...req, gameId: game.id }),
      (r) => ({ view: r.view, notice: `Recorded ${game.name} where it is, as a read-only baseline. Nothing was copied or written.` }),
    );
  }

  verify(buildId: string): Promise<boolean> {
    return this.run(
      `verify:${buildId}`,
      (api) => api.verify(buildId),
      (r) => ({ view: r.view, checks: { ...this.state.checks, [buildId]: r.check }, notice: describeCheck(r.check) }),
    );
  }

  activate(buildId: string): Promise<boolean> {
    return this.run(`activate:${buildId}`, (api) => api.activate(buildId), (view) => ({ view, notice: "That build is now the active one of its slot." }));
  }

  rollback(gameId: string, slot: string): Promise<boolean> {
    return this.run(`rollback:${gameId}:${slot}`, (api) => api.rollback(gameId, slot), (view) => ({ view, notice: `Went back to the previous build of the ${slot} slot.` }));
  }
}
