import { useEffect, useMemo, useSyncExternalStore } from "react";
import { ModsPanel } from "../components/ModsPanel";
import { PageHeader } from "../components/PageHeader";
import { LibraryController, tauriLibraryApi, type LibraryApi } from "../lib/library";
import type { ModsApi } from "../lib/mods";

/**
 * Mods & Mod Builder. Only the mod store, profiles and deploy/purge (spec 2.1) are built so far, and all of it is
 * Experimental. `api` is the mods backend and `libraryApi` the library's (for game names); `undefined` means the real
 * one and `null` means none (a plain browser, or a test).
 */
export default function ModsView({ api, libraryApi }: { api?: ModsApi | null; libraryApi?: LibraryApi | null }) {
  const library = useMemo(() => new LibraryController(libraryApi === undefined ? tauriLibraryApi() : libraryApi), [libraryApi]);
  const state = useSyncExternalStore(library.subscribe, library.getSnapshot, library.getSnapshot);
  useEffect(() => {
    void library.load();
  }, [library]);

  return (
    <>
      <PageHeader title="Mods & Mod Builder" subtitle="Module 2 — mod profiles and deploy/purge are built and Experimental; the rest of the module is not built yet." />
      <ModsPanel api={api === undefined && libraryApi !== undefined ? null : api} games={state.games} />
      <section className="card">
        <h3>Not built yet</h3>
        <p className="muted">
          The pages for the mod builder, patches and external tools are not built yet (the code behind them exists, but nothing in this window uses it). Conflict analysis
          and measured comparisons are not built at all. Nothing here changes a game&apos;s original install.
        </p>
      </section>
    </>
  );
}
