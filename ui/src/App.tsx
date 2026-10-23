import { useCallback, useState } from "react";
import { BackgroundLayer, Splash, TitleBar } from "./components";
import { MODULES, renderModule } from "./modules/registry";
import { Overview } from "./overview";
import { SettingsProvider } from "./settings";

function Shell({ splashMs }: { splashMs: number }) {
  const [page, setPage] = useState("overview");
  const [splash, setSplash] = useState(splashMs > 0);
  const done = useCallback(() => setSplash(false), []);
  if (splash) return <Splash ms={splashMs} onDone={done} />;
  return (
    <>
      <BackgroundLayer />
      <div className="shell">
        <TitleBar />
        <nav className="sidebar" aria-label="Modules">
          <button className={page === "overview" ? "nav active" : "nav"} aria-current={page === "overview" ? "page" : undefined} onClick={() => setPage("overview")}>
            <span aria-hidden>⌂</span> Overview
          </button>
          {MODULES.map((m) => (
            <button key={m.id} className={page === m.id ? "nav active" : "nav"} aria-current={page === m.id ? "page" : undefined} onClick={() => setPage(m.id)}>
              <span aria-hidden>{m.glyph}</span> {m.title}
            </button>
          ))}
        </nav>
        <main className="content">{page === "overview" ? <Overview onOpen={setPage} /> : renderModule(page)}</main>
      </div>
    </>
  );
}

export function App({ splashMs = 1800 }: { splashMs?: number }) {
  return (
    <SettingsProvider>
      <Shell splashMs={splashMs} />
    </SettingsProvider>
  );
}
