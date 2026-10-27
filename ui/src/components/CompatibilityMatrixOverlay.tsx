import { StatusBadge } from "./StatusBadge";
import { COMPAT_HELP, COMPAT_LABEL, type FeatureId, type MatrixView } from "../lib/library";

export interface CompatibilityMatrixOverlayProps {
  /** Used for the accessible names. */
  gameName: string;
  /** Null while the matrix is still being read. */
  matrix: MatrixView | null;
  /** A problem with this game's matrix or tick, shown above the rows. */
  error?: string | null;
  /** Whether a change to this feature is in flight; its checkbox is locked until the answer arrives. */
  isPending?: (feature: FeatureId) => boolean;
  onToggle: (feature: FeatureId, enabled: boolean) => void;
  /** The element id, so a button can point at this region with `aria-controls`. */
  id?: string;
}

/**
 * One game's tick-and-compatibility matrix: a checkbox per feature, a colour-coded compatibility
 * badge and the capability label. The checkbox shows what the backend last saved, not what was
 * clicked: a tick the rules refuse bounces back and the reason appears above the rows.
 *
 * Colour is never the only signal: every badge carries its text.
 */
export function CompatibilityMatrixOverlay({
  gameName,
  matrix,
  error,
  isPending,
  onToggle,
  id,
}: CompatibilityMatrixOverlayProps) {
  return (
    <section className="matrix" id={id} aria-label={`Compatibility matrix for ${gameName}`}>
      {error && (
        <p className="matrix-error" role="alert">
          {error}
        </p>
      )}
      {matrix === null ? (
        !error && (
          <p className="muted" role="status">
            Reading the matrix…
          </p>
        )
      ) : (
        <ul className="matrix-rows">
          {matrix.rows.map((row) => {
            const noteId = `${id ?? matrix.game_id}-${row.feature}-note`;
            // Unticking is always allowed; ticking only when the matrix allows it.
            const locked = isPending?.(row.feature) === true || (!row.enabled && !row.can_enable);
            return (
              <li key={row.feature} className={`matrix-row compat-${row.compatibility}`}>
                <label className="matrix-check">
                  <input
                    type="checkbox"
                    checked={row.enabled}
                    disabled={locked}
                    aria-describedby={row.note ? noteId : undefined}
                    onChange={(event) => onToggle(row.feature, event.currentTarget.checked)}
                  />
                  <span>{row.label}</span>
                </label>
                <span className="matrix-badges">
                  <span
                    className={`badge badge-compat badge-compat-${row.compatibility}`}
                    title={COMPAT_HELP[row.compatibility]}
                  >
                    {COMPAT_LABEL[row.compatibility]}
                  </span>
                  <StatusBadge status={row.status} />
                </span>
                {row.note && (
                  <span className="matrix-note muted" id={noteId}>
                    {row.note}
                  </span>
                )}
              </li>
            );
          })}
        </ul>
      )}
    </section>
  );
}
