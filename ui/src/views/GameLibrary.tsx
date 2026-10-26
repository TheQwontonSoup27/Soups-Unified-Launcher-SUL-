import { MOCK_CAPABILITIES, STATUS_HELP, STATUS_ORDER, weakestStatus } from "../capabilities";
import { PageHeader } from "../components/PageHeader";
import { StatusBadge } from "../components/StatusBadge";
import type { Status } from "../types";

const GAMES = [...new Set(MOCK_CAPABILITIES.map((c) => c.game))];

/** A game's headline status is the weakest status among its capabilities (never overclaim). */
function headline(game: string): Status {
  return weakestStatus(MOCK_CAPABILITIES.filter((c) => c.game === game).map((c) => c.status));
}

export default function GameLibrary() {
  return (
    <>
      <PageHeader title="Game Library" subtitle="One interface; game-specific capabilities are clearly labelled." />
      <section className="hero card">
        <p className="eyebrow">Your library, builds and mods</p>
        <h2>Keep every game version under control.</h2>
        <p className="muted">Original installs stay untouched. Changes are isolated, logged and reversible.</p>
        <p className="muted">The library grid, emulated game profiles and the news feed are not built yet.</p>
      </section>
      <section aria-label="Games under investigation" className="game-grid">
        {GAMES.map((g) => (
          <article className="card game" key={g}>
            <h3>{g}</h3>
            <StatusBadge status={headline(g)} />
          </article>
        ))}
      </section>
      <section className="card">
        <h3>Capability status</h3>
        <p className="muted">Development data: nothing has been tested yet. The scanner's real results are not shown here yet.</p>
        <dl className="legend">
          {STATUS_ORDER.map((s) => (
            <div key={s}>
              <dt><StatusBadge status={s} /></dt>
              <dd>{STATUS_HELP[s]}</dd>
            </div>
          ))}
        </dl>
      </section>
    </>
  );
}
