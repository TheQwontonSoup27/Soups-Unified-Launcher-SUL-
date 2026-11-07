import { useEffect, useMemo, useSyncExternalStore } from "react";
import { BuilderPanel } from "../components/BuilderPanel";
import { ModsPanel } from "../components/ModsPanel";
import { PageHeader } from "../components/PageHeader";
import { LibraryController, tauriLibraryApi, type LibraryApi } from "../lib/library";
import type { BuilderApi } from "../lib/builder";
import type { ModsApi } from "../lib/mods";

/**
 * Mods & Mod Builder. The mod store, profiles and deploy/purge (spec 2.1), and the builder and patches (2.2 and 2.3),
 * are built, and all of it is Experimental. `api` is the mods backend, `builderApi` the builder's and `libraryApi` the
 * library's (for game names); `undefined` means the real one and `null` means none (a plain browser, or a test).
 */
export default function ModsView({ api, builderApi, libraryApi }: { api?: ModsApi | null; builderApi?: BuilderApi | null; libraryApi?: LibraryApi | null }) {
  const library = useMemo(() => new LibraryController(libraryApi === undefined ? tauriLibraryApi() : libraryApi), [libraryApi]);
  const state = useSyncExternalStore(library.subscribe, library.getSnapshot, library.getSnapshot);
  useEffect(() => {
    void library.load();
  }, [library]);

  return (
    <>
      <PageHeader title="Mods & Mod Builder" subtitle="Module 2 — mod profiles, deploy/purge, the builder and patches are built and Experimental; external tools are not built yet." />
      <ModsPanel api={api === undefined && libraryApi !== undefined ? null : api} games={state.games} />
      <BuilderPanel api={builderApi === undefined && libraryApi !== undefined ? null : builderApi} games={state.games} />
      <section className="card">
        <h3>Not built yet</h3>
        <p className="muted">
          The page for external tools is not built yet (the code behind it exists, but nothing in this window uses it). Conflict analysis and measured comparisons are not
          built at all. Nothing here changes a game&apos;s original install.
        </p>
      </section>
    </>
  );
}
