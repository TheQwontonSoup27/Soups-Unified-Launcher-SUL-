import { useEffect, useMemo, useState, useSyncExternalStore } from "react";
import { StatusBadge } from "./StatusBadge";
import type { GameView } from "../lib/library";
import { BUILD_STATE_LABEL } from "../lib/manager";
import {
  MAX_HISTORY_SHOWN,
  PlayController,
  describeLaunchState,
  describeRecord,
  formatUtc,
  holdReason,
  tauriPlayApi,
  type LaunchRecord,
  type LaunchView,
  type PlayApi,
  type PlayBuild,
} from "../lib/playing";

/** How often a running game is asked whether it has ended. */
export const POLL_MS = 2000;

const nameOf = (games: GameView[], id: string) => games.find((g) => g.id === id)?.name ?? id;

/**
 * Play a build SUL manages (spec 1.4). `api` is the backend to use; `undefined` means the real one and `null` means
 * none (a plain browser, or a test). `games` are the games the library lists, for their names.
 *
 * Starting is one press, with nothing to confirm: it only starts the program the game's adapter names inside a build
 * that was checked, and it changes no file. The core refuses everything else and its sentence is shown as is. The
 * Steam Play button on each game card is a separate thing and is not changed by this panel.
 */
export function PlayPanel({ api, games, pollMs = POLL_MS }: { api?: PlayApi | null; games: GameView[]; pollMs?: number }) {
  const controller = useMemo(() => new PlayController(api === undefined ? tauriPlayApi() : api), [api]);
  const state = useSyncExternalStore(controller.subscribe, controller.getSnapshot, controller.getSnapshot);
  const view = state.view;
  const busy = state.busy !== null;
  const running = view?.games.some((g) => g.state.state === "running") ?? false;
  const [capture, setCapture] = useState(false);

  useEffect(() => {
    void controller.load();
  }, [controller]);

  // While a game runs, ask now and then whether it has ended.
  useEffect(() => {
    if (!running) return;
    const timer = setInterval(() => void controller.pollRunning(), pollMs);
    return () => clearInterval(timer);
  }, [controller, running, pollMs]);

  if (!controller.hasBackend) {
    return (
      <section className="card managed" aria-label="Play a managed build">
        <header className="game-card-head">
          <h3>Play a managed build</h3>
          <StatusBadge status="experimental" />
        </header>
        <p className="muted">Browser preview: starting a build needs the desktop app, so nothing is listed here.</p>
      </section>
    );
  }

  return (
    <section className="managed" aria-label="Play a managed build">
      <div className="card">
        <header className="game-card-head">
          <h3>Play a managed build</h3>
          <StatusBadge status="experimental" />
        </header>
        <p className="muted">
          Experimental: tested on made-up programs only. SUL starts a build only when it is ready, through its game&apos;s adapter, and only the program that adapter names inside the
          build. No adapter for a real game exists yet, so Play is refused for every real game today. The game-file risk checks (integrity and anti-cheat signals) are not connected to
          launching yet, and SUL cannot stop a program it started. The Play button on a game card (Steam) is separate and unchanged.
        </p>
        <label className="field">
          <span>
            <input type="checkbox" checked={capture} onChange={(e) => setCapture(e.target.checked)} /> Keep the program&apos;s output in a log file for the next start (off by default;
            capped, with secrets masked)
          </span>
        </label>
        <p className="muted" role="status" aria-live="polite">
          {busy ? "Starting…" : (state.notice ?? "")}
        </p>
        {state.error && (
          <p className="matrix-error" role="alert">
            {state.error}
          </p>
        )}
        {state.warnings.length > 0 && (
          <ul className="muted" aria-label="Warnings about this start">
            {state.warnings.map((w, i) => (
              <li key={i}>{w}</li>
            ))}
          </ul>
        )}
      </div>

      {view && <Builds controller={controller} view={view} games={games} busy={busy} capture={capture} />}
      {view && <History records={state.finished} view={view} games={games} />}
    </section>
  );
}

function Builds({ controller, view, games, busy, capture }: { controller: PlayController; view: LaunchView; games: GameView[]; busy: boolean; capture: boolean }) {
  const gameIds = [...new Set(view.builds.map((b) => b.game_id))];
  return (
    <div className="card" aria-label="Builds you can start">
      <h3>Builds you can start</h3>
      {gameIds.length === 0 && <p className="muted">No builds yet. Add one from the library folders and builds list above.</p>}
      {gameIds.map((id) => {
        const state = view.games.find((g) => g.game_id === id)?.state;
        return (
          <div key={id} aria-label={`Builds of ${nameOf(games, id)}`}>
            <h4>{nameOf(games, id)}</h4>
            {state && <p className="muted">{describeLaunchState(state)}</p>}
            <ul className="trust-list">
              {view.builds
                .filter((b) => b.game_id === id)
                .map((b) => (
                  <BuildRow key={b.build_id} b={b} view={view} busy={busy} capture={capture} controller={controller} />
                ))}
            </ul>
          </div>
        );
      })}
    </div>
  );
}

function BuildRow({ b, view, busy, capture, controller }: { b: PlayBuild; view: LaunchView; busy: boolean; capture: boolean; controller: PlayController }) {
  const held = holdReason(b, view);
  return (
    <li aria-label={`Build ${b.build_id}`}>
      <span className="trust-title">{b.build_id}</span> <span className="tag">{b.managed ? "managed copy" : "install stays in place"}</span>{" "}
      {b.active && <span className="tag">active</span>} <span className="tag">{BUILD_STATE_LABEL[b.state]}</span>
      {held !== null && <p className="muted">{held}</p>}
      <div className="row">
        <button type="button" className="btn" disabled={busy || held !== null} aria-label={`Play build ${b.build_id}`} onClick={() => void controller.start(b.build_id, capture)}>
          Play
        </button>
      </div>
    </li>
  );
}

function History({ records, view, games }: { records: LaunchRecord[]; view: LaunchView; games: GameView[] }) {
  const shown = view.history.slice(0, MAX_HISTORY_SHOWN);
  return (
    <div className="card" aria-label="Launch history">
      <h3>Launch history</h3>
      {shown.length === 0 && <p className="muted">Nothing has been started or refused since SUL opened.</p>}
      <ul aria-label="Launches, newest first">
        {shown.map((r) => (
          <li key={r.launch_id} aria-label={`Launch ${r.launch_id}`}>
            <strong>{nameOf(games, r.game_id)}</strong> · {formatUtc(r.started_unix)} · {describeRecord(r)}
            {r.log_file !== null && (
              <>
                {" "}
                Output kept in <code>{r.log_file}</code>.
              </>
            )}
            {r.warnings.length > 0 && (
              <ul className="muted" aria-label="Warnings">
                {r.warnings.map((w, i) => (
                  <li key={i}>{w}</li>
                ))}
              </ul>
            )}
          </li>
        ))}
      </ul>
      {view.history.length > shown.length && <p className="muted">…and {view.history.length - shown.length} older launches.</p>}
      {records.length > 0 && <p className="muted">{records.length === 1 ? "1 launch ended" : `${records.length} launches ended`} while this page was open.</p>}
    </div>
  );
}
