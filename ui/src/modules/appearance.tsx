import { useRef, useState } from "react";
import { Dialog } from "../components/Dialog";
import { ACCEPTED } from "../background";
import { useSettings } from "../settings";
import { THEMES, TOKEN_KEYS, TOKEN_LABELS, checkPairs } from "../theme";

/** Themes, colours and background. Shown as a pane inside Settings. */
export function AppearancePanel() {
  const s = useSettings();
  const [msg, setMsg] = useState<string | null>(null);
  const [confirm, setConfirm] = useState(false);
  const fileRef = useRef<HTMLInputElement>(null);
  const results = checkPairs(s.theme);
  const failing = results.filter((r) => !r.ok);

  return (
    <>
      <section className="card">
        <h3>Theme</h3>
        <div className="theme-grid" role="radiogroup" aria-label="Theme">
          {THEMES.map((t) => (
            <button key={t.id} role="radio" aria-checked={s.themeId === t.id}
              className={`theme-option${s.themeId === t.id ? " selected" : ""}`} onClick={() => s.setThemeId(t.id)}>
              <span className="swatch" style={{ background: t.bg, borderColor: t.accent }}>
                <i style={{ background: t.titlebar }} /><i style={{ background: t.button }} />
              </span>
              <strong>{t.label}</strong>
            </button>
          ))}
        </div>
      </section>

      <section className="card">
        <h3>Colours</h3>
        <p className="muted">Change any colour. Edits are saved on top of the selected theme.</p>
        <div className="color-grid">
          {TOKEN_KEYS.map((k) => (
            <label key={k} className="color-row">
              <input type="color" value={s.theme[k]} onChange={(e) => s.setColor(k, e.target.value)} aria-label={TOKEN_LABELS[k]} />
              <span>{TOKEN_LABELS[k]}</span>
              <code>{s.theme[k]}</code>
            </label>
          ))}
        </div>
        <button className="primary inline" onClick={s.resetColors}>Reset colours to theme</button>
        <div className="contrast" role="status">
          {failing.length === 0
            ? <p className="muted">All text/background pairs meet the 4.5:1 readability guideline.</p>
            : <ul>{failing.map((f) => <li key={f.label}>⚠ {f.label}: {f.ratio.toFixed(1)}:1 is hard to read (aim for 4.5:1)</li>)}</ul>}
        </div>
      </section>

      <section className="card">
        <h3>Background</h3>
        <div className="seg" role="radiogroup" aria-label="Background type">
          {([["solid", "Solid colour"], ["gallery", "Gallery"], ["custom", "My file"]] as const).map(([m, label]) => (
            <button key={m} role="radio" aria-checked={s.bg.mode === m} className={s.bg.mode === m ? "selected" : ""}
              disabled={m === "custom" && !s.custom} onClick={() => s.setBg({ mode: m })}>{label}</button>
          ))}
        </div>

        <h4>Gallery</h4>
        {s.wallpapers.length === 0
          ? <p className="muted">No gallery images are installed. Place images in <code>ui/public/wallpapers/</code> (see the README there).</p>
          : <div className="wall-grid">
              {s.wallpapers.map((w) => (
                <button key={w.id} className={`wall${s.bg.mode === "gallery" && s.bg.galleryId === w.id ? " selected" : ""}`}
                  onClick={() => s.setBg({ mode: "gallery", galleryId: w.id })} title={w.lowRes ? `${w.name} (low resolution)` : w.name}>
                  <img src={w.thumb} alt={w.name} loading="lazy" />
                  <span>{w.name}{w.lowRes ? " • low-res" : ""}</span>
                </button>
              ))}
            </div>}

        <h4>Your own picture, GIF or video</h4>
        <input ref={fileRef} type="file" accept={ACCEPTED} hidden
          onChange={async (e) => { const f = e.target.files?.[0]; if (f) setMsg(await s.setCustomFile(f)); e.target.value = ""; }} />
        <div className="row">
          <button className="primary inline" onClick={() => fileRef.current?.click()}>Choose file…</button>
          {s.custom && <button className="primary inline" onClick={() => setConfirm(true)}>Remove “{s.custom.name}”</button>}
        </div>
        {msg && <p role="status" className="muted">{msg}</p>}

        <h4>Readability dimming</h4>
        <label className="row">
          <input type="range" min={0} max={90} value={Math.round(s.bg.overlay * 100)} onChange={(e) => s.setBg({ overlay: Number(e.target.value) / 100 })} aria-label="Background dimming" />
          <span>{Math.round(s.bg.overlay * 100)}%</span>
        </label>
      </section>

      {confirm && (
        <Dialog title="Remove background file?" onClose={() => setConfirm(false)}
          actions={<>
            <button className="primary inline" onClick={() => setConfirm(false)}>Keep it</button>
            <button className="primary inline" onClick={async () => { await s.removeCustom(); setConfirm(false); }}>Remove</button>
          </>}>
          <p>This deletes the stored copy from SUL. Your original file is not touched.</p>
        </Dialog>
      )}
    </>
  );
}
