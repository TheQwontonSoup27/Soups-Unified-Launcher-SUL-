import { useEffect, useMemo, useSyncExternalStore } from "react";
import { BuilderPanel } from "../components/BuilderPanel";
import { ModsPanel } from "../components/ModsPanel";
import { PageHeader } from "../components/PageHeader";
import { ToolsPanel } from "../components/ToolsPanel";
import { LibraryController, tauriLibraryApi, type LibraryApi } from "../lib/library";
import type { BuilderApi } from "../lib/builder";
import type { ModsApi } from "../lib/mods";
import type { ToolsApi } from "../lib/tools";

/**
 * Mods & Mod Builder. The mod store, profiles and deploy/purge (spec 2.1), the builder and patches (2.2 and 2.3) and the
 * external tools (2.4) are built, and all of it is Experimental. `api` is the mods backend, `builderApi` the builder's,
 * `toolsApi` the tools' and `libraryApi` the library's (for game names); `undefined` means the real one and `null` means
 * none (a plain browser, or a test).
 */
export default function ModsView({ api, builderApi, toolsApi, libraryApi }: { api?: ModsApi | null; builderApi?: BuilderApi | null; toolsApi?: ToolsApi | null; libraryApi?: LibraryApi | null }) {
  const library = useMemo(() => new LibraryController(libraryApi === undefined ? tauriLibraryApi() : libraryApi), [libraryApi]);
  const state = useSyncExternalStore(library.subscribe, library.getSnapshot, library.getSnapshot);
  useEffect(() => {
    void library.load();
  }, [library]);

  return (
    <>
      <PageHeader title="Mods & Mod Builder" subtitle="Module 2 — mod profiles, deploy/purge, the builder, patches and external tools are built and Experimental; conflict analysis and measured comparisons are not built." />
      <ModsPanel api={api === undefined && libraryApi !== undefined ? null : api} games={state.games} />
      <BuilderPanel api={builderApi === undefined && libraryApi !== undefined ? null : builderApi} games={state.games} />
      <ToolsPanel api={toolsApi === undefined && libraryApi !== undefined ? null : toolsApi} />
      <section className="card">
        <h3>Not built yet</h3>
        <p className="muted">
          Conflict analysis and measured comparisons are not built at all. Nothing on this page changes a game&apos;s original install.
        </p>
      </section>
    </>
  );
}
