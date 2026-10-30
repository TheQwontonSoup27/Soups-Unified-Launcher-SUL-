import { PerformanceArea } from "../components/PerformanceArea";
import { DENSITY_MODULES } from "../density";
import { renderModule } from "../modules/registry";

/**
 * Diagnostics & Performance: the module's placeholder page, then the Performance Overview, one section for each
 * module that has measurements. Only the Media & Clipping Engine is listed so far, and nothing is measured yet.
 */
export default function DiagnosticsView() {
  return (
    <>
      {renderModule("diagnostics")}
      <h2>Performance Overview</h2>
      <p className="muted">Measurements of SUL's own parts. Each module can show them compact or detailed.</p>
      {DENSITY_MODULES.map((m) => (
        <PerformanceArea key={m.id} moduleId={m.id} title={m.label} status={m.status} />
      ))}
    </>
  );
}
