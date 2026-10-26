import { useCallback, useEffect, useState } from "react";
import { BackgroundLayer } from "./components/BackgroundLayer";
import { Splash } from "./components/Splash";
import Sidebar from "./components/Sidebar";
import { TitleBar } from "./components/TitleBar";
import { getAppInfo, type AppInfo } from "./lib/backend";
import { renderModule } from "./modules/registry";
import type { ViewId } from "./nav";
import { PresenceProvider } from "./presence";
import { SettingsProvider } from "./settings";
import GameLibrary from "./views/GameLibrary";
import PlaytimeTracker from "./views/PlaytimeTracker";
import SettingsView from "./views/SettingsView";

function renderView(view: ViewId) {
  if (view === "library") return <GameLibrary />;
  if (view === "settings") return <SettingsView />;
  if (view === "playtime") return <PlaytimeTracker />;
  return renderModule(view.slice("module:".length));
}

function Shell({ splashMs }: { splashMs: number }) {
  const [view, setView] = useState<ViewId>("library");
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [splash, setSplash] = useState(splashMs > 0);
  const done = useCallback(() => setSplash(false), []);

  useEffect(() => {
    getAppInfo().then(setInfo).catch(console.error);
  }, []);

  if (splash) return <Splash ms={splashMs} onDone={done} />;
  return (
    <>
      <BackgroundLayer />
      <div className="shell">
        <TitleBar />
        <Sidebar active={view} onSelect={setView} />
        <main className="content">
          {renderView(view)}
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
      <PresenceProvider>
        <Shell splashMs={splashMs} />
      </PresenceProvider>
    </SettingsProvider>
  );
}

export default App;
