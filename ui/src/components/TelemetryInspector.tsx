import { StatusBadge } from "./StatusBadge";

/**
 * The full panel for one measured item, opened in place by "Details". Nothing is measured yet: the sampler is
 * not built, so every figure is a dash and the panel says so. The rows are the ones the finished panel will fill:
 * what the item is predicted to cost, what it costs now (averaged over the last 10 seconds), and the difference.
 */
export function TelemetryInspector({ id, title }: { id: string; title: string }) {
  return (
    <div className="telemetry-panel" id={id} role="region" aria-label={`${title}: details`}>
      <h4>
        {title} <StatusBadge status="not_investigated" />
      </h4>
      <table className="telemetry">
        <thead>
          <tr>
            <th scope="col">Measure</th>
            <th scope="col">Predicted spec</th>
            <th scope="col">Live (10 s average)</th>
            <th scope="col">Difference</th>
          </tr>
        </thead>
        <tbody>
          <tr>
            <th scope="row">RAM (this subsystem)</th>
            <td>—</td>
            <td>—</td>
            <td>—</td>
          </tr>
          <tr>
            <th scope="row">CPU (this subsystem)</th>
            <td>—</td>
            <td>—</td>
            <td>—</td>
          </tr>
          <tr>
            <th scope="row">Frame-time variance</th>
            <td>—</td>
            <td>not available</td>
            <td>—</td>
          </tr>
          <tr>
            <th scope="row">Active threads</th>
            <td>n/a</td>
            <td>—</td>
            <td>n/a</td>
          </tr>
          <tr>
            <th scope="row">Kernel interrupts</th>
            <td colSpan={3}>system-wide only, so they cannot be attributed to SUL</td>
          </tr>
        </tbody>
      </table>
      <p className="muted">
        Nothing is measured yet: the sampler is not built, so no figure is shown. Once it exists it will sample once a
        second, average over 10 seconds, keep everything on this computer and upload nothing.
      </p>
    </div>
  );
}
