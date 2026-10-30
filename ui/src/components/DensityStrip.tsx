import { useId } from "react";
import { CHOICES, CHOICE_LABEL, choiceFor, describeEffective } from "../density";
import { useDensity } from "../densityContext";

/**
 * A module's own display density: Inherit Global, Always Compact or Always Detailed, and which view that makes
 * today. It edits the same stored value as the card in Settings > Appearance.
 */
export function DensityStrip({ moduleId, moduleLabel }: { moduleId: string; moduleLabel: string }) {
  const { settings, setChoice } = useDensity();
  const labelId = useId();
  const current = choiceFor(settings, moduleId);
  return (
    <div className="density-row">
      <span id={labelId}>View</span>
      <div className="seg wrap" role="radiogroup" aria-labelledby={labelId} aria-describedby={`${labelId}-now`} data-module={moduleId}>
        {CHOICES.map((choice) => (
          <button
            key={choice}
            type="button"
            role="radio"
            aria-checked={current === choice}
            aria-label={`${CHOICE_LABEL[choice]} for ${moduleLabel}`}
            className={current === choice ? "selected" : ""}
            onClick={() => setChoice(moduleId, choice)}
          >
            {CHOICE_LABEL[choice]}
          </button>
        ))}
      </div>
      <span id={`${labelId}-now`} className="muted" aria-live="polite">
        now: {describeEffective(settings, moduleId)}
      </span>
    </div>
  );
}
