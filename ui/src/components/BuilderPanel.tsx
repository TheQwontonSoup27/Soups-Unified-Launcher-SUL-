import { useEffect, useMemo, useState, useSyncExternalStore, type Dispatch, type SetStateAction } from "react";
import { Dialog } from "./Dialog";
import { StatusBadge } from "./StatusBadge";
import type { GameView } from "../lib/library";
import { formatSize, modKey, type ModView, type VerifyReport } from "../lib/mods";
import {
  BuilderController,
  EMPTY_DRAFT,
  canPlanGame,
  describePlan,
  describeStep,
  describeVerifyRun,
  draftOf,
  runKey,
  shortHash,
  tauriBuilderApi,
  type BuilderApi,
  type BuilderView,
  type Draft,
  type DraftStep,
  type RecipeView,
  type RunPlan,
  type RunRecord,
} from "../lib/builder";

/** Rows of a preview or a record shown before "and N more"; the core still does every file. */
const SHOWN_ROWS = 200;

const nameOf = (games: GameView[], id: string) => games.find((g) => g.id === id)?.name ?? id;

function List({ items, label, className }: { items: string[]; label: string; className?: string }) {
  if (items.length === 0) return null;
  return (
    <ul className={className} aria-label={label} role={className === "matrix-error" ? "alert" : undefined}>
      {items.map((m, i) => (
        <li key={i}>{m}</li>
      ))}
    </ul>
  );
}

/**
 * The mod builder and patches (spec 2.2 and 2.3). `api` is the backend to use; `undefined` means the real one and `null`
 * means none (a plain browser, or a test). `games` are the games the library lists, for their names.
 *
 * A recipe is only a description: saving one reads and writes nothing but the recipe list. Previewing a run writes
 * nothing. Building the run is asked about first, and the core plans again when it runs, so whatever this page sends it
 * will not build a blocked plan. A run is built into a workspace folder in the mod library, never into a game.
 */
export function BuilderPanel({ api, games }: { api?: BuilderApi | null; games: GameView[] }) {
  const controller = useMemo(() => new BuilderController(api === undefined ? tauriBuilderApi() : api), [api]);
  const state = useSyncExternalStore(controller.subscribe, controller.getSnapshot, controller.getSnapshot);
  const view = state.view;
  const busy = state.busy !== null;
  // The recipe being written lives here, so "Change…" on a saved recipe can fill it in.
  const [draft, setDraft] = useState<Draft>(EMPTY_DRAFT);

  useEffect(() => {
    void controller.load();
  }, [controller]);

  if (!controller.hasBackend) {
    return (
      <section className="card managed" aria-label="Mod builder and patches">
        <header className="game-card-head">
          <h3>Mod builder and patches</h3>
          <StatusBadge status="experimental" />
        </header>
        <p className="muted">Browser preview: the mod builder needs the desktop app, so nothing is listed here.</p>
      </section>
    );
  }

  return (
    <section className="managed builder" aria-label="Mod builder and patches">
      <div className="card">
        <header className="game-card-head">
          <h3>Mod builder and patches</h3>
          <StatusBadge status="experimental" />
        </header>
        <p className="muted">
          Experimental: tested on made-up folders and files only. A recipe lists steps that layer mods and change bytes in a copy of one file; a run builds the result into a
          workspace folder in your mod library and records where every file came from. Nothing here changes a game, and a patch only ever writes a new file. SUL can preview or
          run a recipe only for a game that has an adapter, and no adapter for a real game exists yet, so previews and runs are refused for every real game today. A step that
          changes game code while the game runs can be written down but SUL cannot run it.
        </p>
        <p className="muted" role="status" aria-live="polite">
          {busy ? "Working… building a large run can take a while." : (state.notice ?? "")}
        </p>
        {state.error && (
          <p className="matrix-error" role="alert">
            {state.error}
          </p>
        )}
        {view && <List items={view.run_errors} label="Runs that could not be read" className="matrix-error" />}
      </div>

      {view && (
        <>
          <Recipes
            controller={controller}
            view={view}
            games={games}
            busy={busy}
            checks={state.checks}
            onChange={(rv) => {
              setDraft(draftOf(rv));
              controller.clearFiles();
            }}
          />
          <Editor controller={controller} view={view} games={games} busy={busy} draft={draft} setDraft={setDraft} />
        </>
      )}

      {state.preview && <PreviewDialog preview={state.preview} view={view} games={games} controller={controller} busy={busy} />}
      {state.openRun && <RecordDialog record={state.openRun.record} onClose={() => controller.closeRun()} />}
    </section>
  );
}

// ---------------------------------------------------------------------------
// Saved recipes
// ---------------------------------------------------------------------------

function Recipes({ controller, view, games, busy, checks, onChange }: { controller: BuilderController; view: BuilderView; games: GameView[]; busy: boolean; checks: Record<string, VerifyReport>; onChange: (rv: RecipeView) => void }) {
  const [rootId, setRootId] = useState("");
  const root = view.mod_roots.find((r) => r.id === rootId) ?? view.mod_roots[0];
  return (
    <div className="card" aria-label="Recipes">
      <h3>Recipes</h3>
      {view.recipes.length === 0 && <p className="muted">No recipes yet. Write one below.</p>}
      {view.recipes.length > 0 && (
        <label className="field">
          Build runs into the workspace of
          <select value={root?.id ?? ""} onChange={(e) => setRootId(e.target.value)}>
            {view.mod_roots.length === 0 && <option value="">(add a library folder for mods first)</option>}
            {view.mod_roots.map((r) => (
              <option key={r.id} value={r.id}>
                {r.label}
              </option>
            ))}
          </select>
        </label>
      )}
      <ul className="trust-list">
        {view.recipes.map((rv) => (
          <RecipeRow key={rv.recipe.id} rv={rv} view={view} games={games} busy={busy} rootId={root?.id ?? ""} checks={checks} controller={controller} onChange={onChange} />
        ))}
      </ul>
    </div>
  );
}

function RecipeRow({
  rv,
  view,
  games,
  busy,
  rootId,
  checks,
  controller,
  onChange,
}: {
  rv: RecipeView;
  view: BuilderView;
  games: GameView[];
  busy: boolean;
  rootId: string;
  checks: Record<string, VerifyReport>;
  controller: BuilderController;
  onChange: (rv: RecipeView) => void;
}) {
  const r = rv.recipe;
  const planable = canPlanGame(view, r.game_id) && !rv.has_runtime_code;
  return (
    <li aria-label={`Recipe ${r.name}`}>
      <span className="trust-title">{r.name}</span> <span className="tag">{r.steps.length === 1 ? "1 step" : `${r.steps.length} steps`}</span>{" "}
      {rv.has_runtime_code && <span className="tag tag-warn">Changes game code: SUL cannot run it</span>}
      <p className="muted">
        <code>{r.id}</code> · {nameOf(games, r.game_id)}
        {r.description !== "" && ` · ${r.description}`}
      </p>
      <ol className="muted steps" aria-label={`Steps of ${r.name}`}>
        {r.steps.map((s, i) => (
          <li key={i}>{describeStep(s)}</li>
        ))}
      </ol>
      {!canPlanGame(view, r.game_id) && <p className="muted">This game has no adapter yet, so the recipe can be saved but not previewed or run.</p>}
      {rv.has_runtime_code && <p className="muted">A runtime-code step is only a note: SUL does not change game code, so this recipe cannot be previewed or run.</p>}
      <div className="row">
        <button type="button" className="btn" disabled={busy || !planable || rootId === ""} aria-label={`Preview a run of ${r.name}`} onClick={() => void controller.previewRun(r.id, rootId)}>
          Preview run…
        </button>
        <button type="button" className="btn btn-ghost" disabled={busy} aria-label={`Change ${r.name}`} onClick={() => onChange(rv)}>
          Change…
        </button>
      </div>
      {rv.runs.length > 0 && (
        <>
          <h4>Runs</h4>
          <ul className="trust-list" aria-label={`Runs of ${r.name}`}>
            {rv.runs.map((run) => {
              const check = checks[runKey(r.id, run)];
              const label = view.mod_roots.find((m) => m.id === run.root_id)?.label ?? run.root_id;
              return (
                <li key={`${run.root_id}/${run.run_id}`}>
                  Run <code>{run.run_id}</code> in “{label}”
                  {check && <p className="muted">Last check: {describeVerifyRun(check)}</p>}
                  <div className="row">
                    <button type="button" className="btn btn-ghost" disabled={busy} aria-label={`Check the files of run ${run.run_id}`} onClick={() => void controller.verifyRun(r.id, run)}>
                      Check files
                    </button>
                    <button type="button" className="btn btn-ghost" disabled={busy} aria-label={`Show the record of run ${run.run_id}`} onClick={() => void controller.showRun(r.id, run)}>
                      Show record
                    </button>
                  </div>
                </li>
              );
            })}
          </ul>
        </>
      )}
    </li>
  );
}

// ---------------------------------------------------------------------------
// Writing a recipe
// ---------------------------------------------------------------------------

function Editor({
  controller,
  view,
  games,
  busy,
  draft,
  setDraft,
}: {
  controller: BuilderController;
  view: BuilderView;
  games: GameView[];
  busy: boolean;
  draft: Draft;
  setDraft: Dispatch<SetStateAction<Draft>>;
}) {
  const [lookupStep, setLookupStep] = useState<number | null>(null);
  const [filter, setFilter] = useState("");
  const state = useSyncExternalStore(controller.subscribe, controller.getSnapshot, controller.getSnapshot);

  const gameIds = [...new Set([...games.map((g) => g.id), ...view.mods.map((m) => m.game_id), ...view.recipes.map((r) => r.recipe.game_id)])];
  const game = draft.game_id !== "" && gameIds.includes(draft.game_id) ? draft.game_id : "";
  const modsOfGame = view.mods.filter((m) => m.game_id === game);

  const edit = (patch: Partial<Draft>) => setDraft((d) => ({ ...d, ...patch }));
  const editStep = (i: number, patch: Partial<DraftStep>) => setDraft((d) => ({ ...d, steps: d.steps.map((s, j) => (j === i ? ({ ...s, ...patch } as DraftStep) : s)) }));
  const addStep = (s: DraftStep) => setDraft((d) => ({ ...d, steps: [...d.steps, s] }));
  const removeStep = (i: number) => {
    setDraft((d) => ({ ...d, steps: d.steps.filter((_, j) => j !== i) }));
    setLookupStep(null);
    controller.clearFiles();
  };
  const moveStep = (i: number, by: -1 | 1) => {
    // A file lookup belongs to the step it was made for, so it is dropped when the steps change places.
    setLookupStep(null);
    controller.clearFiles();
    setDraft((d) => {
      const j = i + by;
      if (j < 0 || j >= d.steps.length) return d;
      const steps = [...d.steps];
      [steps[i], steps[j]] = [steps[j]!, steps[i]!];
      return { ...d, steps };
    });
  };

  const lookUp = (i: number, m: ModView | undefined) => {
    if (!m) return;
    setLookupStep(i);
    void controller.lookUpFiles(m, filter);
  };

  return (
    <div className="card" aria-label="Write a recipe">
      <h3>{draft.editing ? `Change the recipe “${draft.name}”` : "Write a recipe"}</h3>
      <p className="muted">
        Steps run in order. Layering a mod copies its files into the workspace, and a later layer replaces an earlier file at the same path. A patch changes bytes at fixed
        positions of one file in the workspace, and only if that file is exactly the one you named by its SHA-256; it writes a new file and never edits the stored mod.
        {draft.editing && " Saving replaces the saved recipe, unless it was changed somewhere else since you opened it."}
      </p>
      <form className="row" aria-label="Recipe details" onSubmit={(e) => e.preventDefault()}>
        <label className="field">
          Game
          <select value={game} disabled={draft.editing !== null} onChange={(e) => edit({ game_id: e.target.value })}>
            <option value="">{gameIds.length === 0 ? "(no game yet)" : "(pick a game)"}</option>
            {gameIds.map((id) => (
              <option key={id} value={id}>
                {nameOf(games, id)}
              </option>
            ))}
          </select>
        </label>
        <label className="field">
          Recipe name
          <input type="text" value={draft.name} disabled={draft.editing !== null} onChange={(e) => edit({ name: e.target.value })} />
        </label>
        <label className="field">
          What it is for
          <input type="text" value={draft.description} onChange={(e) => edit({ description: e.target.value })} />
        </label>
      </form>

      <h4>Steps</h4>
      {draft.steps.length === 0 && <p className="muted">No steps yet.</p>}
      <ol className="trust-list" aria-label="Steps of the recipe being written">
        {draft.steps.map((s, i) => (
          <li key={i} aria-label={`Step ${i + 1}`}>
            <p className="trust-title">
              Step {i + 1}: {s.op === "layer_mod" ? "layer a mod" : s.op === "patch_file" ? "patch a file" : "runtime-code note"}
            </p>
            {s.op === "layer_mod" && (
              <div className="row">
                <label className="field">
                  Layer this mod
                  <select
                    value={s.mod_id === "" ? "" : modKey({ game_id: game, mod_id: s.mod_id, version: s.version })}
                    onChange={(e) => {
                      const m = modsOfGame.find((x) => modKey(x) === e.target.value);
                      editStep(i, { mod_id: m?.mod_id ?? "", version: m?.version ?? "" });
                    }}
                  >
                    <option value="">{game === "" ? "(pick a game first)" : modsOfGame.length === 0 ? "(no mods for this game yet)" : "(pick a mod)"}</option>
                    {modsOfGame.map((m) => (
                      <option key={modKey(m)} value={modKey(m)}>
                        {m.name} {m.version}
                      </option>
                    ))}
                  </select>
                </label>
                <label className="field">
                  Into this folder of the workspace (blank for the top)
                  <input type="text" value={s.into} onChange={(e) => editStep(i, { into: e.target.value })} />
                </label>
              </div>
            )}
            {s.op === "patch_file" && (
              <>
                <div className="row">
                  <label className="field">
                    Patch name
                    <input type="text" value={s.patch_name} onChange={(e) => editStep(i, { patch_name: e.target.value })} />
                  </label>
                  <label className="field">
                    What it does
                    <input type="text" value={s.description} onChange={(e) => editStep(i, { description: e.target.value })} />
                  </label>
                  <label className="field">
                    File to patch (path in the workspace)
                    <input type="text" value={s.path} onChange={(e) => editStep(i, { path: e.target.value })} />
                  </label>
                  <label className="field">
                    The file&apos;s SHA-256
                    <input type="text" value={s.expect_sha256} onChange={(e) => editStep(i, { expect_sha256: e.target.value })} />
                  </label>
                </div>
                <label className="field">
                  Changes: one per line, as offset, old bytes, new bytes (hexadecimal, same length)
                  <textarea rows={3} value={s.hunks} onChange={(e) => editStep(i, { hunks: e.target.value })} />
                </label>
                <details>
                  <summary>Look up a file&apos;s SHA-256 from a mod</summary>
                  <div className="row">
                    <label className="field">
                      Words in the file&apos;s path
                      <input type="text" value={filter} onChange={(e) => setFilter(e.target.value)} />
                    </label>
                    {modsOfGame.map((m) => (
                      <button key={modKey(m)} type="button" className="btn btn-ghost" disabled={busy} aria-label={`List files of ${m.name} ${m.version} for step ${i + 1}`} onClick={() => lookUp(i, m)}>
                        Files of {m.name} {m.version}
                      </button>
                    ))}
                  </div>
                  {modsOfGame.length === 0 && <p className="muted">No mod is stored for this game yet, so there is nothing to look up.</p>}
                  {lookupStep === i && state.files && (
                    <>
                      <p className="muted">
                        {state.files.matching > state.files.files.length ? `Showing ${state.files.files.length} of ${state.files.matching} matching files` : `${state.files.matching} matching`}{" "}
                        (the mod has {state.files.total}).
                      </p>
                      <ul className="trust-list" aria-label="Files of the mod">
                        {state.files.files.map((f) => (
                          <li key={f.path}>
                            <code>{f.path}</code> · {formatSize(f.size)} · <code>{shortHash(f.sha256)}</code>{" "}
                            <button type="button" className="btn btn-ghost" aria-label={`Use ${f.path} in step ${i + 1}`} onClick={() => editStep(i, { path: f.path, expect_sha256: f.sha256 })}>
                              Use this file
                            </button>
                          </li>
                        ))}
                      </ul>
                    </>
                  )}
                </details>
              </>
            )}
            {s.op === "runtime_code" && (
              <label className="field">
                Runtime-code change (a note only: SUL cannot run it)
                <input type="text" value={s.description} onChange={(e) => editStep(i, { description: e.target.value })} />
              </label>
            )}
            <div className="row">
              <button type="button" className="btn btn-ghost" disabled={i === 0} aria-label={`Move step ${i + 1} up`} onClick={() => moveStep(i, -1)}>
                Move up
              </button>
              <button type="button" className="btn btn-ghost" disabled={i === draft.steps.length - 1} aria-label={`Move step ${i + 1} down`} onClick={() => moveStep(i, 1)}>
                Move down
              </button>
              <button type="button" className="btn btn-ghost" aria-label={`Remove step ${i + 1}`} onClick={() => removeStep(i)}>
                Remove step
              </button>
            </div>
          </li>
        ))}
      </ol>
      <div className="row">
        <button type="button" className="btn btn-ghost" onClick={() => addStep({ op: "layer_mod", mod_id: "", version: "", into: "" })}>
          Add: layer a mod
        </button>
        <button type="button" className="btn btn-ghost" onClick={() => addStep({ op: "patch_file", path: "", patch_name: "", description: "", expect_sha256: "", hunks: "" })}>
          Add: patch a file
        </button>
        <button type="button" className="btn btn-ghost" onClick={() => addStep({ op: "runtime_code", description: "" })}>
          Add: runtime-code note
        </button>
      </div>
      <div className="row">
        <button
          type="button"
          className="btn"
          disabled={busy}
          onClick={() => {
            const d = { ...draft, game_id: game };
            void controller.saveDraft(d).then((ok) => {
              if (ok) {
                setDraft(EMPTY_DRAFT);
                setLookupStep(null);
                controller.clearFiles();
              }
            });
          }}
        >
          {draft.editing ? "Save the change" : "Save the recipe"}
        </button>
        {(draft.editing || draft.steps.length > 0 || draft.name !== "") && (
          <button
            type="button"
            className="btn btn-ghost"
            onClick={() => {
              setDraft(EMPTY_DRAFT);
              setLookupStep(null);
              controller.clearFiles();
            }}
          >
            Start over
          </button>
        )}
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Previews and records
// ---------------------------------------------------------------------------

function PlanBody({ plan }: { plan: RunPlan }) {
  const shown = plan.files.slice(0, SHOWN_ROWS);
  return (
    <>
      <p>{describePlan(plan)}</p>
      <List items={plan.problems} label="Problems that block this run" className="matrix-error" />
      <List items={plan.warnings} label="Things to check before this run" className="muted" />
      {plan.inputs.length > 0 && (
        <>
          <h4>Mods layered</h4>
          <ul aria-label="Mods layered">
            {plan.inputs.map((m) => (
              <li key={`${m.step}`}>
                Step {m.step + 1}: <code>{m.mod_id}</code> {m.version}
                {m.source !== "" && <> · from {m.source}</>}
                {m.license_note !== "" && <> · licence note (your words, not checked): {m.license_note}</>}
              </li>
            ))}
          </ul>
        </>
      )}
      {plan.patches.length > 0 && (
        <>
          <h4>Patches applied</h4>
          <ul aria-label="Patches applied">
            {plan.patches.map((p) => (
              <li key={`${p.step}`}>
                Step {p.step + 1}: <code>{p.outcome.patch_id}</code> on <code>{p.path}</code> · {p.outcome.hunks} {p.outcome.hunks === 1 ? "change" : "changes"}, {p.outcome.bytes_changed}{" "}
                {p.outcome.bytes_changed === 1 ? "byte" : "bytes"} · <code>{shortHash(p.outcome.before_sha256)}</code> to <code>{shortHash(p.outcome.after_sha256)}</code>
              </li>
            ))}
          </ul>
        </>
      )}
      <h4>Files</h4>
      <ul aria-label="Files the run builds">
        {shown.map((f) => (
          <li key={f.path}>
            <code>{f.path}</code> · {formatSize(f.size)} · from {f.origin}
            {f.replaced.length > 0 && <> · replaces the same path from {f.replaced.join(", ")}</>}
          </li>
        ))}
      </ul>
      {plan.files.length > shown.length && <p className="muted">…and {plan.files.length - shown.length} more files. SUL still builds and records all of them.</p>}
      <p className="muted">
        Result fingerprint <code>{shortHash(plan.output_sha256)}</code>. The same recipe over the same stored mods gives the same fingerprint.
      </p>
    </>
  );
}

function PreviewDialog({ preview, view, games, controller, busy }: { preview: NonNullable<ReturnType<BuilderController["getSnapshot"]>["preview"]>; view: BuilderView | null; games: GameView[]; controller: BuilderController; busy: boolean }) {
  const recipe = view?.recipes.find((r) => r.recipe.id === preview.recipeId)?.recipe;
  const root = view?.mod_roots.find((r) => r.id === preview.rootId);
  const blocked = preview.plan.problems.length > 0;
  return (
    <Dialog
      title={`Build “${recipe?.name ?? preview.recipeId}”?`}
      onClose={() => controller.closePreview()}
      actions={
        <>
          <button type="button" className="btn btn-ghost" onClick={() => controller.closePreview()}>
            Cancel
          </button>
          <button type="button" className="btn" disabled={busy || blocked} onClick={() => void controller.confirmRun()}>
            Build into the workspace
          </button>
        </>
      }
    >
      <p>
        This builds the files below into a new workspace folder in “{root?.label ?? preview.rootId}”
        {recipe ? ` for ${nameOf(games, recipe.game_id)}` : ""}. No game file is touched, no mod is changed and nothing is deleted. The preview is not trusted: SUL works it out again
        when you confirm and refuses to build if anything changed.
      </p>
      <PlanBody plan={preview.plan} />
    </Dialog>
  );
}

function RecordDialog({ record, onClose }: { record: RunRecord; onClose: () => void }) {
  const shown = record.files.slice(0, SHOWN_ROWS);
  return (
    <Dialog
      title={`Run ${record.run_id}`}
      onClose={onClose}
      actions={
        <button type="button" className="btn" onClick={onClose}>
          Close
        </button>
      }
    >
      <p>
        Recipe “{record.recipe.name}”, built {new Date(record.started_unix * 1000).toISOString().slice(0, 16).replace("T", " ")} UTC from {record.inputs.length}{" "}
        {record.inputs.length === 1 ? "mod" : "mods"} and {record.patches.length} {record.patches.length === 1 ? "patch" : "patches"}: {record.files.length}{" "}
        {record.files.length === 1 ? "file" : "files"}. Result fingerprint <code>{shortHash(record.output_sha256)}</code>.
      </p>
      <List items={record.warnings} label="Warnings recorded for this run" className="muted" />
      {record.inputs.length > 0 && (
        <ul aria-label="Mods the run used">
          {record.inputs.map((m) => (
            <li key={m.step}>
              Step {m.step + 1}: <code>{m.mod_id}</code> {m.version}
              {m.source !== "" && <> · from {m.source}</>}
              {m.license_note !== "" && <> · licence note (your words, not checked): {m.license_note}</>}
            </li>
          ))}
        </ul>
      )}
      <ul aria-label="Files of the run">
        {shown.map((f) => (
          <li key={f.path}>
            <code>{f.path}</code> · {formatSize(f.size)} · from {f.origin}
          </li>
        ))}
      </ul>
      {record.files.length > shown.length && <p className="muted">…and {record.files.length - shown.length} more files.</p>}
    </Dialog>
  );
}
