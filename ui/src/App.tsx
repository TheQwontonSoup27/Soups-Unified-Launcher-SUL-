import { useCallback, useEffect, useState } from "react";
import { BackgroundLayer } from "./components/BackgroundLayer";
import { Splash } from "./components/Splash";
import Sidebar from "./components/Sidebar";
import { TitleBar } from "./components/TitleBar";
import { getAppInfo, type AppInfo } from "./lib/backend";
import { DensityProvider } from "./densityContext";
import { renderModule } from "./modules/registry";
import type { ViewId } from "./nav";
import { PresenceProvider } from "./presence";
import { SettingsProvider } from "./settings";
import DiagnosticsView from "./views/DiagnosticsView";
import GameLibrary from "./views/GameLibrary";
import PlaytimeTracker from "./views/PlaytimeTracker";
import SettingsView from "./views/SettingsView";

function renderView(view: ViewId, query: string) {
  if (view === "library") return <GameLibrary query={query} />;
  if (view === "settings") return <SettingsView />;
  if (view === "playtime") return <PlaytimeTracker />;
  if (view === "module:diagnostics") return <DiagnosticsView />;
  return renderModule(view.slice("module:".length));
}

function Shell({ splashMs }: { splashMs: number }) {
  const [view, setView] = useState<ViewId>("library");
  const [query, setQuery] = useState("");
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [splash, setSplash] = useState(splashMs > 0);
  const done = useCallback(() => setSplash(false), []);

  // The search looks through the games, which only the library lists: typing anywhere else goes there.
  const search = useCallback((text: string) => {
    setQuery(text);
    if (text.trim() !== "") setView("library");
  }, []);

  useEffect(() => {
    getAppInfo().then(setInfo).catch(console.error);
  }, []);

  if (splash) return <Splash ms={splashMs} onDone={done} />;
  return (
    <>
      <BackgroundLayer />
      <div className="shell">
        <TitleBar />
        <Sidebar active={view} onSelect={setView} query={query} onSearch={search} />
        <main className="content">
          {renderView(view, query)}
          <footer className="status">
            {info
              ? `v${info.version} · ${info.install_mode} install · ${info.app_root}`
              : "Browser preview: no Tauri backend connected"}
          </footer>
        </main>
      </div>
    </>
  );
}

export function App({ splashMs = 1800 }: { splashMs?: number }) {
  return (
    <SettingsProvider>
      <DensityProvider>
        <PresenceProvider>
          <Shell splashMs={splashMs} />
        </PresenceProvider>
      </DensityProvider>
    </SettingsProvider>
  );
}

export default App;
