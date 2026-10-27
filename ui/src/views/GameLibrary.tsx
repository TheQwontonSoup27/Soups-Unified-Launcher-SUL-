import { useEffect, useMemo, useSyncExternalStore } from "react";
import { MOCK_CAPABILITIES, STATUS_HELP, STATUS_ORDER, weakestStatus } from "../capabilities";
import { GameLibraryGrid } from "../components/GameLibraryGrid";
import { PageHeader } from "../components/PageHeader";
import { StatusBadge } from "../components/StatusBadge";
import { LibraryController, tauriLibraryApi, type LibraryApi } from "../lib/library";
import type { Status } from "../types";

const PLANNED_GAMES = [...new Set(MOCK_CAPABILITIES.map((c) => c.game))];

/** A game's headline status is the weakest status among its capabilities (never overclaim). */
function headline(game: string): Status {
  return weakestStatus(MOCK_CAPABILITIES.filter((c) => c.game === game).map((c) => c.status));
}

/**
 * `api` is the backend to use; `undefined` means the real one, `null` means none (a plain browser,
 * or a test), in which case the page explains that scanning needs the desktop app.
 */
export default function GameLibrary({ api }: { api?: LibraryApi | null }) {
  const controller = useMemo(() => new LibraryController(api === undefined ? tauriLibraryApi() : api), [api]);
  const state = useSyncExternalStore(controller.subscribe, controller.getSnapshot, controller.getSnapshot);

  useEffect(() => {
    void controller.load();
  }, [controller]);

  return (
    <>
      <PageHeader title="Game Library" subtitle="One interface; game-specific capabilities are clearly labelled." />
      <section className="hero card">
        <p className="eyebrow">Your library, builds and mods</p>
        <h2>Keep every game version under control.</h2>
        <p className="muted">Original installs stay untouched. Changes are isolated, logged and reversible.</p>
        <p className="muted">Emulated game profiles and the news feed are not built yet.</p>
      </section>

      <section className="card library-toolbar" aria-label="Scan">
        <div>
          <h3>Installed games</h3>
          <p className="muted" role="status">
            {!controller.hasBackend
              ? "Browser preview: scanning needs the desktop app, so no games are listed."
              : state.scanning
                ? "Scanning the registry, Steam libraries and drives…"
                : (state.scanSummary ??
                  (state.loaded && state.games.length === 0
                    ? "No games yet. Scan to find the ones installed on this computer."
                    : "Scans only read: nothing is installed, moved or changed."))}
          </p>
        </div>
        <button
          type="button"
          className="btn"
          disabled={!controller.hasBackend || state.scanning}
          aria-busy={state.scanning}
          onClick={() => void controller.scan()}
        >
          {state.scanning ? "Scanning…" : "Scan for games"}
        </button>
      </section>
      {state.error && (
        <p className="card matrix-error" role="alert">
          {state.error}
        </p>
      )}

      <GameLibraryGrid state={state} actions={controller} />

      <section className="card">
        <h3>Planned game adapters</h3>
        <p className="muted">Development data: nothing has been tested yet.</p>
        <div className="game-grid">
          {PLANNED_GAMES.map((g) => (
            <article className="card game" key={g}>
              <h3>{g}</h3>
              <StatusBadge status={headline(g)} />
            </article>
          ))}
        </div>
      </section>

      <section className="card">
        <h3>Capability status</h3>
        <dl className="legend">
          {STATUS_ORDER.map((s) => (
            <div key={s}>
              <dt>
                <StatusBadge status={s} />
              </dt>
              <dd>{STATUS_HELP[s]}</dd>
            </div>
          ))}
        </dl>
      </section>
    </>
  );
}
