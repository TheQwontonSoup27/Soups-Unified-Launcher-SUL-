import { invoke, isTauri } from "@tauri-apps/api/core";
import { messageOf } from "./library";
import { slugId, type RootView } from "./manager";
import { formatSize, type ModView, type VerifyReport } from "./mods";

// ---------------------------------------------------------------------------
// Wire types. They mirror `sul_core::builder` (recipe, patch, book) and `sul_core::manager` (the `building` part),
// serde, snake_case. `src-tauri/src/ipc_tests.rs` pins the field names from the Rust side.
// ---------------------------------------------------------------------------

export interface Hunk {
  offset: number;
  old_hex: string;
  new_hex: string;
}

export interface BytePatch {
  id: string;
  description: string;
  /** SHA-256 (lower-case hex) of the only file this patch applies to. */
  expect_sha256: string;
  hunks: Hunk[];
}

export type Step =
  | { op: "layer_mod"; mod_id: string; version: string; into: string }
  | { op: "patch_file"; path: string; patch: BytePatch }
  | { op: "runtime_code"; description: string };

export interface Recipe {
  schema: number;
  id: string;
  name: string;
  game_id: string;
  description: string;
  steps: Step[];
}

export interface RunRef {
  root_id: string;
  run_id: string;
}

export interface RecipeView {
  recipe: Recipe;
  /** The recipe's identity. A change to a saved recipe names it, so a stale window cannot overwrite a newer edit. */
  sha256: string;
  has_runtime_code: boolean;
  runs: RunRef[];
}

export interface BuilderView {
  recipes: RecipeView[];
  mods: ModView[];
  mod_roots: RootView[];
  /** Games with an adapter: the only games a recipe can be previewed or run for. */
  deployable_games: string[];
  run_errors: string[];
}

export interface RunInput {
  step: number;
  mod_id: string;
  version: string;
  source: string;
  /** What the owner wrote down when importing the mod. SUL does not check it. */
  license_note: string;
  inventory_sha256: string;
}

export interface PatchOutcome {
  patch_id: string;
  before_sha256: string;
  after_sha256: string;
  hunks: number;
  bytes_changed: number;
}

export interface AppliedPatch {
  step: number;
  path: string;
  outcome: PatchOutcome;
}

/** A file a run would write or wrote (the core calls it `PlannedFile`). */
export interface RecipeFile {
  path: string;
  sha256: string;
  size: number;
  origin: string;
  replaced: string[];
}

export interface RunPlan {
  recipe_sha256: string;
  files: RecipeFile[];
  inputs: RunInput[];
  patches: AppliedPatch[];
  total_bytes: number;
  output_sha256: string;
  warnings: string[];
  problems: string[];
}

export interface RunRecord {
  schema: number;
  run_id: string;
  started_unix: number;
  recipe: Recipe;
  recipe_sha256: string;
  inputs: RunInput[];
  patches: AppliedPatch[];
  files: RecipeFile[];
  output_sha256: string;
  warnings: string[];
}

export interface RecipeRan {
  record: RunRecord;
  view: BuilderView;
}

export interface ModFile {
  path: string;
  sha256: string;
  size: number;
}

export interface ModFileList {
  files: ModFile[];
  matching: number;
  total: number;
}

export interface BuilderApi {
  view(): Promise<BuilderView>;
  saveRecipe(recipe: Recipe, replaces: string | null): Promise<BuilderView>;
  planRecipe(recipeId: string): Promise<RunPlan>;
  runRecipe(recipeId: string, rootId: string): Promise<RecipeRan>;
  readRun(recipeId: string, rootId: string, runId: string): Promise<RunRecord>;
  verifyRun(recipeId: string, rootId: string, runId: string): Promise<VerifyReport>;
  modFiles(gameId: string, modId: string, version: string, filter: string): Promise<ModFileList>;
}

const UNEXPECTED = "The app sent builder data in a shape this page does not understand, so it was not used.";

const isObject = (v: unknown): v is Record<string, unknown> => typeof v === "object" && v !== null;
const arrays = (o: Record<string, unknown>, keys: string[]) => keys.every((k) => Array.isArray(o[k]));

/** Whether the backend sent a view of the shape the page reads. A wrong shape would otherwise crash the page. */
export function isBuilderView(v: unknown): v is BuilderView {
  return isObject(v) && arrays(v, ["recipes", "mods", "mod_roots", "deployable_games", "run_errors"]);
}

const isRecord = (v: unknown): v is RunRecord => isObject(v) && typeof v.run_id === "string" && isObject(v.recipe) && arrays(v, ["inputs", "patches", "files", "warnings"]);

function expectView(v: unknown): BuilderView {
  if (!isBuilderView(v)) throw new Error(UNEXPECTED);
  return v;
}

function expectPlan(v: unknown): RunPlan {
  if (!isObject(v) || !arrays(v, ["files", "inputs", "patches", "warnings", "problems"])) throw new Error(UNEXPECTED);
  return v as unknown as RunPlan;
}

function expectRecord(v: unknown): RunRecord {
  if (!isRecord(v)) throw new Error(UNEXPECTED);
  return v;
}

function expectRan(v: unknown): RecipeRan {
  if (!isObject(v) || !isRecord(v.record) || !isBuilderView(v.view)) throw new Error(UNEXPECTED);
  return v as unknown as RecipeRan;
}

function expectReport(v: unknown): VerifyReport {
  if (!isObject(v) || !arrays(v, ["missing", "modified", "extra"])) throw new Error(UNEXPECTED);
  return v as unknown as VerifyReport;
}

function expectFiles(v: unknown): ModFileList {
  if (!isObject(v) || !arrays(v, ["files"]) || typeof v.matching !== "number" || typeof v.total !== "number") throw new Error(UNEXPECTED);
  return v as unknown as ModFileList;
}

/** The real backend, or null in a plain browser where there is none. */
export function tauriBuilderApi(): BuilderApi | null {
  if (!isTauri()) return null;
  return {
    view: async () => expectView(await invoke<unknown>("builder_view")),
    saveRecipe: async (recipe, replaces) => expectView(await invoke<unknown>("builder_save_recipe", { recipe, replaces })),
    planRecipe: async (recipeId) => expectPlan(await invoke<unknown>("builder_plan_recipe", { recipeId })),
    runRecipe: async (recipeId, rootId) => expectRan(await invoke<unknown>("builder_run_recipe", { recipeId, rootId })),
    readRun: async (recipeId, rootId, runId) => expectRecord(await invoke<unknown>("builder_read_run", { recipeId, rootId, runId })),
    verifyRun: async (recipeId, rootId, runId) => expectReport(await invoke<unknown>("builder_verify_run", { recipeId, rootId, runId })),
    modFiles: async (gameId, modId, version, filter) => expectFiles(await invoke<unknown>("builder_mod_files", { gameId, modId, version, filter })),
  };
}

// ---------------------------------------------------------------------------
// Words and small rules the page shows (the core enforces the real ones)
// ---------------------------------------------------------------------------

export const RECIPE_SCHEMA = 1;

function count(n: number, one: string, many = `${one}s`): string {
  return `${n} ${n === 1 ? one : many}`;
}

export function shortHash(sha: string): string {
  return sha.length > 12 ? `${sha.slice(0, 12)}…` : sha;
}

export const runKey = (recipeId: string, r: RunRef): string => `${recipeId}\u0000${r.root_id}\u0000${r.run_id}`;

/** Whether a preview or a run is possible for this recipe's game (it needs an adapter, which gives the folder rules). */
export function canPlanGame(view: BuilderView, gameId: string): boolean {
  return view.deployable_games.includes(gameId);
}

/** One line for a step, as the person wrote it. */
export function describeStep(step: Step): string {
  switch (step.op) {
    case "layer_mod":
      return `Layer mod ${step.mod_id} ${step.version} ${step.into === "" ? "at the top of the workspace" : `into ${step.into}`}`;
    case "patch_file":
      return `Patch ${step.path}: ${count(step.patch.hunks.length, "change")} (only for the file whose SHA-256 starts ${shortHash(step.patch.expect_sha256)})`;
    case "runtime_code":
      return `Runtime-code change (cannot be run by SUL): ${step.description}`;
  }
}

/**
 * Hunks typed one per line as `offset old-bytes new-bytes`: the offset in decimal or as 0x… hexadecimal, the bytes as
 * hexadecimal of the same length. The core checks the rest (order, overlap, same length, that the file has those bytes).
 */
export function parseHunks(text: string): { hunks: Hunk[] } | { bad: string } {
  const hunks: Hunk[] = [];
  const lines = text.split("\n");
  for (let i = 0; i < lines.length; i += 1) {
    const line = lines[i]!.trim();
    if (line === "") continue;
    const parts = line.split(/\s+/);
    if (parts.length !== 3) return { bad: `Line ${i + 1}: write the offset, the old bytes and the new bytes, separated by spaces.` };
    const [offset, oldHex, newHex] = parts as [string, string, string];
    if (!/^(0x[0-9a-f]+|[0-9]+)$/i.test(offset)) return { bad: `Line ${i + 1}: the offset “${offset}” is not a number (use digits, or 0x followed by hexadecimal).` };
    const n = Number(offset);
    if (!Number.isSafeInteger(n)) return { bad: `Line ${i + 1}: the offset is too large.` };
    for (const [what, hex] of [["old", oldHex], ["new", newHex]] as const) {
      if (!/^([0-9a-f]{2})+$/i.test(hex)) return { bad: `Line ${i + 1}: the ${what} bytes “${hex}” must be hexadecimal, two digits per byte.` };
    }
    hunks.push({ offset: n, old_hex: oldHex.toLowerCase(), new_hex: newHex.toLowerCase() });
  }
  if (hunks.length === 0) return { bad: "Write at least one change: an offset, the old bytes and the new bytes." };
  return { hunks };
}

/** The reverse of `parseHunks`, to edit a patch that is already in a recipe. */
export function hunksToText(hunks: Hunk[]): string {
  return hunks.map((h) => `${h.offset} ${h.old_hex} ${h.new_hex}`).join("\n");
}

export function describePlan(p: RunPlan): string {
  return `${count(p.files.length, "file")}, ${formatSize(p.total_bytes)}, from ${count(p.inputs.length, "mod")} and ${count(p.patches.length, "patch", "patches")}.`;
}

export function describeVerifyRun(r: VerifyReport): string {
  if (r.missing.length === 0 && r.modified.length === 0 && r.extra.length === 0) return "Matches its record.";
  return `Does not match its record: ${r.missing.length} missing, ${r.modified.length} changed, ${r.extra.length} not in the record.`;
}

// ---------------------------------------------------------------------------
// The recipe being written (a draft is text the person typed; `recipeOf` makes it a recipe or says what is wrong)
// ---------------------------------------------------------------------------

export type DraftStep =
  | { op: "layer_mod"; mod_id: string; version: string; into: string }
  | { op: "patch_file"; path: string; patch_name: string; description: string; expect_sha256: string; hunks: string }
  | { op: "runtime_code"; description: string };

export interface Draft {
  /** Set when an already saved recipe is being changed: its id (which stays) and the SHA-256 it had when opened. */
  editing: { id: string; sha256: string } | null;
  name: string;
  game_id: string;
  description: string;
  steps: DraftStep[];
}

export const EMPTY_DRAFT: Draft = { editing: null, name: "", game_id: "", description: "", steps: [] };

export function draftOf(rv: RecipeView): Draft {
  const r = rv.recipe;
  return {
    editing: { id: r.id, sha256: rv.sha256 },
    name: r.name,
    game_id: r.game_id,
    description: r.description,
    steps: r.steps.map((s): DraftStep => {
      if (s.op === "patch_file") {
        return { op: "patch_file", path: s.path, patch_name: s.patch.id, description: s.patch.description, expect_sha256: s.patch.expect_sha256, hunks: hunksToText(s.patch.hunks) };
      }
      return { ...s };
    }),
  };
}

const SHA256 = /^[0-9a-f]{64}$/;

/** The recipe a draft describes, or what to fix. The core checks it again (names, patches, order of hunks). */
export function recipeOf(d: Draft): { recipe: Recipe } | { bad: string } {
  const id = d.editing ? d.editing.id : slugId(d.name);
  if (d.name.trim() === "") return { bad: "Give the recipe a name." };
  if (id === "") return { bad: "Give the recipe a name that has some letters or digits in it." };
  if (d.game_id === "") return { bad: "Pick a game first. Scan for games on the Game Library page if the list is empty." };
  if (d.steps.length === 0) return { bad: "Add at least one step." };
  const steps: Step[] = [];
  for (let i = 0; i < d.steps.length; i += 1) {
    const s = d.steps[i]!;
    const at = `Step ${i + 1}`;
    if (s.op === "layer_mod") {
      if (s.mod_id === "" || s.version === "") return { bad: `${at}: pick the mod to layer.` };
      steps.push({ op: "layer_mod", mod_id: s.mod_id, version: s.version, into: s.into.trim() });
    } else if (s.op === "patch_file") {
      if (s.path.trim() === "") return { bad: `${at}: write the path of the file to patch, as it is inside the workspace.` };
      const id = slugId(s.patch_name);
      if (id === "") return { bad: `${at}: give the patch a name that has some letters or digits in it.` };
      if (!SHA256.test(s.expect_sha256.trim().toLowerCase())) return { bad: `${at}: the file's SHA-256 must be 64 hexadecimal digits. “Look up files” fills it in from a mod.` };
      const h = parseHunks(s.hunks);
      if ("bad" in h) return { bad: `${at}: ${h.bad}` };
      steps.push({ op: "patch_file", path: s.path.trim(), patch: { id, description: s.description.trim(), expect_sha256: s.expect_sha256.trim().toLowerCase(), hunks: h.hunks } });
    } else {
      if (s.description.trim() === "") return { bad: `${at}: say what the runtime-code change is.` };
      steps.push({ op: "runtime_code", description: s.description.trim() });
    }
  }
  return { recipe: { schema: RECIPE_SCHEMA, id, name: d.name.trim(), game_id: d.game_id, description: d.description.trim(), steps } };
}

// ---------------------------------------------------------------------------
// The controller
// ---------------------------------------------------------------------------

export interface RunPreview {
  recipeId: string;
  rootId: string;
  plan: RunPlan;
}

export interface OpenRun {
  recipeId: string;
  rootId: string;
  record: RunRecord;
}

export interface BuilderState {
  loaded: boolean;
  view: BuilderView | null;
  /** What is running, or null. */
  busy: string | null;
  error: string | null;
  notice: string | null;
  /** A preview of a run, waiting for the person to confirm or cancel. */
  preview: RunPreview | null;
  /** A finished run's record, being looked at. */
  openRun: OpenRun | null;
  /** The latest check of each run, by `runKey`. */
  checks: Record<string, VerifyReport>;
  /** The latest file lookup for writing a patch step. */
  files: ModFileList | null;
}

export const EMPTY_BUILDER: BuilderState = { loaded: false, view: null, busy: null, error: null, notice: null, preview: null, openRun: null, checks: {}, files: null };

export class BuilderController {
  private state: BuilderState = EMPTY_BUILDER;
  private readonly listeners = new Set<() => void>();

  /** `api` is null when there is no backend (a plain browser). */
  constructor(private readonly api: BuilderApi | null) {}

  get hasBackend(): boolean {
    return this.api !== null;
  }

  subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  };

  getSnapshot = (): BuilderState => this.state;

  private set(patch: Partial<BuilderState>): void {
    this.state = { ...this.state, ...patch };
    for (const listener of this.listeners) listener();
  }

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

  /** Run one backend operation, one at a time. A failure leaves the data as it was and says why. */
  private async run<T>(what: string, op: (api: BuilderApi) => Promise<T>, done: (result: T) => Partial<BuilderState>): Promise<boolean> {
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

  private refuse(message: string): Promise<boolean> {
    this.set({ error: message, notice: null });
    return Promise.resolve(false);
  }

  /** Keep the recipe a draft describes. A draft with something wrong is refused here and nothing is sent. */
  saveDraft(d: Draft): Promise<boolean> {
    const r = recipeOf(d);
    if ("bad" in r) return this.refuse(r.bad);
    return this.saveRecipe(r.recipe, d.editing ? d.editing.sha256 : null);
  }

  /** Keep a recipe: new (`replaces` null) or a change to the saved recipe whose SHA-256 is `replaces`. */
  saveRecipe(recipe: Recipe, replaces: string | null): Promise<boolean> {
    if (recipe.id === "") return this.refuse("Give the recipe a name that has some letters or digits in it.");
    if (recipe.name.trim() === "") return this.refuse("Give the recipe a name.");
    if (recipe.game_id === "") return this.refuse("Pick a game first. Scan for games on the Game Library page if the list is empty.");
    if (recipe.steps.length === 0) return this.refuse("Add at least one step.");
    return this.run("save", (api) => api.saveRecipe({ ...recipe, name: recipe.name.trim(), description: recipe.description.trim() }, replaces), (view) => ({
      view,
      notice: `Saved the recipe “${recipe.name.trim()}”. Nothing was built, and no mod or game file was read.`,
    }));
  }

  /** What the run would build. Writes nothing. Nothing is built until `confirmRun`. */
  async previewRun(recipeId: string, rootId: string): Promise<boolean> {
    if (rootId === "") return this.refuse("Add a library folder for mods first, on the Game Library page: a run builds into its workspace.");
    return this.run("preview", (api) => api.planRecipe(recipeId), (plan) => ({ preview: { recipeId, rootId, plan } }));
  }

  closePreview(): void {
    this.set({ preview: null });
  }

  /** Build what the preview showed. The core plans again and refuses a blocked or changed plan. */
  async confirmRun(): Promise<boolean> {
    const p = this.state.preview;
    if (!p) return false;
    if (p.plan.problems.length > 0) return this.refuse("That run is blocked, so nothing was built. The preview lists what blocks it.");
    const rootLabel = this.state.view?.mod_roots.find((r) => r.id === p.rootId)?.label ?? p.rootId;
    return this.run("run", (api) => api.runRecipe(p.recipeId, p.rootId), (r) => ({
      view: r.view,
      preview: null,
      notice: `Built ${count(r.record.files.length, "file")} into the workspace of “${rootLabel}” as run ${r.record.run_id}. No game was changed and nothing was deleted.`,
    }));
  }

  showRun(recipeId: string, ref: RunRef): Promise<boolean> {
    return this.run("read", (api) => api.readRun(recipeId, ref.root_id, ref.run_id), (record) => ({ openRun: { recipeId, rootId: ref.root_id, record } }));
  }

  closeRun(): void {
    this.set({ openRun: null });
  }

  verifyRun(recipeId: string, ref: RunRef): Promise<boolean> {
    return this.run("check", (api) => api.verifyRun(recipeId, ref.root_id, ref.run_id), (report) => ({ checks: { ...this.state.checks, [runKey(recipeId, ref)]: report } }));
  }

  /** Look up some of a mod's files and their hashes, to fill in a patch step. */
  lookUpFiles(m: { game_id: string; mod_id: string; version: string }, filter: string): Promise<boolean> {
    return this.run("lookup", (api) => api.modFiles(m.game_id, m.mod_id, m.version, filter), (files) => ({ files }));
  }

  clearFiles(): void {
    this.set({ files: null });
  }
}
