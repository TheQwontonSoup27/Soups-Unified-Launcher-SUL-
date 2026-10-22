import { useEffect, useState } from "react";
import { MODULES, renderModule } from "./modules/registry";
import { Overview } from "./overview";
import { THEMES, applyTheme, loadThemeId } from "./theme";

export function App() {
  const [page, setPage] = useState("overview");

  useEffect(() => {
    applyTheme(THEMES.find((t) => t.id === loadThemeId()) ?? THEMES[0]!);
  }, []);

  return (
    <div className="shell">
      <header className="topbar">
        <span className="logo">SUL</span>
        <strong>Soups Unified Launcher</strong>
        <span className="topbar-note">Early development • no features are tested yet</span>
      </header>
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
  );
}
