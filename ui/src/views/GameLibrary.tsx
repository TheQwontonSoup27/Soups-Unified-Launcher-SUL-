import { useEffect, useMemo, useSyncExternalStore } from "react";
import { MOCK_CAPABILITIES, STATUS_HELP, STATUS_ORDER, weakestStatus } from "../capabilities";
import { CategoryChips } from "../components/CategoryChips";
import { GameDetailPanel } from "../components/GameDetailPanel";
import { GameLibraryGrid } from "../components/GameLibraryGrid";
import { PageHeader } from "../components/PageHeader";
import { StatusBadge } from "../components/StatusBadge";
import { ManagedBuildsPanel } from "../components/ManagedBuildsPanel";
import { PlayPanel } from "../components/PlayPanel";
import { CatalogueController, applyCategory, categoriesOf, filterSummary, tauriCatalogueApi, type CatalogueApi } from "../lib/catalogue";
import { LibraryController, filterGames, tauriLibraryApi, type LibraryApi } from "../lib/library";
import type { ManagerApi } from "../lib/manager";
import type { PlayApi } from "../lib/playing";
import type { Status } from "../types";

const PLANNED_GAMES = [...new Set(MOCK_CAPABILITIES.map((c) => c.game))];

/** A game's headline status is the weakest status among its capabilities (never overclaim). */
function headline(game: string): Status {
  return weakestStatus(MOCK_CAPABILITIES.filter((c) => c.game === game).map((c) => c.status));
}

/**
 * `api` is the backend to use; `undefined` means the real one, `null` means none (a plain browser,
 * or a test), in which case the page explains that scanning needs the desktop app. `query` is what is
 * typed in the sidebar's search box: only the games that match it are shown. `managerApi` is the backend of the
 * folders-and-builds panel and `playApi` the backend of the Play panel; when a test hands the page a fake library
 * backend and no manager or play backend, that panel gets none rather than reaching for the real one. `catalogueApi` is the
 * backend of the categories and the game details, treated the same way.
 */
export default function GameLibrary({
  api,
  managerApi,
  playApi,
  catalogueApi,
  query = "",
}: {
  api?: LibraryApi | null;
  managerApi?: ManagerApi | null;
  playApi?: PlayApi | null;
  catalogueApi?: CatalogueApi | null;
  query?: string;
}) {
  const controller = useMemo(() => new LibraryController(api === undefined ? tauriLibraryApi() : api), [api]);
  const state = useSyncExternalStore(controller.subscribe, controller.getSnapshot, controller.getSnapshot);
  const catalogue = useMemo(() => new CatalogueController(catalogueApi === undefined ? (api === undefined ? tauriCatalogueApi() : null) : catalogueApi), [catalogueApi, api]);
  const cat = useSyncExternalStore(catalogue.subscribe, catalogue.getSnapshot, catalogue.getSnapshot);
  const shown = useMemo(() => applyCategory(filterGames(state.games, query), cat.view, cat.selected), [state.games, query, cat.view, cat.selected]);
  const summary = state.games.length > 0 ? filterSummary(query, cat.selected, shown.length, state.games.length) : null;

  useEffect(() => {
    void controller.load();
  }, [controller]);

  // The category lists depend on which games the library has, so ask again when that set changes (a scan, or the first load).
  const gameIds = state.games.map((g) => g.id).join("\n");
  useEffect(() => {
    void catalogue.load();
  }, [catalogue, gameIds]);

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
                : summary !== null
                  ? summary
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

      {cat.error && (
        <p className="card matrix-error" role="alert">
          {cat.error}
        </p>
      )}

      <CategoryChips view={cat.view} selected={cat.selected} total={state.games.length} onSelect={(c) => catalogue.select(c)} />

      <GameLibraryGrid
        state={{ ...state, games: shown }}
        actions={controller}
        catalogue={
          catalogue.hasBackend
            ? {
                categoriesOf: (id) => categoriesOf(cat.view, id),
                openId: cat.openId,
                onToggleDetails: (game) => void catalogue.open(game.id),
                panel: (game) => <GameDetailPanel key={game.id} controller={catalogue} state={cat} name={game.name} />,
              }
            : undefined
        }
      />

      <ManagedBuildsPanel api={managerApi === undefined && api !== undefined ? null : managerApi} games={state.games} />

      <PlayPanel api={playApi === undefined && api !== undefined ? null : playApi} games={state.games} />

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
