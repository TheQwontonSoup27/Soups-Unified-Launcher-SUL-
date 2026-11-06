import { invoke, isTauri } from "@tauri-apps/api/core";
import { messageOf } from "./library";
import { slugId, type RootView } from "./manager";

// ---------------------------------------------------------------------------
// Wire types. They mirror `sul_core::manager` (the `modding` part) and `sul_core::mods` (serde, snake_case).
// `src-tauri/src/ipc_tests.rs` pins the field names from the Rust side.
// ---------------------------------------------------------------------------

export interface Requirement {
  mod_id: string;
  /** The oldest version of that mod that will do, when the author said so. */
  min_version: string | null;
}

export interface ModView {
  game_id: string;
  mod_id: string;
  name: string;
  version: string;
  requires: Requirement[];
  /** Game versions the mod is said to work with. Empty means nobody said; it is never read as "any". */
  compatible_versions: string[];
  file_count: number;
  total_bytes: number;
  source: string;
  license_note: string;
  imported_unix: number;
  root_id: string;
}

export interface ProfileMod {
  mod_id: string;
  version: string;
  enabled: boolean;
}

export type Severity = "error" | "warning";

export interface Problem {
  severity: Severity;
  kind: string;
  mod_id: string | null;
  path: string | null;
  message: string;
}

export interface ProfileView {
  id: string;
  name: string;
  game_id: string;
  build_id: string;
  /** In load order, first loaded first. A later mod wins when two provide the same file. */
  mods: ProfileMod[];
  config: Record<string, string>;
  notes: string;
  created_unix: number;
  updated_unix: number;
  problems: Problem[];
}

export type DeployStatus = "in_progress" | "deployed";

export interface DeploymentView {
  id: string;
  profile_id: string;
  profile_name: string;
  status: DeployStatus;
  started_unix: number;
  finished_unix: number | null;
  files: number;
}

export interface PurgeSummary {
  id: string;
  profile_name: string;
  unix: number;
  files: number;
}

export interface HistoryItem {
  id: string;
  kind: "deploy" | "purge" | "undo_purge";
  profile_id: string;
  unix: number;
  files: number;
  note: string;
}

export interface BuildModsView {
  build_id: string;
  game_id: string;
  /** The build's slot (original, optimized, legacy, experimental or a name). */
  slot: string;
  deployment: DeploymentView | null;
  undoable_purge: PurgeSummary | null;
  history: HistoryItem[];
  /** Why the build's mod state could not be read (an unplugged drive, a damaged file). The rest is then empty. */
  error: string | null;
}

export interface ModsView {
  mods: ModView[];
  profiles: ProfileView[];
  builds: BuildModsView[];
  mod_roots: RootView[];
  /** Games with an adapter, which are the only games mods are deployed for. */
  deployable_games: string[];
}

export interface VerifyReport {
  missing: string[];
  modified: string[];
  extra: string[];
}

export interface ModVerified {
  report: VerifyReport;
  view: ModsView;
}

export interface ReplacedFile {
  sha256: string;
  size: number;
  in_build_manifest: boolean;
}

export interface PlannedFile {
  path: string;
  mod_id: string;
  mod_version: string;
  action: "add" | "replace";
  sha256: string;
  size: number;
  replaces: ReplacedFile | null;
  /** Other enabled mods that provide the same file and lose to this one. */
  overrides: string[];
}

export interface DeployPlan {
  profile_id: string;
  build_id: string;
  files: PlannedFile[];
  problems: Problem[];
  total_bytes: number;
}

export type PurgeAction =
  | "remove_mod_file"
  | "remove_mod_file_and_restore_base"
  | "restore_base_only"
  | "forget"
  | "leave_changed_since_deploy"
  | "leave_base_copy_problem";

export interface PurgeItem {
  path: string;
  mod_id: string;
  action: PurgeAction;
  detail: string;
}

export interface PurgePlan {
  build_id: string;
  deployment: { id: string; profile_id: string; profile_name: string; started_unix: number } | null;
  items: PurgeItem[];
}

export interface DeployReport {
  deployment_id: string;
  added: string[];
  replaced: string[];
  kept: [string, string][];
  warnings: Problem[];
}

export interface PurgeReport {
  purge_id: string | null;
  removed: string[];
  restored: string[];
  forgotten: string[];
  /** Files left in place, with the reason. Nonempty means the build is still partly modded. */
  left: PurgeItem[];
  recovery_dir: string | null;
}

export interface UndoReport {
  put_back: string[];
  originals_moved_aside_again: string[];
}

export interface PurgeAndDeployReport {
  purge: PurgeReport;
  /** Null when the purge left files in place and the deploy was not attempted. */
  deploy: DeployReport | null;
}

export interface Deployed {
  report: DeployReport;
  view: ModsView;
}
export interface Purged {
  report: PurgeReport;
  view: ModsView;
}
export interface Undone {
  report: UndoReport;
  view: ModsView;
}
export interface PurgedAndDeployed {
  report: PurgeAndDeployReport;
  view: ModsView;
}

/** A mod folder to import. The folder is only read. */
export interface ModSource {
  game_id: string;
  mod_id: string;
  name: string;
  version: string;
  requires: Requirement[];
  compatible_versions: string[];
  source: string;
  license_note: string;
  path: string;
}

export type ProfileEdit =
  | { action: "rename"; name: string }
  | { action: "notes"; notes: string }
  | { action: "set_build"; build_id: string }
  | { action: "add_mod"; mod_id: string; version: string; position: number | null }
  | { action: "remove_mod"; mod_id: string }
  | { action: "set_enabled"; mod_id: string; enabled: boolean }
  | { action: "set_version"; mod_id: string; version: string }
  | { action: "move"; mod_id: string; to_index: number }
  | { action: "set_config"; key: string; value: string | null };

// ---------------------------------------------------------------------------
// Backend
// ---------------------------------------------------------------------------

/** The commands of `src-tauri/src/commands.rs` that belong to mods. */
export interface ModsApi {
  view(): Promise<ModsView>;
  importMod(source: ModSource, rootId: string): Promise<ModsView>;
  verifyMod(gameId: string, modId: string, version: string): Promise<ModVerified>;
  createProfile(id: string, name: string, gameId: string, buildId: string): Promise<ModsView>;
  editProfile(profileId: string, edit: ProfileEdit): Promise<ModsView>;
  planDeploy(profileId: string): Promise<DeployPlan>;
  deploy(profileId: string): Promise<Deployed>;
  planPurge(buildId: string): Promise<PurgePlan>;
  purge(buildId: string): Promise<Purged>;
  undoPurge(buildId: string, purgeId: string): Promise<Undone>;
  purgeAndDeploy(profileId: string): Promise<PurgedAndDeployed>;
}

const UNEXPECTED = "The app sent mod data in a shape this page does not understand, so it was not used.";

const isObject = (v: unknown): v is Record<string, unknown> => typeof v === "object" && v !== null;
const arrays = (o: Record<string, unknown>, keys: string[]) => keys.every((k) => Array.isArray(o[k]));

/** Whether the backend sent a view of the shape the page reads. A wrong shape would otherwise crash the page. */
export function isModsView(v: unknown): v is ModsView {
  return isObject(v) && arrays(v, ["mods", "profiles", "builds", "mod_roots", "deployable_games"]);
}

function expectView(v: unknown): ModsView {
  if (!isModsView(v)) throw new Error(UNEXPECTED);
  return v;
}

/** An answer that holds `report` (an object with the given list fields) and a `view`. */
function expectWith<T>(v: unknown, reportLists: string[]): T {
  const report = isObject(v) ? v.report : undefined;
  if (!isObject(v) || !isObject(report) || !arrays(report, reportLists) || !isModsView(v.view)) throw new Error(UNEXPECTED);
  return v as T;
}

function expectPurgeAndDeploy(v: unknown): PurgedAndDeployed {
  const report = isObject(v) ? v.report : undefined;
  const purge = isObject(report) ? report.purge : undefined;
  if (!isObject(v) || !isObject(report) || !isObject(purge) || !arrays(purge, ["removed", "restored", "forgotten", "left"]) || !isModsView(v.view)) throw new Error(UNEXPECTED);
  return v as unknown as PurgedAndDeployed;
}

function expectDeployPlan(v: unknown): DeployPlan {
  if (!isObject(v) || !arrays(v, ["files", "problems"])) throw new Error(UNEXPECTED);
  return v as unknown as DeployPlan;
}

function expectPurgePlan(v: unknown): PurgePlan {
  if (!isObject(v) || !arrays(v, ["items"])) throw new Error(UNEXPECTED);
  return v as unknown as PurgePlan;
}

/** The real backend, or null in a plain browser where there is none. */
export function tauriModsApi(): ModsApi | null {
  if (!isTauri()) return null;
  const view = async (cmd: string, args?: Record<string, unknown>) => expectView(await invoke<unknown>(cmd, args));
  return {
    view: () => view("mods_view"),
    importMod: (source, rootId) => view("mods_import", { source, rootId }),
    verifyMod: async (gameId, modId, version) => {
      const v = await invoke<unknown>("mods_verify", { gameId, modId, version });
      const report = isObject(v) ? v.report : undefined;
      if (!isObject(v) || !isObject(report) || !arrays(report, ["missing", "modified", "extra"]) || !isModsView(v.view)) throw new Error(UNEXPECTED);
      return v as unknown as ModVerified;
    },
    createProfile: (id, name, gameId, buildId) => view("mods_create_profile", { id, name, gameId, buildId }),
    editProfile: (profileId, edit) => view("mods_edit_profile", { profileId, edit }),
    planDeploy: async (profileId) => expectDeployPlan(await invoke<unknown>("mods_plan_deploy", { profileId })),
    deploy: async (profileId) => expectWith<Deployed>(await invoke<unknown>("mods_deploy", { profileId }), ["added", "replaced", "kept", "warnings"]),
    planPurge: async (buildId) => expectPurgePlan(await invoke<unknown>("mods_plan_purge", { buildId })),
    purge: async (buildId) => expectWith<Purged>(await invoke<unknown>("mods_purge", { buildId }), ["removed", "restored", "forgotten", "left"]),
    undoPurge: async (buildId, purgeId) => expectWith<Undone>(await invoke<unknown>("mods_undo_purge", { buildId, purgeId }), ["put_back", "originals_moved_aside_again"]),
    purgeAndDeploy: async (profileId) => expectPurgeAndDeploy(await invoke<unknown>("mods_purge_and_deploy", { profileId })),
  };
}

// ---------------------------------------------------------------------------
// Words and small rules the page shows (the core enforces the real ones)
// ---------------------------------------------------------------------------

export const PURGE_ACTION_LABEL: Record<PurgeAction, string> = {
  remove_mod_file: "Move the mod's file to the recovery folder",
  remove_mod_file_and_restore_base: "Move the mod's file to the recovery folder and put the original file back",
  restore_base_only: "Put the original file back (the mod's file is already gone)",
  forget: "Nothing to do; drop it from the record",
  leave_changed_since_deploy: "Leave it: the file was changed after it was deployed",
  leave_base_copy_problem: "Leave it: the kept copy of the original file is missing or has changed",
};

/** Whether a purge action leaves the file where it is (the build then stays partly modded). */
export function leavesFile(a: PurgeAction): boolean {
  return a === "leave_changed_since_deploy" || a === "leave_base_copy_problem";
}

export function formatSize(n: number): string {
  const units = ["B", "KB", "MB", "GB", "TB"];
  let value = n;
  let i = 0;
  while (value >= 999.5 && i < units.length - 1) {
    value /= 1000;
    i += 1;
  }
  return `${i === 0 ? value : value.toFixed(value < 10 ? 1 : 0)} ${units[i]}`;
}

/** The errors among a list of problems: they block a deploy. */
export function errorsOf(problems: Problem[]): Problem[] {
  return problems.filter((p) => p.severity === "error");
}

export function warningsOf(problems: Problem[]): Problem[] {
  return problems.filter((p) => p.severity === "warning");
}

/** Whether the mod can be deployed for this game at all (it has an adapter). */
export function canDeployGame(view: ModsView, gameId: string): boolean {
  return view.deployable_games.includes(gameId);
}

export function modKey(m: { game_id: string; mod_id: string; version: string }): string {
  return `${m.game_id}/${m.mod_id}@${m.version}`;
}

/** The mod ids a person typed, comma separated, each optionally with `>=version`. Null names the first bad piece. */
export function parseRequires(text: string): { requires: Requirement[] } | { bad: string } {
  const out: Requirement[] = [];
  for (const raw of text.split(",")) {
    const piece = raw.trim();
    if (piece === "") continue;
    const [id, min, ...rest] = piece.split(">=").map((s) => s.trim());
    if (!id || rest.length > 0 || min === "") return { bad: piece };
    if (!/^[A-Za-z0-9][A-Za-z0-9._-]*$/.test(id)) return { bad: piece };
    if (min !== undefined && !/^[A-Za-z0-9][A-Za-z0-9._-]*$/.test(min)) return { bad: piece };
    out.push({ mod_id: id, min_version: min ?? null });
  }
  return { requires: out };
}

export function parseVersions(text: string): string[] {
  return text
    .split(",")
    .map((s) => s.trim())
    .filter((s) => s !== "");
}

/** The versions of a mod in the store, newest import first, for a profile's version picker. */
export function versionsOf(view: ModsView, gameId: string, modId: string): string[] {
  return view.mods
    .filter((m) => m.game_id === gameId && m.mod_id === modId)
    .sort((a, b) => b.imported_unix - a.imported_unix)
    .map((m) => m.version);
}

/** The mods of one game, by name. */
export function modsOfGame(view: ModsView, gameId: string): ModView[] {
  return view.mods.filter((m) => m.game_id === gameId).sort((a, b) => a.name.localeCompare(b.name) || a.version.localeCompare(b.version));
}

/** The most recently imported version of each mod id of a game (not the highest version number), for the "add to profile" picker. */
export function latestOfEachMod(view: ModsView, gameId: string): ModView[] {
  const best = new Map<string, ModView>();
  for (const m of view.mods.filter((x) => x.game_id === gameId)) {
    const have = best.get(m.mod_id);
    if (!have || m.imported_unix >= have.imported_unix) best.set(m.mod_id, m);
  }
  return [...best.values()].sort((a, b) => a.name.localeCompare(b.name));
}

export function buildModsOf(view: ModsView, buildId: string): BuildModsView | undefined {
  return view.builds.find((b) => b.build_id === buildId);
}

// ---------------------------------------------------------------------------
// Controller: all state and logic behind the Mods page, without React or the DOM.
// ---------------------------------------------------------------------------

/** A preview the person is looking at. Nothing is changed until they confirm it. */
export type Preview =
  | { kind: "deploy"; profileId: string; plan: DeployPlan }
  | { kind: "purge"; buildId: string; plan: PurgePlan }
  | { kind: "purge_and_deploy"; profileId: string; buildId: string; plan: PurgePlan }
  | { kind: "undo"; buildId: string; summary: PurgeSummary };

export interface ModsState {
  loaded: boolean;
  view: ModsView | null;
  /** What is running now. The backend does one thing at a time, so only one thing is started at a time. */
  busy: string | null;
  error: string | null;
  notice: string | null;
  /** The last check of each stored mod, by `modKey`. */
  checks: Record<string, VerifyReport>;
  preview: Preview | null;
}

export const EMPTY_MODS: ModsState = { loaded: false, view: null, busy: null, error: null, notice: null, checks: {}, preview: null };

function count(n: number, one: string, many = `${one}s`): string {
  return `${n} ${n === 1 ? one : many}`;
}

export function describeVerify(r: VerifyReport): string {
  if (r.missing.length === 0 && r.modified.length === 0) {
    return r.extra.length === 0 ? "Matches what was imported." : `Matches what was imported (${count(r.extra.length, "extra file")} not listed).`;
  }
  return `Does not match what was imported: ${r.missing.length} missing, ${r.modified.length} changed.`;
}

export function describePurge(r: PurgeReport): string {
  const parts = [`${count(r.removed.length, "file")} moved to the recovery folder`, `${r.restored.length} original ${r.restored.length === 1 ? "file" : "files"} put back`];
  const text = `Purged: ${parts.join(", ")}.`;
  if (r.left.length === 0) return text;
  return `${text} ${count(r.left.length, "file")} left in place, so the build is still partly modded: ${r.left.map((i) => `${i.path} (${i.detail})`).join("; ")}.`;
}

export function describeDeploy(r: DeployReport): string {
  return `Deployed ${count(r.added.length + r.replaced.length, "file")}: ${r.added.length} added, ${r.replaced.length} replaced (the replaced originals are kept).`;
}

export class ModsController {
  private state: ModsState = EMPTY_MODS;
  private readonly listeners = new Set<() => void>();

  /** `api` is null when there is no backend (a plain browser). */
  constructor(private readonly api: ModsApi | null) {}

  get hasBackend(): boolean {
    return this.api !== null;
  }

  subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  };

  getSnapshot = (): ModsState => this.state;

  private set(patch: Partial<ModsState>): void {
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
  private async run<T>(what: string, op: (api: ModsApi) => Promise<T>, done: (result: T) => Partial<ModsState>): Promise<boolean> {
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

  /** Show a refusal without running anything. */
  private refuse(message: string): Promise<boolean> {
    this.set({ error: message, notice: null });
    return Promise.resolve(false);
  }

  importMod(source: ModSource, rootId: string): Promise<boolean> {
    if (source.mod_id === "" || source.name.trim() === "") return this.refuse("Give the mod a name that has some letters or digits in it.");
    if (source.version.trim() === "") return this.refuse("Type the mod's version (for example 1.0).");
    if (source.path.trim() === "") return this.refuse("Type the path of the folder that holds the mod's files.");
    if (rootId === "") return this.refuse("Add a library folder for mods first (on the Game Library page).");
    return this.run("import", (api) => api.importMod({ ...source, path: source.path.trim(), version: source.version.trim() }, rootId), (view) => ({
      view,
      notice: `Copied “${source.name.trim()}” into the mod library and checked the copy. The folder it came from was only read.`,
    }));
  }

  verifyMod(m: ModView): Promise<boolean> {
    return this.run(
      `verify:${modKey(m)}`,
      (api) => api.verifyMod(m.game_id, m.mod_id, m.version),
      (r) => ({ view: r.view, checks: { ...this.state.checks, [modKey(m)]: r.report }, notice: describeVerify(r.report) }),
    );
  }

  createProfile(name: string, gameId: string, buildId: string): Promise<boolean> {
    const id = slugId(name);
    if (id === "" || name.trim() === "") return this.refuse("Give the profile a name that has some letters or digits in it.");
    if (gameId === "" || buildId === "") return this.refuse("Pick a game and one of its builds for the profile.");
    return this.run("create-profile", (api) => api.createProfile(id, name.trim(), gameId, buildId), (view) => ({ view, notice: `Made the profile “${name.trim()}”. Nothing was deployed.` }));
  }

  editProfile(profileId: string, edit: ProfileEdit): Promise<boolean> {
    return this.run(`edit:${profileId}`, (api) => api.editProfile(profileId, edit), (view) => ({
      view,
      notice: view.builds.some((b) => b.deployment?.profile_id === profileId) ? "Saved. The build keeps what was deployed; the change applies at the next deploy." : "Saved.",
    }));
  }

  // ---- previews (read only) and the operations they lead to ----

  closePreview(): void {
    this.set({ preview: null });
  }

  /** Show what a deploy would do. Changes nothing. */
  async previewDeploy(profileId: string): Promise<boolean> {
    return this.run(`plan-deploy:${profileId}`, (api) => api.planDeploy(profileId), (plan) => ({ preview: { kind: "deploy", profileId, plan } }));
  }

  /** Show what a purge would do. Changes nothing. */
  async previewPurge(buildId: string): Promise<boolean> {
    return this.run(`plan-purge:${buildId}`, (api) => api.planPurge(buildId), (plan) => ({ preview: { kind: "purge", buildId, plan } }));
  }

  /** Show what the purge half of Purge & Deploy would do. The deploy half is planned again after the purge. */
  async previewPurgeAndDeploy(profileId: string): Promise<boolean> {
    const profile = this.state.view?.profiles.find((p) => p.id === profileId);
    if (!profile) return this.refuse("That profile is not there any more.");
    return this.run(`plan-purge-deploy:${profileId}`, (api) => api.planPurge(profile.build_id), (plan) => ({
      preview: { kind: "purge_and_deploy", profileId, buildId: profile.build_id, plan },
    }));
  }

  /** Ask about undoing the latest purge of a build. Nothing runs until `confirm`. */
  previewUndo(buildId: string): boolean {
    const summary = this.state.view?.builds.find((b) => b.build_id === buildId)?.undoable_purge;
    if (!summary) {
      this.set({ error: "There is no purge of this build that can still be undone.", notice: null });
      return false;
    }
    this.set({ preview: { kind: "undo", buildId, summary }, error: null, notice: null });
    return true;
  }

  /** Do what the open preview showed. The core plans again, so a stale preview is never trusted. */
  async confirm(): Promise<boolean> {
    const p = this.state.preview;
    if (!p) return false;
    // The question closes first, so it never stays up over a long operation.
    this.set({ preview: null });
    // A blocked plan is never run, even if a page offers to. The core refuses it too.
    if (p.kind === "deploy" && errorsOf(p.plan.problems).length > 0) return this.refuse("That deploy is blocked, so nothing was done. The preview lists what blocks it.");
    switch (p.kind) {
      case "deploy":
        return this.run("deploy", (api) => api.deploy(p.profileId), (r) => ({ view: r.view, notice: describeDeploy(r.report) }));
      case "purge":
        return this.run("purge", (api) => api.purge(p.buildId), (r) => ({ view: r.view, notice: describePurge(r.report) }));
      case "purge_and_deploy":
        return this.run("purge-and-deploy", (api) => api.purgeAndDeploy(p.profileId), (r) => ({
          view: r.view,
          notice: r.report.deploy ? `${describePurge(r.report.purge)} ${describeDeploy(r.report.deploy)}` : `${describePurge(r.report.purge)} The deploy was not attempted.`,
        }));
      case "undo":
        return this.run("undo", (api) => api.undoPurge(p.buildId, p.summary.id), (r) => ({
          view: r.view,
          notice: `Undid the purge: ${count(r.report.put_back.length, "file")} put back.`,
        }));
    }
  }
}
