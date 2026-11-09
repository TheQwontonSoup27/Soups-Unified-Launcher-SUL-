import { useEffect, useMemo, useState, useSyncExternalStore } from "react";
import { Dialog } from "./Dialog";
import { StatusBadge } from "./StatusBadge";
import type { GameView } from "../lib/library";
import {
  BUILD_STATE_LABEL,
  BUILTIN_VARIANTS,
  ManagerController,
  ROOT_KIND_LABEL,
  buildsOf,
  canRollBack,
  describeCheck,
  describeLocation,
  formatBytes,
  gamesWithBuilds,
  rootProblem,
  slugId,
  tauriManagerApi,
  variantLabel,
  type ManagerApi,
  type RootKind,
  type RootView,
  type Variant,
} from "../lib/manager";

type Confirm =
  | { kind: "import"; game: GameView; root: RootView; variant: Variant; version: string; description: string }
  | { kind: "register"; game: GameView; variant: Variant; version: string; description: string }
  | { kind: "forget"; root: RootView }
  | null;

/**
 * Library folders and managed builds (spec 1.2, 1.3, 1.5). `api` is the backend to use; `undefined` means the real
 * one and `null` means none (a plain browser, or a test). `games` are the games the library lists.
 *
 * Every action that copies or removes something from SUL's list asks first. The question is not the safety check:
 * the core refuses unsafe actions whatever this page sends.
 */
export function ManagedBuildsPanel({ api, games }: { api?: ManagerApi | null; games: GameView[] }) {
  const controller = useMemo(() => new ManagerController(api === undefined ? tauriManagerApi() : api), [api]);
  const state = useSyncExternalStore(controller.subscribe, controller.getSnapshot, controller.getSnapshot);
  const view = state.view;
  const busy = state.busy !== null;

  useEffect(() => {
    void controller.load();
  }, [controller]);

  // Add-a-folder form.
  const [name, setName] = useState("");
  const [path, setPath] = useState("");
  const [kind, setKind] = useState<RootKind>("builds");
  // Point-a-folder-at-its-new-place form.
  const [relinkId, setRelinkId] = useState<string | null>(null);
  const [relinkPath, setRelinkPath] = useState("");
  // Add-a-build form.
  const present = games.filter((g) => g.present);
  const [gameId, setGameId] = useState("");
  const [variantKind, setVariantKind] = useState<string>("original");
  const [customName, setCustomName] = useState("");
  const [version, setVersion] = useState("");
  const [description, setDescription] = useState("");
  const [rootId, setRootId] = useState("");
  const [confirm, setConfirm] = useState<Confirm>(null);
  const [formError, setFormError] = useState<string | null>(null);

  const buildRoots = (view?.roots ?? []).filter((r) => r.kind === "builds");
  const chosenGame = present.find((g) => g.id === (gameId || present[0]?.id));
  const chosenRoot = buildRoots.find((r) => r.id === (rootId || buildRoots[0]?.id));

  const variant = (): Variant | null => {
    if (variantKind !== "custom") return variantKind as Variant;
    const id = slugId(customName);
    return id === "" ? null : { custom: id };
  };

  /** Check the add-a-build form; on success open the question. */
  const ask = (action: "import" | "register") => {
    setFormError(null);
    const v = variant();
    if (!chosenGame) return setFormError("Pick a game first. Scan for games above if the list is empty.");
    if (v === null) return setFormError("Give the custom variant a name made of letters or digits.");
    if (version.trim() === "") return setFormError("Type the game's version (for example 1.0.0).");
    const rest = { variant: v, version: version.trim(), description: description.trim() || "my own install" };
    if (action === "register") return setConfirm({ kind: "register", game: chosenGame, ...rest });
    if (!chosenRoot) return setFormError("Add a library folder for builds first.");
    setConfirm({ kind: "import", game: chosenGame, root: chosenRoot, ...rest });
  };

  const sure = async () => {
    const c = confirm;
    setConfirm(null);
    if (!c) return;
    if (c.kind === "forget") await controller.forgetRoot(c.root);
    else if (c.kind === "register") await controller.registerInPlace(c.game, { variant: c.variant, baseVersion: c.version, sourceDescription: c.description });
    else await controller.importBuild(c.game, { variant: c.variant, baseVersion: c.version, sourceDescription: c.description }, c.root);
  };

  if (!controller.hasBackend) {
    return (
      <section className="card managed" aria-label="Folders and builds">
        <header className="game-card-head">
          <h3>Library folders and managed builds</h3>
          <StatusBadge status="experimental" />
        </header>
        <p className="muted">Browser preview: folders and builds need the desktop app, so nothing is listed here.</p>
      </section>
    );
  }

  return (
    <section className="managed" aria-label="Folders and builds">
      <div className="card">
        <header className="game-card-head">
          <h3>Library folders and managed builds</h3>
          <StatusBadge status="experimental" />
        </header>
        <p className="muted">
          Experimental: tested on made-up folders only. No adapter for a real game exists yet, and nothing here has been tried on a real game.
          Your original installs are only ever read.
        </p>
        <p className="muted" role="status" aria-live="polite">
          {busy ? "Working… this can take a while for a large game, and it cannot be cancelled once started." : (state.notice ?? "")}
        </p>
        {(state.error || formError) && (
          <p className="matrix-error" role="alert">
            {state.error ?? formError}
          </p>
        )}
        {view && view.interrupted.length > 0 && (
          <div className="matrix-error" role="alert" aria-label="Unfinished operations">
            <p>
              {view.interrupted.length === 1 ? "An earlier operation did not finish" : `${view.interrupted.length} earlier operations did not finish`} (a crash or a power cut).
              SUL does not repair this by itself; check the files it names before you go on.
            </p>
            <ul>
              {view.interrupted.map((i) => (
                <li key={i.id}>
                  <code>{i.op}</code>: {i.detail}
                </li>
              ))}
            </ul>
          </div>
        )}
      </div>

      <div className="card" aria-label="Library folders">
        <h3>Library folders</h3>
        <p className="muted">
          The folders where SUL keeps managed builds and mods. A folder must already exist. SUL writes one small marker file into it and nothing else,
          and never into a game&apos;s own install folder.
        </p>
        {view && view.roots.length === 0 && <p className="muted">No library folders yet.</p>}
        <ul className="trust-list">
          {(view?.roots ?? []).map((r) => {
            const problem = rootProblem(r);
            return (
              <li key={r.id}>
                <span className="trust-title">{r.label}</span> <span className="tag">{ROOT_KIND_LABEL[r.kind]}</span>
                <p className="muted">
                  <code>{r.path}</code> · {formatBytes(r.free_bytes)} free
                </p>
                {problem && (
                  <p className="matrix-error" role="alert">
                    {problem}
                  </p>
                )}
                <div className="row">
                  <button type="button" className="btn btn-ghost" disabled={busy} aria-label={`Point ${r.label} at a new place`} onClick={() => (setRelinkId(relinkId === r.id ? null : r.id), setRelinkPath(""))}>
                    New place…
                  </button>
                  <button type="button" className="btn btn-ghost" disabled={busy} aria-label={`Stop listing ${r.label}`} onClick={() => setConfirm({ kind: "forget", root: r })}>
                    Stop listing
                  </button>
                </div>
                {relinkId === r.id && (
                  <form
                    className="row"
                    aria-label={`New place for ${r.label}`}
                    onSubmit={(e) => {
                      e.preventDefault();
                      void controller.relinkRoot(r.id, relinkPath).then((ok) => ok && setRelinkId(null));
                    }}
                  >
                    <label className="field">
                      New path
                      <input type="text" value={relinkPath} onChange={(e) => setRelinkPath(e.target.value)} />
                    </label>
                    <button type="submit" className="btn" disabled={busy}>
                      Use this place
                    </button>
                  </form>
                )}
              </li>
            );
          })}
        </ul>
        <form
          className="row"
          aria-label="Add a library folder"
          onSubmit={(e) => {
            e.preventDefault();
            void controller.addRoot(name, path, kind).then((ok) => ok && (setName(""), setPath("")));
          }}
        >
          <label className="field">
            Name
            <input type="text" value={name} onChange={(e) => setName(e.target.value)} />
          </label>
          <label className="field">
            Path
            <input type="text" value={path} onChange={(e) => setPath(e.target.value)} />
          </label>
          <label className="field">
            Holds
            <select value={kind} onChange={(e) => setKind(e.target.value as RootKind)}>
              <option value="builds">{ROOT_KIND_LABEL.builds}</option>
              <option value="mods">{ROOT_KIND_LABEL.mods}</option>
            </select>
          </label>
          <button type="submit" className="btn" disabled={busy}>
            Add folder
          </button>
        </form>
      </div>

      <div className="card" aria-label="Managed builds">
        <h3>Managed builds</h3>
        <p className="muted">
          A build is a copy of a game&apos;s files that SUL can check, switch and roll back. Importing copies the whole install and checks the copy against the
          original. Recording an install in place copies nothing: it remembers the install as a read-only baseline.
        </p>
        {view && view.builds.length === 0 && <p className="muted">No builds yet.</p>}
        {view &&
          gamesWithBuilds(view, games).map(({ gameId: id, name: gameName }) => {
            const slots = [...new Set(buildsOf(view, id).map((b) => b.slot))];
            return (
              <article key={id} aria-label={`Builds of ${gameName}`}>
                <h4>{gameName}</h4>
                <ul className="trust-list">
                  {buildsOf(view, id).map((b) => {
                    const check = state.checks[b.id];
                    return (
                      <li key={b.id}>
                        <span className="trust-title">{variantLabel(b.variant)}</span> {b.active && <span className="tag">Active</span>}{" "}
                        <span className="muted">{BUILD_STATE_LABEL[b.state]}</span>
                        <p className="muted">
                          {describeLocation(b.location, view.roots)} · <code>{b.id}</code>
                        </p>
                        {check && <p className="muted">Last check: {describeCheck(check)}</p>}
                        <div className="row">
                          <button type="button" className="btn btn-ghost" disabled={busy} aria-label={`Check the files of ${b.id}`} onClick={() => void controller.verify(b.id)}>
                            Check files
                          </button>
                          <button type="button" className="btn btn-ghost" disabled={busy || b.active} aria-label={`Make ${b.id} the active build`} onClick={() => void controller.activate(b.id)}>
                            Make active
                          </button>
                        </div>
                      </li>
                    );
                  })}
                </ul>
                {slots.filter((s) => canRollBack(view, id, s)).map((s) => (
                  <button key={s} type="button" className="btn btn-ghost" disabled={busy} aria-label={`Roll the ${s} slot of ${gameName} back`} onClick={() => void controller.rollback(id, s)}>
                    Roll the {s} slot back
                  </button>
                ))}
              </article>
            );
          })}
        {view && view.retired.length > 0 && (
          <p className="muted">
            {view.retired.length} old cop{view.retired.length === 1 ? "y" : "ies"} SUL no longer manages {view.retired.length === 1 ? "is" : "are"} still on disk. SUL never deletes
            them; that is up to you.
          </p>
        )}

        <h4>Add a build</h4>
        {present.length === 0 ? (
          <p className="muted">No game to add yet. Scan for games above first: a build is made from a game the library found.</p>
        ) : (
          <form className="row" aria-label="Add a build" onSubmit={(e) => e.preventDefault()}>
            <label className="field">
              Game
              <select value={chosenGame?.id ?? ""} onChange={(e) => setGameId(e.target.value)}>
                {present.map((g) => (
                  <option key={g.id} value={g.id}>
                    {g.name}
                  </option>
                ))}
              </select>
            </label>
            <label className="field">
              Slot
              <select value={variantKind} onChange={(e) => setVariantKind(e.target.value)}>
                {BUILTIN_VARIANTS.map((v) => (
                  <option key={v} value={v}>
                    {variantLabel(v)}
                  </option>
                ))}
                <option value="custom">Custom name…</option>
              </select>
            </label>
            {variantKind === "custom" && (
              <label className="field">
                Custom name
                <input type="text" value={customName} onChange={(e) => setCustomName(e.target.value)} />
              </label>
            )}
            <label className="field">
              Game version
              <input type="text" value={version} onChange={(e) => setVersion(e.target.value)} />
            </label>
            <label className="field">
              Where it came from
              <input type="text" value={description} onChange={(e) => setDescription(e.target.value)} placeholder="my own install" />
            </label>
            <label className="field">
              Copy into
              <select value={chosenRoot?.id ?? ""} onChange={(e) => setRootId(e.target.value)}>
                {buildRoots.length === 0 && <option value="">(add a library folder for builds first)</option>}
                {buildRoots.map((r) => (
                  <option key={r.id} value={r.id}>
                    {r.label}
                  </option>
                ))}
              </select>
            </label>
            <button type="button" className="btn" disabled={busy} onClick={() => ask("import")}>
              Copy into a library folder…
            </button>
            <button type="button" className="btn btn-ghost" disabled={busy} onClick={() => ask("register")}>
              Record in place…
            </button>
          </form>
        )}
      </div>

      {confirm && (
        <Dialog
          title={confirm.kind === "import" ? "Copy this game into a library folder?" : confirm.kind === "register" ? "Record this install where it is?" : "Stop listing this folder?"}
          onClose={() => setConfirm(null)}
          actions={
            <>
              <button type="button" className="btn btn-ghost" onClick={() => setConfirm(null)}>
                Cancel
              </button>
              <button type="button" className="btn" onClick={() => void sure()}>
                {confirm.kind === "import" ? "Copy" : confirm.kind === "register" ? "Record" : "Stop listing"}
              </button>
            </>
          }
        >
          {confirm.kind === "import" && (
            <p>
              SUL will copy every file of {confirm.game.name} from <code>{confirm.game.install_dir}</code> into “{confirm.root.label}” (<code>{confirm.root.path}</code>,{" "}
              {formatBytes(confirm.root.free_bytes)} free). Your original install is only read and is never changed. A large game can take a long time, and the copy
              cannot be cancelled once it starts. The copy is checked against the original afterwards.
            </p>
          )}
          {confirm.kind === "register" && (
            <p>
              SUL will remember {confirm.game.name} at <code>{confirm.game.install_dir}</code> as a read-only baseline. It copies nothing and writes nothing there.
            </p>
          )}
          {confirm.kind === "forget" && (
            <p>
              SUL will stop listing “{confirm.root.label}”. The folder and everything in it stay exactly as they are. SUL refuses if a build is stored there.
            </p>
          )}
        </Dialog>
      )}
    </section>
  );
}
