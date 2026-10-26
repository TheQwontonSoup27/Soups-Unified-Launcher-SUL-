import { PageHeader } from "../components/PageHeader";

export default function PlaytimeTracker() {
  return (
    <>
      <PageHeader title="General Playtime Tracker" subtitle="Cumulative playtime across all platforms." />
      <section className="card">
        <p className="muted">Placeholder. Nothing is tracked yet.</p>
      </section>
    </>
  );
}
