import { CompatibilityMatrixOverlay } from "./CompatibilityMatrixOverlay";
import { StatusBadge } from "./StatusBadge";
import {
  SOURCE_LABEL,
  pendingKey,
  playBlockedReason,
  type GameView,
  type LibraryActions,
  type LibraryState,
} from "../lib/library";

export interface GameLibraryGridProps {
  state: LibraryState;
  actions: LibraryActions;
}

/**
 * The scanned games side by side. Each card has a Play button (Steam games only for now) and a
 * Compatibility button that opens the game's matrix inside the card.
 */
export function GameLibraryGrid({ state, actions }: GameLibraryGridProps) {
  if (state.games.length === 0) return null;
  return (
    <section className="game-library-grid" aria-label="Your games">
      {state.games.map((game) => (
        <GameCard key={game.id} game={game} state={state} actions={actions} />
      ))}
    </section>
  );
}

function GameCard({ game, state, actions }: { game: GameView; state: LibraryState; actions: LibraryActions }) {
  const open = state.expanded[game.id] === true;
  const matrixId = `matrix-${game.id}`;
  const blocked = playBlockedReason(game);
  const playing = state.pending[pendingKey(game.id, "play")] === true;
  const error = state.gameErrors[game.id];
  const notice = state.notices[game.id];

  return (
    <article className={`card game-card${game.present ? "" : " game-card-missing"}`} aria-labelledby={`game-${game.id}`}>
      <header className="game-card-head">
        <h3 id={`game-${game.id}`}>{game.name}</h3>
        <StatusBadge status={game.headline} />
      </header>
      <p className="game-meta">
        <span className="tag">{SOURCE_LABEL[game.source]}</span>
        {game.publisher && <span className="muted">{game.publisher}</span>}
        {!game.present && <span className="tag tag-warn">Not found on the last scan</span>}
      </p>
      <p className="game-path muted" title={game.install_dir}>
        {game.install_dir}
      </p>

      <div className="game-actions">
        <button
          type="button"
          className="btn"
          disabled={blocked !== null || playing}
          aria-describedby={blocked !== null ? `play-hint-${game.id}` : undefined}
          onClick={() => actions.play(game)}
        >
          {playing ? "Starting…" : "Play"}
        </button>
        <button
          type="button"
          className="btn btn-ghost"
          aria-expanded={open}
          aria-controls={matrixId}
          onClick={() => actions.toggleExpanded(game.id)}
        >
          {open ? "Hide compatibility" : "Compatibility"}
        </button>
      </div>
      {blocked !== null && (
        <p className="game-hint muted" id={`play-hint-${game.id}`}>
          {blocked}
        </p>
      )}
      {notice && (
        <p className="game-notice" role="status">
          {notice}
        </p>
      )}
      {error && !open && (
        <p className="matrix-error" role="alert">
          {error}
        </p>
      )}

      {open && (
        <CompatibilityMatrixOverlay
          id={matrixId}
          gameName={game.name}
          matrix={state.matrices[game.id] ?? null}
          error={error}
          isPending={(feature) => state.pending[pendingKey(game.id, feature)] === true}
          onToggle={(feature, enabled) => actions.setTick(game.id, feature, enabled)}
        />
      )}
    </article>
  );
}
