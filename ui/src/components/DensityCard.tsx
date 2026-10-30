import { DENSITY_LABEL, CHOICE_LABEL, overridesInUse, type Density } from "../density";
import { useDensity } from "../densityContext";

const GLOBAL_CHOICES: readonly Density[] = ["compact", "detailed"];

/** Settings > Appearance: the global display density and the overrides modules have set. */
export function DensityCard() {
  const { settings, setGlobal, clearOverrides } = useDensity();
  const inUse = overridesInUse(settings);
  return (
    <section className="card" aria-labelledby="density-title">
      <h3 id="density-title">Display density</h3>
      <p className="muted">
        How much detail a panel of measurements shows. Compact is one line with a Details button; Detailed shows the full
        panel. A module can override this for itself.
      </p>
      <div className="density-row">
        <span id="density-global-label">All modules</span>
        <div className="seg" role="radiogroup" aria-labelledby="density-global-label">
          {GLOBAL_CHOICES.map((density) => (
            <button
              key={density}
              type="button"
              role="radio"
              aria-checked={settings.global === density}
              className={settings.global === density ? "selected" : ""}
              onClick={() => setGlobal(density)}
            >
              {DENSITY_LABEL[density]}
            </button>
          ))}
        </div>
      </div>
      {inUse.length === 0 ? (
        <p className="muted">No module overrides: every module follows this setting.</p>
      ) : (
        <>
          <p>Modules with their own setting:</p>
          <ul className="overrides">
            {inUse.map((o) => (
              <li key={o.id}>
                {o.label}: {CHOICE_LABEL[o.density]}
              </li>
            ))}
          </ul>
          <button type="button" className="btn btn-ghost" onClick={clearOverrides}>
            Clear overrides
          </button>
        </>
      )}
    </section>
  );
}
