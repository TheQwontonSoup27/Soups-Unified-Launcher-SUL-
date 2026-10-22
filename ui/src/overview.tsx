import { MOCK_CAPABILITIES, STATUS_HELP, STATUS_LABEL } from "./capabilities";
import { PageHeader, StatusBadge } from "./components";
import type { Status } from "./types";

const GAMES = ["Helldivers 2", "Terraria", "RimWorld", "Stardew Valley"];
const ORDER: Status[] = ["supported_tested", "partial", "experimental", "unsupported", "not_investigated"];

/** A game's headline status is the weakest status among its capabilities (never overclaim). */
function headline(game: string): Status {
  const rank: Record<Status, number> = { unsupported: 0, not_investigated: 1, experimental: 2, partial: 3, supported_tested: 4 };
  const own = MOCK_CAPABILITIES.filter((c) => c.game === game);
  if (own.length === 0) return "not_investigated";
  return own.reduce((a, c) => (rank[c.status] < rank[a] ? c.status : a), own[0]!.status);
}

export function Overview({ onOpen }: { onOpen: (id: string) => void }) {
  return (
    <>
      <PageHeader title="Overview" subtitle="One interface; game-specific capabilities are clearly labelled." />
      <section className="hero card">
        <p className="eyebrow">Your library, builds and mods</p>
        <h2>Keep every game version under control.</h2>
        <p className="muted">Original installs stay untouched. Changes are isolated, logged and reversible.</p>
      </section>
      <section className="game-grid">
        {GAMES.map((g) => (
          <article className="card game" key={g}>
            <h3>{g}</h3>
            <StatusBadge status={headline(g)} />
            <button className="primary" onClick={() => onOpen("library")}>Open game workspace</button>
          </article>
        ))}
      </section>
      <section className="card">
        <h3>Capability status</h3>
        <p className="muted">Development data: nothing has been tested yet.</p>
        <dl className="legend">
          {ORDER.map((s) => (
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

export { STATUS_LABEL };
