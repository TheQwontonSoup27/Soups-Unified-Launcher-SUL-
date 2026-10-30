import { useId, useState } from "react";
import { effectiveDensity } from "../density";
import { useDensity } from "../densityContext";
import type { Status } from "../types";
import { DensityStrip } from "./DensityStrip";
import { StatusBadge } from "./StatusBadge";
import { TelemetryInspector } from "./TelemetryInspector";

/**
 * One module's measurements on the Performance Overview: its title and capability label, its own density strip,
 * and the engine total. Compact shows one line with a Details button that opens the panel in place; Detailed shows
 * the panel straight away.
 */
export function PerformanceArea({ moduleId, title, status }: { moduleId: string; title: string; status: Status }) {
  const { settings } = useDensity();
  const [opened, setOpened] = useState(false);
  const uid = useId();
  const panelId = `${uid}-panel`;
  const detailed = effectiveDensity(settings, moduleId) === "detailed";
  const showPanel = detailed || opened;

  return (
    <section className="card perf-area" aria-labelledby={`${uid}-title`} data-module={moduleId}>
      <header className="game-card-head">
        <h3 id={`${uid}-title`}>{title}</h3>
        <StatusBadge status={status} />
      </header>
      <DensityStrip moduleId={moduleId} moduleLabel={title} />
      {!detailed && (
        <div className="perf-row">
          <span className="tag" aria-label="Engine total: RAM not measured, CPU not measured">
            Engine total · RAM — | CPU —
          </span>
          <button
            type="button"
            className="btn btn-ghost"
            aria-expanded={opened}
            aria-controls={opened ? panelId : undefined}
            onClick={() => setOpened(!opened)}
          >
            {opened ? "Hide details" : <>Details <span aria-hidden="true">→</span></>}
          </button>
        </div>
      )}
      {showPanel && <TelemetryInspector id={panelId} title={`${title}, engine total`} />}
    </section>
  );
}
