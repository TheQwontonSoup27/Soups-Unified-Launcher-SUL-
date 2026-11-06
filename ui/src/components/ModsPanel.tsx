import { useEffect, useMemo, useState, useSyncExternalStore } from "react";
import { Dialog } from "./Dialog";
import { StatusBadge } from "./StatusBadge";
import type { GameView } from "../lib/library";
import { slugId } from "../lib/manager";
import {
  ModsController,
  PURGE_ACTION_LABEL,
  buildModsOf,
  canDeployGame,
  errorsOf,
  formatSize,
  latestOfEachMod,
  leavesFile,
  modKey,
  parseRequires,
  parseVersions,
  describeVerify,
  tauriModsApi,
  versionsOf,
  warningsOf,
  type BuildModsView,
  type ModView,
  type ModsApi,
  type ModsView,
  type Preview,
  type Problem,
  type ProfileView,
  type VerifyReport,
} from "../lib/mods";

/** Rows of a preview shown before "and N more"; the core still does every file. */
const SHOWN_ROWS = 200;

const nameOf = (games: GameView[], id: string) => games.find((g) => g.id === id)?.name ?? id;

function Problems({ problems }: { problems: Problem[] }) {
  const errors = errorsOf(problems);
  const warnings = warningsOf(problems);
  if (errors.length === 0 && warnings.length === 0) return null;
  return (
    <>
      {errors.length > 0 && (
        <ul className="matrix-error" role="alert" aria-label="Problems that block a deploy">
          {errors.map((p, i) => (
            <li key={i}>{p.message}</li>
          ))}
        </ul>
      )}
      {warnings.length > 0 && (
        <ul className="muted" aria-label="Things to check before a deploy">
          {warnings.map((p, i) => (
            <li key={i}>{p.message}</li>
          ))}
        </ul>
      )}
    </>
  );
}

/**
 * Mods and profiles (spec 2.1). `api` is the backend to use; `undefined` means the real one and `null` means none (a
 * plain browser, or a test). `games` are the games the library lists, for their names and to choose a game.
 *
 * Deploy, purge and undo show what they would do and wait for a confirmation. The question is not the safety check:
 * the core plans again when it runs, and refuses anything unsafe whatever this page sends.
 */
export function ModsPanel({ api, games }: { api?: ModsApi | null; games: GameView[] }) {
  const controller = useMemo(() => new ModsController(api === undefined ? tauriModsApi() : api), [api]);
  const state = useSyncExternalStore(controller.subscribe, controller.getSnapshot, controller.getSnapshot);
  const view = state.view;
  const busy = state.busy !== null;

  useEffect(() => {
    void controller.load();
  }, [controller]);

  if (!controller.hasBackend) {
    return (
      <section className="card managed" aria-label="Mods and profiles">
        <header className="game-card-head">
          <h3>Mods and profiles</h3>
          <StatusBadge status="experimental" />
        </header>
        <p className="muted">Browser preview: mods and profiles need the desktop app, so nothing is listed here.</p>
      </section>
    );
  }

  const unfinished = (view?.builds ?? []).filter((b) => b.deployment?.status === "in_progress");

  return (
    <section className="managed mods" aria-label="Mods and profiles">
      <div className="card">
        <header className="game-card-head">
          <h3>Mods and profiles</h3>
          <StatusBadge status="experimental" />
        </header>
        <p className="muted">
          Experimental: tested on made-up folders only. SUL deploys mods only for a game that has an adapter, and no adapter for a real game exists yet, so Deploy is
          refused for every game today. Nothing here has been tried on a real game or with a game&apos;s anti-cheat.
        </p>
        <p className="muted" role="status" aria-live="polite">
          {busy ? "Working… this can take a while for a large mod, and it cannot be cancelled once started." : (state.notice ?? "")}
        </p>
        {state.error && (
          <p className="matrix-error" role="alert">
            {state.error}
          </p>
        )}
        {unfinished.length > 0 && (
          <div className="matrix-error" role="alert" aria-label="Unfinished deploys">
            {unfinished.map((b) => (
              <p key={b.build_id}>
                A deploy into <code>{b.build_id}</code> did not finish, so some of its files may be in place. Preview a purge for that build: it undoes whatever was done.
              </p>
            ))}
          </div>
        )}
      </div>

      {view && (
        <>
          <ModsList controller={controller} view={view} games={games} busy={busy} checks={state.checks} />
          <ProfilesList controller={controller} view={view} games={games} busy={busy} />
          <DeployedBuilds controller={controller} view={view} games={games} busy={busy} />
        </>
      )}

      {state.preview && <PreviewDialog preview={state.preview} view={view} controller={controller} />}
    </section>
  );
}

// ---------------------------------------------------------------------------
// Mods
// ---------------------------------------------------------------------------

function ModsList({ controller, view, games, busy, checks }: { controller: ModsController; view: ModsView; games: GameView[]; busy: boolean; checks: Record<string, VerifyReport> }) {
  const gameIds = [...new Set([...games.map((g) => g.id), ...view.builds.map((b) => b.game_id), ...view.mods.map((m) => m.game_id)])];
  const [gameId, setGameId] = useState("");
  const [name, setName] = useState("");
  const [version, setVersion] = useState("");
  const [path, setPath] = useState("");
  const [source, setSource] = useState("");
  const [licence, setLicence] = useState("");
  const [requires, setRequires] = useState("");
  const [versions, setVersions] = useState("");
  const [rootId, setRootId] = useState("");
  const [problem, setProblem] = useState<string | null>(null);
  const [asking, setAsking] = useState<null | { src: Parameters<ModsController["importMod"]>[0]; rootId: string }>(null);

  const game = gameIds.includes(gameId) ? gameId : (gameIds[0] ?? "");
  const root = view.mod_roots.find((r) => r.id === rootId) ?? view.mod_roots[0];

  const ask = () => {
    setProblem(null);
    const req = parseRequires(requires);
    if ("bad" in req) return setProblem(`“${req.bad}” is not a mod id. Write ids separated by commas, each optionally followed by >= and a version.`);
    if (game === "") return setProblem("Pick a game first. Scan for games on the Game Library page if the list is empty.");
    if (!root) return setProblem("Add a library folder for mods first, on the Game Library page.");
    setAsking({
      src: {
        game_id: game,
        mod_id: slugId(name),
        name: name.trim(),
        version: version.trim(),
        requires: req.requires,
        compatible_versions: parseVersions(versions),
        source: source.trim(),
        license_note: licence.trim(),
        path: path.trim(),
      },
      rootId: root.id,
    });
  };

  return (
    <div className="card" aria-label="Mods">
      <h3>Mods</h3>
      <p className="muted">
        A mod is a folder of files. Importing copies it into your mod library and checks the copy; the folder it came from is only read. The notes you write about
        a mod (where it came from, its licence, the game versions it works with) are your words: SUL does not check them.
      </p>
      {view.mods.length === 0 && <p className="muted">No mods yet.</p>}
      <ul className="trust-list">
        {view.mods.map((m) => (
          <ModRow key={modKey(m)} m={m} games={games} busy={busy} check={checks[modKey(m)]} controller={controller} />
        ))}
      </ul>

      <h4>Add a mod</h4>
      <form className="row" aria-label="Add a mod" onSubmit={(e) => e.preventDefault()}>
        <label className="field">
          Game
          <select value={game} onChange={(e) => setGameId(e.target.value)}>
            {gameIds.length === 0 && <option value="">(no game yet)</option>}
            {gameIds.map((id) => (
              <option key={id} value={id}>
                {nameOf(games, id)}
              </option>
            ))}
          </select>
        </label>
        <label className="field">
          Mod name
          <input type="text" value={name} onChange={(e) => setName(e.target.value)} />
        </label>
        <label className="field">
          Version
          <input type="text" value={version} onChange={(e) => setVersion(e.target.value)} />
        </label>
        <label className="field">
          Folder with the mod&apos;s files
          <input type="text" value={path} onChange={(e) => setPath(e.target.value)} />
        </label>
        <label className="field">
          Where it came from
          <input type="text" value={source} onChange={(e) => setSource(e.target.value)} />
        </label>
        <label className="field">
          Licence or permission
          <input type="text" value={licence} onChange={(e) => setLicence(e.target.value)} />
        </label>
        <label className="field">
          Needs these mods (ids)
          <input type="text" value={requires} onChange={(e) => setRequires(e.target.value)} />
        </label>
        <label className="field">
          Works with game versions
          <input type="text" value={versions} onChange={(e) => setVersions(e.target.value)} />
        </label>
        <label className="field">
          Copy into
          <select value={root?.id ?? ""} onChange={(e) => setRootId(e.target.value)}>
            {view.mod_roots.length === 0 && <option value="">(add a library folder for mods first)</option>}
            {view.mod_roots.map((r) => (
              <option key={r.id} value={r.id}>
                {r.label}
              </option>
            ))}
          </select>
        </label>
        <button type="button" className="btn" disabled={busy} onClick={ask}>
          Copy into the mod library…
        </button>
      </form>
      {problem && (
        <p className="matrix-error" role="alert">
          {problem}
        </p>
      )}

      {asking && (
        <Dialog
          title="Copy this mod into the mod library?"
          onClose={() => setAsking(null)}
          actions={
            <>
              <button type="button" className="btn btn-ghost" onClick={() => setAsking(null)}>
                Cancel
              </button>
              <button
                type="button"
                className="btn"
                onClick={() => {
                  const a = asking;
                  setAsking(null);
                  void controller.importMod(a.src, a.rootId).then((ok) => ok && (setName(""), setVersion(""), setPath(""), setSource(""), setLicence(""), setRequires(""), setVersions("")));
                }}
              >
                Copy
              </button>
            </>
          }
        >
          <p>
            SUL will copy every file in <code>{asking.src.path}</code> into the mod library “{view.mod_roots.find((r) => r.id === asking.rootId)?.label}” as {asking.src.name}{" "}
            {asking.src.version} for {nameOf(games, asking.src.game_id)}, and check the copy against the folder. The folder is only read. Nothing is deployed to a game.
          </p>
        </Dialog>
      )}
    </div>
  );
}

function ModRow({ m, games, busy, check, controller }: { m: ModView; games: GameView[]; busy: boolean; check: VerifyReport | undefined; controller: ModsController }) {
  return (
    <li>
      <span className="trust-title">{m.name}</span> <span className="tag">{m.version}</span>
      <p className="muted">
        <code>{m.mod_id}</code> · {nameOf(games, m.game_id)} · {m.file_count} {m.file_count === 1 ? "file" : "files"}, {formatSize(m.total_bytes)}
      </p>
      <p className="muted">
        {m.compatible_versions.length > 0 ? `Said to work with game version ${m.compatible_versions.join(", ")}` : "Nobody said which game versions it works with, so SUL does not assume any"}
        {m.requires.length > 0 && ` · Needs ${m.requires.map((r) => (r.min_version ? `${r.mod_id} ${r.min_version} or newer` : r.mod_id)).join(", ")}`}
      </p>
      {(m.source !== "" || m.license_note !== "") && (
        <p className="muted">
          {m.source !== "" && <>From: {m.source}. </>}
          {m.license_note !== "" && <>Licence note (your words, not checked): {m.license_note}</>}
        </p>
      )}
      {check && <p className="muted">Last check: {describeVerify(check)}</p>}
      <div className="row">
        <button type="button" className="btn btn-ghost" disabled={busy} aria-label={`Check the files of ${m.name} ${m.version}`} onClick={() => void controller.verifyMod(m)}>
          Check files
        </button>
      </div>
    </li>
  );
}

// ---------------------------------------------------------------------------
// Profiles
// ---------------------------------------------------------------------------

function ProfilesList({ controller, view, games, busy }: { controller: ModsController; view: ModsView; games: GameView[]; busy: boolean }) {
  // A profile is made for a build SUL manages as a copy; the original and installs that stay put take no mods.
  const gameIds = [...new Set(view.builds.map((b) => b.game_id))];
  const [name, setName] = useState("");
  const [gameId, setGameId] = useState("");
  const [buildId, setBuildId] = useState("");
  const game = gameIds.includes(gameId) ? gameId : (gameIds[0] ?? "");
  const builds = view.builds.filter((b) => b.game_id === game);
  const build = builds.find((b) => b.build_id === buildId) ?? builds[0];

  return (
    <div className="card" aria-label="Profiles">
      <h3>Profiles</h3>
      <p className="muted">
        A profile is a named, ordered list of mods for one game and one managed build. A later mod in the list wins when two provide the same file. Editing a profile
        changes only SUL&apos;s own profile file; nothing reaches a game until you deploy.
      </p>
      {view.profiles.length === 0 && <p className="muted">No profiles yet.</p>}
      {view.profiles.map((p) => (
        <ProfileCard key={p.id} p={p} view={view} games={games} busy={busy} controller={controller} />
      ))}

      <h4>Make a profile</h4>
      {gameIds.length === 0 ? (
        <p className="muted">A profile needs a build SUL manages as a copy of a game. Add one on the Game Library page first.</p>
      ) : (
        <form
          className="row"
          aria-label="Make a profile"
          onSubmit={(e) => {
            e.preventDefault();
            void controller.createProfile(name, game, build?.build_id ?? "").then((ok) => ok && setName(""));
          }}
        >
          <label className="field">
            Name
            <input type="text" value={name} onChange={(e) => setName(e.target.value)} />
          </label>
          <label className="field">
            Game
            <select value={game} onChange={(e) => setGameId(e.target.value)}>
              {gameIds.map((id) => (
                <option key={id} value={id}>
                  {nameOf(games, id)}
                </option>
              ))}
            </select>
          </label>
          <label className="field">
            Build
            <select value={build?.build_id ?? ""} onChange={(e) => setBuildId(e.target.value)}>
              {builds.map((b) => (
                <option key={b.build_id} value={b.build_id}>
                  {b.slot === "original" ? `${b.slot} (takes no mods)` : b.slot} · {b.build_id}
                </option>
              ))}
            </select>
          </label>
          <button type="submit" className="btn" disabled={busy}>
            Make profile
          </button>
        </form>
      )}
    </div>
  );
}

function ProfileCard({ p, view, games, busy, controller }: { p: ProfileView; view: ModsView; games: GameView[]; busy: boolean; controller: ModsController }) {
  const edit = (e: Parameters<ModsController["editProfile"]>[1]) => controller.editProfile(p.id, e);
  const [name, setName] = useState(p.name);
  const [notes, setNotes] = useState(p.notes);
  const [adding, setAdding] = useState("");
  const [key, setKey] = useState("");
  const [value, setValue] = useState("");
  const [moving, setMoving] = useState("");
  const build = buildModsOf(view, p.build_id);
  const deployable = canDeployGame(view, p.game_id);
  const inProfile = new Set(p.mods.map((m) => m.mod_id));
  const addable = latestOfEachMod(view, p.game_id).filter((m) => !inProfile.has(m.mod_id));
  const pick = addable.find((m) => m.mod_id === adding) ?? addable[0];
  const deployedHere = build?.deployment?.profile_id === p.id;
  const otherBuilds = view.builds.filter((b) => b.game_id === p.game_id && b.build_id !== p.build_id);

  return (
    <article className="profile" aria-label={`Profile ${p.name}`}>
      <header className="game-card-head">
        <h4>{p.name}</h4>
        {deployedHere && <span className="tag">Deployed</span>}
      </header>
      <p className="muted">
        {nameOf(games, p.game_id)} · build {build ? `${build.slot} ` : ""}
        <code>{p.build_id}</code>
      </p>
      <Problems problems={p.problems} />

      {p.mods.length === 0 ? (
        <p className="muted">No mods in this profile yet.</p>
      ) : (
        <ol className="trust-list" aria-label={`Load order of ${p.name}`}>
          {p.mods.map((m, i) => {
            const known = view.mods.find((x) => x.game_id === p.game_id && x.mod_id === m.mod_id && x.version === m.version);
            const versions = versionsOf(view, p.game_id, m.mod_id);
            return (
              <li key={m.mod_id}>
                <span className="trust-title">{known?.name ?? m.mod_id}</span> <code>{m.mod_id}</code>
                <div className="row">
                  <label className="field">
                    Version
                    <select value={m.version} disabled={busy} onChange={(e) => void edit({ action: "set_version", mod_id: m.mod_id, version: e.target.value })}>
                      {!versions.includes(m.version) && <option value={m.version}>{m.version} (not in the store)</option>}
                      {versions.map((v) => (
                        <option key={v} value={v}>
                          {v}
                        </option>
                      ))}
                    </select>
                  </label>
                  <label className="field">
                    <span>
                      <input type="checkbox" checked={m.enabled} disabled={busy} onChange={(e) => void edit({ action: "set_enabled", mod_id: m.mod_id, enabled: e.target.checked })} /> Enabled
                    </span>
                  </label>
                  <button type="button" className="btn btn-ghost" disabled={busy || i === 0} aria-label={`Load ${m.mod_id} earlier`} onClick={() => void edit({ action: "move", mod_id: m.mod_id, to_index: i - 1 })}>
                    Earlier
                  </button>
                  <button type="button" className="btn btn-ghost" disabled={busy || i === p.mods.length - 1} aria-label={`Load ${m.mod_id} later`} onClick={() => void edit({ action: "move", mod_id: m.mod_id, to_index: i + 1 })}>
                    Later
                  </button>
                  <button type="button" className="btn btn-ghost" disabled={busy} aria-label={`Take ${m.mod_id} out of ${p.name}`} onClick={() => void edit({ action: "remove_mod", mod_id: m.mod_id })}>
                    Take out
                  </button>
                </div>
              </li>
            );
          })}
        </ol>
      )}

      <form className="row" aria-label={`Add a mod to ${p.name}`} onSubmit={(e) => e.preventDefault()}>
        <label className="field">
          Add a mod
          <select value={pick?.mod_id ?? ""} onChange={(e) => setAdding(e.target.value)}>
            {addable.length === 0 && <option value="">(no other mod for this game)</option>}
            {addable.map((m) => (
              <option key={m.mod_id} value={m.mod_id}>
                {m.name} {m.version}
              </option>
            ))}
          </select>
        </label>
        <button type="button" className="btn btn-ghost" disabled={busy || !pick} onClick={() => pick && void edit({ action: "add_mod", mod_id: pick.mod_id, version: pick.version, position: null })}>
          Add to the end
        </button>
      </form>

      <div className="row">
        <button type="button" className="btn" disabled={busy || !deployable} onClick={() => void controller.previewDeploy(p.id)}>
          Preview deploy…
        </button>
        <button type="button" className="btn btn-ghost" disabled={busy || !deployable || !deployedHere} onClick={() => void controller.previewPurgeAndDeploy(p.id)}>
          Purge &amp; Deploy…
        </button>
      </div>
      {!deployable && <p className="muted">SUL has no adapter for {nameOf(games, p.game_id)}, so it does not know which folders mods may change there, and it deploys nothing without that.</p>}

      <details>
        <summary>Settings, notes, name and build</summary>
        <p className="muted">Settings are kept with the profile and shown here only: nothing passes them to a game yet.</p>
        {Object.entries(p.config).length > 0 && (
          <ul className="trust-list" aria-label={`Settings of ${p.name}`}>
            {Object.entries(p.config).map(([k, v]) => (
              <li key={k}>
                <code>{k}</code> = {v}{" "}
                <button type="button" className="btn btn-ghost" disabled={busy} aria-label={`Remove the setting ${k}`} onClick={() => void edit({ action: "set_config", key: k, value: null })}>
                  Remove
                </button>
              </li>
            ))}
          </ul>
        )}
        <form
          className="row"
          aria-label={`Add a setting to ${p.name}`}
          onSubmit={(e) => {
            e.preventDefault();
            void edit({ action: "set_config", key: key.trim(), value }).then((ok) => ok && (setKey(""), setValue("")));
          }}
        >
          <label className="field">
            Setting
            <input type="text" value={key} onChange={(e) => setKey(e.target.value)} />
          </label>
          <label className="field">
            Value
            <input type="text" value={value} onChange={(e) => setValue(e.target.value)} />
          </label>
          <button type="submit" className="btn btn-ghost" disabled={busy || key.trim() === ""}>
            Save setting
          </button>
        </form>
        <form
          className="row"
          aria-label={`Rename ${p.name}`}
          onSubmit={(e) => {
            e.preventDefault();
            void edit({ action: "rename", name });
          }}
        >
          <label className="field">
            Name
            <input type="text" value={name} onChange={(e) => setName(e.target.value)} />
          </label>
          <button type="submit" className="btn btn-ghost" disabled={busy || name.trim() === "" || name.trim() === p.name}>
            Rename
          </button>
        </form>
        <form
          className="row"
          aria-label={`Notes for ${p.name}`}
          onSubmit={(e) => {
            e.preventDefault();
            void edit({ action: "notes", notes });
          }}
        >
          <label className="field">
            Notes
            <textarea value={notes} rows={3} onChange={(e) => setNotes(e.target.value)} />
          </label>
          <button type="submit" className="btn btn-ghost" disabled={busy || notes === p.notes}>
            Save notes
          </button>
        </form>
        <form className="row" aria-label={`Move ${p.name} to another build`} onSubmit={(e) => e.preventDefault()}>
          <label className="field">
            Another build of this game
            <select value={moving || otherBuilds[0]?.build_id || ""} onChange={(e) => setMoving(e.target.value)}>
              {otherBuilds.length === 0 && <option value="">(no other build)</option>}
              {otherBuilds.map((b) => (
                <option key={b.build_id} value={b.build_id}>
                  {b.slot} · {b.build_id}
                </option>
              ))}
            </select>
          </label>
          <button
            type="button"
            className="btn btn-ghost"
            disabled={busy || otherBuilds.length === 0}
            onClick={() => void edit({ action: "set_build", build_id: moving || otherBuilds[0]!.build_id })}
          >
            Move the profile
          </button>
        </form>
        {deployedHere && <p className="muted">This profile is deployed in its build. Purge it there before moving it to another build; SUL refuses otherwise.</p>}
      </details>
    </article>
  );
}

// ---------------------------------------------------------------------------
// What is deployed, purge and undo
// ---------------------------------------------------------------------------

function DeployedBuilds({ controller, view, games, busy }: { controller: ModsController; view: ModsView; games: GameView[]; busy: boolean }) {
  const interesting = view.builds.filter((b) => b.deployment || b.undoable_purge || b.error || b.history.length > 0);
  return (
    <div className="card" aria-label="Deployed mods">
      <h3>What is deployed</h3>
      <p className="muted">
        A purge removes only what SUL&apos;s record proves it put there, and only if the file is still exactly what was deployed. What it removes goes to a recovery
        folder rather than being deleted, and the latest purge can be undone until something else changes in the build. A purge needs no adapter.
      </p>
      {interesting.length === 0 && <p className="muted">Nothing is deployed in any build.</p>}
      <ul className="trust-list">
        {interesting.map((b) => (
          <BuildRow key={b.build_id} b={b} games={games} busy={busy} controller={controller} />
        ))}
      </ul>
    </div>
  );
}

function BuildRow({ b, games, busy, controller }: { b: BuildModsView; games: GameView[]; busy: boolean; controller: ModsController }) {
  return (
    <li>
      <span className="trust-title">
        {nameOf(games, b.game_id)} · {b.slot}
      </span>{" "}
      <code>{b.build_id}</code>
      {b.error && (
        <p className="matrix-error" role="alert">
          The mod state of this build cannot be read: {b.error}
        </p>
      )}
      {b.deployment ? (
        <p className="muted">
          {b.deployment.status === "in_progress" ? "A deploy of" : "Deployed:"} “{b.deployment.profile_name}” ({b.deployment.files} {b.deployment.files === 1 ? "file" : "files"}
          {b.deployment.status === "in_progress" ? ", not finished" : ""})
        </p>
      ) : (
        !b.error && <p className="muted">Nothing is deployed here.</p>
      )}
      {b.undoable_purge && (
        <p className="muted">
          The latest purge ({b.undoable_purge.files} {b.undoable_purge.files === 1 ? "file" : "files"} of “{b.undoable_purge.profile_name}”) can still be undone.
        </p>
      )}
      <div className="row">
        <button type="button" className="btn btn-ghost" disabled={busy || !b.deployment} aria-label={`Preview a purge of ${b.build_id}`} onClick={() => void controller.previewPurge(b.build_id)}>
          Preview purge…
        </button>
        <button type="button" className="btn btn-ghost" disabled={busy || !b.undoable_purge} aria-label={`Undo the latest purge of ${b.build_id}`} onClick={() => controller.previewUndo(b.build_id)}>
          Undo the latest purge…
        </button>
      </div>
      {b.history.length > 0 && (
        <details>
          <summary>History ({b.history.length})</summary>
          <ul>
            {[...b.history].reverse().map((h) => (
              <li key={h.id} className="muted">
                {new Date(h.unix * 1000).toLocaleString()} · {h.kind === "deploy" ? "Deploy" : h.kind === "purge" ? "Purge" : "Undo of a purge"} · {h.files} {h.files === 1 ? "file" : "files"}
                {h.note !== "" && ` · ${h.note}`}
              </li>
            ))}
          </ul>
        </details>
      )}
    </li>
  );
}

// ---------------------------------------------------------------------------
// The question that comes before anything is changed
// ---------------------------------------------------------------------------

function PreviewDialog({ preview, view, controller }: { preview: Preview; view: ModsView | null; controller: ModsController }) {
  const close = () => controller.closePreview();
  const cancel = (
    <button type="button" className="btn btn-ghost" onClick={close}>
      Cancel
    </button>
  );

  if (preview.kind === "undo") {
    return (
      <Dialog
        title="Undo the latest purge?"
        onClose={close}
        actions={
          <>
            {cancel}
            <button type="button" className="btn" onClick={() => void controller.confirm()}>
              Undo the purge
            </button>
          </>
        }
      >
        <p>
          SUL will put back the {preview.summary.files} {preview.summary.files === 1 ? "file" : "files"} that purge of “{preview.summary.profile_name}” took out. It only does this when every file is
          exactly where the purge left it, and it overwrites nothing: if anything changed it refuses and changes nothing.
        </p>
      </Dialog>
    );
  }

  if (preview.kind === "deploy") {
    const { plan } = preview;
    const profile = view?.profiles.find((p) => p.id === preview.profileId);
    const blockers = errorsOf(plan.problems);
    return (
      <Dialog
        title={`Deploy ${profile ? `“${profile.name}”` : "this profile"}?`}
        onClose={close}
        actions={
          <>
            {cancel}
            <button type="button" className="btn" disabled={blockers.length > 0} onClick={() => void controller.confirm()}>
              Deploy
            </button>
          </>
        }
      >
        {blockers.length > 0 ? <p>This deploy is blocked, so nothing will be changed. What blocks it:</p> : <p>This is exactly what a deploy would do now. Nothing has been changed yet.</p>}
        <Problems problems={plan.problems} />
        {plan.files.length > 0 && (
          <>
            <p>
              {plan.files.length} {plan.files.length === 1 ? "file" : "files"}, {formatSize(plan.total_bytes)}. A file it replaces is moved aside and kept, not deleted.
            </p>
            <ul className="preview-list" aria-label="Files this deploy would change">
              {plan.files.slice(0, SHOWN_ROWS).map((f) => (
                <li key={f.path}>
                  <strong>{f.action === "add" ? "Add" : "Replace"}</strong> <code>{f.path}</code> <span className="muted">from {f.mod_id} {f.mod_version}</span>
                  {f.replaces && !f.replaces.in_build_manifest && <span className="muted"> · the build’s record did not list this file</span>}
                  {f.overrides.length > 0 && <span className="muted"> · wins over {f.overrides.join(", ")}</span>}
                </li>
              ))}
            </ul>
            {plan.files.length > SHOWN_ROWS && <p className="muted">…and {plan.files.length - SHOWN_ROWS} more. The deploy covers all {plan.files.length}.</p>}
          </>
        )}
      </Dialog>
    );
  }

  // purge and purge_and_deploy both show the purge's dry run.
  const { plan } = preview;
  const both = preview.kind === "purge_and_deploy";
  const profile = both ? view?.profiles.find((p) => p.id === preview.profileId) : undefined;
  const blockers = profile ? errorsOf(profile.problems) : [];
  const nothing = plan.deployment === null;
  const left = plan.items.filter((i) => leavesFile(i.action));
  return (
    <Dialog
      title={both ? `Purge, then deploy ${profile ? `“${profile.name}”` : "the profile"}?` : "Purge what is deployed?"}
      onClose={close}
      actions={
        <>
          {cancel}
          <button type="button" className="btn" disabled={(nothing && !both) || blockers.length > 0} onClick={() => void controller.confirm()}>
            {both ? "Purge & Deploy" : "Purge"}
          </button>
        </>
      }
    >
      {nothing ? (
        <p>{both ? "Nothing is deployed in this build, so there is nothing to purge; the profile would be deployed as it is." : "Nothing is deployed in this build."}</p>
      ) : (
        <p>
          This is what a purge of “{plan.deployment?.profile_name}” would do now. Nothing has been changed yet. Files the purge moves go to a recovery folder, and the purge can be
          undone afterwards.
        </p>
      )}
      {both && blockers.length > 0 && (
        <>
          <p>The profile cannot be deployed, so nothing would be purged either:</p>
          <Problems problems={blockers} />
        </>
      )}
      {plan.items.length > 0 && (
        <ul className="preview-list" aria-label="Files this purge would handle">
          {plan.items.slice(0, SHOWN_ROWS).map((i) => (
            <li key={i.path} className={leavesFile(i.action) ? "left-alone" : undefined}>
              <code>{i.path}</code> <span className="muted">({i.mod_id})</span> — {PURGE_ACTION_LABEL[i.action]}
              {i.detail !== "" && <span className="muted"> · {i.detail}</span>}
            </li>
          ))}
        </ul>
      )}
      {plan.items.length > SHOWN_ROWS && <p className="muted">…and {plan.items.length - SHOWN_ROWS} more. The purge covers all {plan.items.length}.</p>}
      {left.length > 0 && (
        <p className="matrix-error" role="alert">
          {left.length} {left.length === 1 ? "file" : "files"} would be left in place, so the build would stay partly modded.{both && " Purge & Deploy would then not deploy."}
        </p>
      )}
      {both && !nothing && <p className="muted">If the purge leaves nothing behind, SUL then plans the deploy again and runs it; the files it would add are not listed here because they depend on the purge.</p>}
    </Dialog>
  );
}
