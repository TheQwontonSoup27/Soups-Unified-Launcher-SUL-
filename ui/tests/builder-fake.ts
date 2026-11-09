// A backend for the Builder tab that lives in memory. It is NOT the core: it keeps the shape of the real answers and
// refuses the few things a test needs refused, with plain-text errors like a failing command. What the core really
// refuses (a stale recipe, a patch that does not fit its file, a mod that is not stored, a game with no adapter) is
// tested in `crates/sul-core/src/builder/` and `src-tauri/src/ipc_tests.rs`.
import { createHash } from "node:crypto";
import type { BuilderApi, BuilderView, ModFile, ModFileList, Recipe, RecipeView, RunPlan, RunRecord } from "../src/lib/builder";
import { modRoot, mod } from "./mods-fake";

export { mod, modRoot };

export const EMPTY_BUILDER_VIEW: BuilderView = { recipes: [], mods: [], mod_roots: [], deployable_games: [], run_errors: [] };

/** A stand-in for the recipe's identity: stable, 64 hex digits, and different for different recipes. */
export const shaOf = (r: Recipe): string => createHash("sha256").update(JSON.stringify(r)).digest("hex");

export function recipeView(recipe: Recipe, extra: Partial<RecipeView> = {}): RecipeView {
  return { recipe, sha256: shaOf(recipe), has_runtime_code: recipe.steps.some((s) => s.op === "runtime_code"), runs: [], ...extra };
}

export function aRecipe(gameId: string, id = "loud", extra: Partial<Recipe> = {}): Recipe {
  return {
    schema: 1,
    id,
    name: id.toUpperCase(),
    game_id: gameId,
    description: "",
    steps: [{ op: "layer_mod", mod_id: "louder", version: "1.0", into: "" }],
    ...extra,
  };
}

export const HEX64 = "ab".repeat(32);

export function aPlan(extra: Partial<RunPlan> = {}): RunPlan {
  return {
    recipe_sha256: HEX64,
    files: [{ path: "data/a.bin", sha256: "cd".repeat(32), size: 4, origin: "step 1: louder 1.0", replaced: [] }],
    inputs: [{ step: 0, mod_id: "louder", version: "1.0", source: "made by hand", license_note: "mine", inventory_sha256: "ef".repeat(32) }],
    patches: [],
    total_bytes: 4,
    output_sha256: "12".repeat(32),
    warnings: [],
    problems: [],
    ...extra,
  };
}

export interface FakeCall {
  op: string;
  args: unknown[];
}

export function fakeBuilder(initial: Partial<BuilderView> = {}) {
  const calls: FakeCall[] = [];
  let view: BuilderView = { ...EMPTY_BUILDER_VIEW, ...initial };
  const failures = new Map<string, string>();
  let gate: Promise<void> | null = null;
  let plan: RunPlan | null = null;
  let files: ModFile[] = [];
  let runCount = 0;

  const enter = async (op: string, ...args: unknown[]) => {
    calls.push({ op, args });
    if (gate) await gate;
    const failure = failures.get(op);
    if (failure !== undefined) {
      failures.delete(op);
      throw failure; // `invoke` rejects with a plain string
    }
  };
  const snapshot = (): BuilderView => structuredClone(view);
  const needPlan = (recipeId: string): { rv: RecipeView; plan: RunPlan } => {
    const rv = view.recipes.find((r) => r.recipe.id === recipeId);
    if (!rv) throw `there is no saved recipe “${recipeId}”`;
    if (!view.deployable_games.includes(rv.recipe.game_id)) throw `SUL has no adapter for “${rv.recipe.game_id}”, so it does not know which folders a build for that game may hold. Nothing was changed.`;
    if (rv.has_runtime_code) throw "this recipe changes game code at run time, which SUL cannot do; it can be saved but not planned or run";
    return { rv, plan: structuredClone(plan ?? aPlan()) };
  };

  const api: BuilderApi = {
    async view() {
      await enter("view");
      return snapshot();
    },
    async saveRecipe(recipe, replaces) {
      await enter("saveRecipe", recipe, replaces);
      const at = view.recipes.findIndex((r) => r.recipe.id === recipe.id);
      if (replaces === null) {
        if (at >= 0) throw `a recipe “${recipe.id}” already exists; open it to change it, or pick another id`;
        view = { ...view, recipes: [...view.recipes, recipeView(recipe)] };
      } else {
        if (at < 0) throw `there is no saved recipe “${recipe.id}” to replace`;
        if (view.recipes[at]!.sha256 !== replaces) throw `recipe “${recipe.id}” changed since it was opened; open it again and redo the change`;
        view = { ...view, recipes: view.recipes.map((r, i) => (i === at ? recipeView(recipe, { runs: r.runs }) : r)) };
      }
      return snapshot();
    },
    async planRecipe(recipeId) {
      await enter("planRecipe", recipeId);
      return needPlan(recipeId).plan;
    },
    async runRecipe(recipeId, rootId) {
      await enter("runRecipe", recipeId, rootId);
      const { rv, plan: p } = needPlan(recipeId);
      if (p.problems.length > 0) throw `the plan has problems: ${p.problems.join("; ")}`;
      runCount += 1;
      const run_id = `run-${runCount}`;
      const record: RunRecord = {
        schema: 1,
        run_id,
        started_unix: 1_700_000_000,
        recipe: rv.recipe,
        recipe_sha256: rv.sha256,
        inputs: p.inputs,
        patches: p.patches,
        files: p.files,
        output_sha256: p.output_sha256,
        warnings: p.warnings,
      };
      view = { ...view, recipes: view.recipes.map((r) => (r.recipe.id === recipeId ? { ...r, runs: [...r.runs, { root_id: rootId, run_id }] } : r)) };
      return { record, view: snapshot() };
    },
    async readRun(recipeId, rootId, runId) {
      await enter("readRun", recipeId, rootId, runId);
      const rv = view.recipes.find((r) => r.recipe.id === recipeId);
      if (!rv?.runs.some((r) => r.root_id === rootId && r.run_id === runId)) throw `there is no run “${runId}” of “${recipeId}” in “${rootId}”`;
      const p = plan ?? aPlan();
      return { schema: 1, run_id: runId, started_unix: 1_700_000_000, recipe: rv.recipe, recipe_sha256: rv.sha256, inputs: p.inputs, patches: p.patches, files: p.files, output_sha256: p.output_sha256, warnings: p.warnings };
    },
    async verifyRun(recipeId, rootId, runId) {
      await enter("verifyRun", recipeId, rootId, runId);
      return { missing: [], modified: [], extra: [] };
    },
    async modFiles(gameId, modId, version, filter): Promise<ModFileList> {
      await enter("modFiles", gameId, modId, version, filter);
      const matching = files.filter((f) => f.path.includes(filter));
      return { files: matching.slice(0, 200), matching: matching.length, total: files.length };
    },
  };

  return {
    api,
    calls,
    /** The calls that change something (a save or a run). Everything else only reads. */
    changes: () => calls.filter((c) => c.op === "saveRecipe" || c.op === "runRecipe"),
    failNext(op: string, message: string) {
      failures.set(op, message);
    },
    /** What the next previews, runs and records show. */
    setPlan(p: RunPlan) {
      plan = p;
    },
    setFiles(f: ModFile[]) {
      files = f;
    },
    /** Hold every call open until `release()`. */
    hold(): () => void {
      let release!: () => void;
      gate = new Promise<void>((resolve) => (release = resolve));
      return () => {
        gate = null;
        release();
      };
    },
    current: () => snapshot(),
  };
}
