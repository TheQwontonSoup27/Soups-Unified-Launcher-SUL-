import { useId } from "react";
import { StatusBadge } from "../components/StatusBadge";
import { usePresence } from "../presence";
import { ALL_ROWS, CHECKED_ON, DATA_SHARING, CREDENTIALS, PROTECTIONS, RULE_TAG, SENDERS, headlineStatus, type TrustRow, type TrustRule } from "../security";

function Rule({ rule }: { rule: TrustRule }) {
  return (
    <>
      <p>{rule.statement}</p>
      <p className="muted">
        <strong>How this was checked.</strong> {rule.evidence}
      </p>
    </>
  );
}

function Row({ row, extra }: { row: TrustRow; extra?: string }) {
  return (
    <li>
      <span className="trust-title">{row.title}</span> <StatusBadge status={row.status} />
      <p>{row.text}</p>
      {extra && <p><strong>{extra}</strong></p>}
      {row.limitation && <p className="muted">Limit: {row.limitation}</p>}
    </li>
  );
}

/**
 * Settings > Security & Trust: a read-only report. It shows what SUL does by design, what can send data off this PC,
 * and which protections exist (none yet). The one live fact is whether Discord presence is on right now. It has no
 * controls: presence is switched under Settings > Discord.
 */
export function SecurityPanel() {
  const uid = useId();
  const presence = usePresence();
  const headline = headlineStatus();
  return (
    <>
      <section className="card" aria-labelledby={`${uid}-overall`}>
        <header className="game-card-head">
          <h3 id={`${uid}-overall`}>Security &amp; Trust</h3>
          <StatusBadge status={headline} />
        </header>
        <p>
          This page only reports. It has no switches and changes nothing. The label above is the weakest label on the page, so it can never say
          more than the rows below do ({ALL_ROWS.length} rows; checked against the code on {CHECKED_ON}).
        </p>
      </section>

      <section className="card" aria-labelledby={`${uid}-credentials`}>
        <header className="game-card-head">
          <h3 id={`${uid}-credentials`}>{CREDENTIALS.title}</h3>
          <span className="tag">{RULE_TAG}</span>
        </header>
        <Rule rule={CREDENTIALS} />
      </section>

      <section className="card" aria-labelledby={`${uid}-sharing`}>
        <header className="game-card-head">
          <h3 id={`${uid}-sharing`}>{DATA_SHARING.title}</h3>
          <span className="tag">{RULE_TAG}</span>
        </header>
        <Rule rule={DATA_SHARING} />
        <h4>What could send data</h4>
        <ul className="trust-list">
          {SENDERS.map((row) => (
            <Row key={row.id} row={row} extra={row.id === "rich-presence" ? `Right now: ${presence.on ? "on" : "off"}.` : undefined} />
          ))}
        </ul>
      </section>

      <section className="card" aria-labelledby={`${uid}-protections`}>
        <h3 id={`${uid}-protections`}>Protections</h3>
        <ul className="trust-list">
          {PROTECTIONS.map((row) => (
            <Row key={row.id} row={row} />
          ))}
        </ul>
      </section>
    </>
  );
}
