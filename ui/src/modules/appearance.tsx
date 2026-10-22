import { useState } from "react";
import { PageHeader } from "../components";
import { THEMES, applyTheme, contrast, loadThemeId, saveThemeId } from "../theme";

export function AppearancePage() {
  const [current, setCurrent] = useState(loadThemeId());
  return (
    <>
      <PageHeader title="Appearance" subtitle="Theme presets built on shared design tokens." />
      <section className="card">
        <div className="theme-grid" role="radiogroup" aria-label="Theme">
          {THEMES.map((t) => (
            <button
              key={t.id}
              role="radio"
              aria-checked={current === t.id}
              className={`theme-option${current === t.id ? " selected" : ""}`}
              onClick={() => { applyTheme(t); saveThemeId(t.id); setCurrent(t.id); }}
            >
              <span className="swatch" style={{ background: t.bg, borderColor: t.accent }}>
                <i style={{ background: t.accent }} />
              </span>
              <strong>{t.label}</strong>
              <small>Text contrast {contrast(t.text, t.bg).toFixed(1)}:1</small>
            </button>
          ))}
        </div>
      </section>
    </>
  );
}
