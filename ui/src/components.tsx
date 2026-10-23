import { useEffect, useRef, useState, type ReactNode } from "react";
import type { Status } from "./types";
import { STATUS_HELP, STATUS_LABEL } from "./capabilities";
import { useSettings } from "./settings";
import { isVideoType } from "./background";
import { getInvoker, loadPresenceWanted, savePresenceWanted, setPresence } from "./discord";

export function StatusBadge({ status }: { status: Status }) {
  return (
    <span className={`badge badge-${status}`} title={STATUS_HELP[status]}>
      {STATUS_LABEL[status]}
    </span>
  );
}

export function PageHeader({ title, subtitle }: { title: string; subtitle: string }) {
  return (
    <header className="page-header">
      <h1>{title}</h1>
      <p>{subtitle}</p>
    </header>
  );
}

interface TauriWindow { minimize(): Promise<void>; toggleMaximize(): Promise<void>; close(): Promise<void> }
function tauriWindow(): TauriWindow | null {
  try {
    const t = (globalThis as unknown as { __TAURI__?: { window?: { getCurrentWindow?: () => TauriWindow } } }).__TAURI__;
    return t?.window?.getCurrentWindow?.() ?? null;
  } catch { return null; }
}
function showControls(): boolean {
  if (typeof window === "undefined") return false;
  if (tauriWindow()) return true;
  return new URLSearchParams(window.location.search).get("desktop") === "1"; // design preview in a plain browser
}

/** Global Discord Rich Presence switch (spec: lives in the top-left global UI, off by default). */
export function PresenceToggle() {
  const [on, setOn] = useState(false);
  const [msg, setMsg] = useState("");
  useEffect(() => {
    if (!loadPresenceWanted()) return;
    void setPresence(true, getInvoker()).then((r) => { setOn(r.on); setMsg(r.message); if (!r.on) savePresenceWanted(false); });
  }, []);
  const flip = async () => {
    const r = await setPresence(!on, getInvoker());
    setOn(r.on); setMsg(r.message); savePresenceWanted(r.on);
  };
  return (
    <span className="presence">
      <button role="switch" aria-checked={on} className={on ? "presence-btn on" : "presence-btn"} onClick={() => void flip()}
        title="Show what you are doing in Discord. Off by default; games that are not supported never publish.">
        {`Discord presence: ${on ? "On" : "Off"}`}
      </button>
      {msg && <span className="presence-msg" role="status">{msg}</span>}
    </span>
  );
}

export function TitleBar() {
  const controls = showControls();
  const win = controls ? tauriWindow() : null;
  return (
    <header className="titlebar" data-tauri-drag-region>
      <img className="titlebar-icon" src="logo.png" alt="" width={24} height={24} data-tauri-drag-region />
      <strong data-tauri-drag-region>Soups Unified Launcher</strong>
      <PresenceToggle />
      <span className="titlebar-note" data-tauri-drag-region>Early development • no features are tested yet</span>
      {controls && (
        <div className="wincontrols">
          <button aria-label="Minimize" onClick={() => void win?.minimize()}><svg width="12" height="12" viewBox="0 0 12 12" aria-hidden><path d="M1 6h10" stroke="currentColor" strokeWidth="1.5" /></svg></button>
          <button aria-label="Maximize or restore" onClick={() => void win?.toggleMaximize()}><svg width="12" height="12" viewBox="0 0 12 12" aria-hidden><rect x="1.5" y="1.5" width="9" height="9" fill="none" stroke="currentColor" strokeWidth="1.5" /></svg></button>
          <button aria-label="Close" className="close" onClick={() => void win?.close()}><svg width="12" height="12" viewBox="0 0 12 12" aria-hidden><path d="M1 1l10 10M11 1L1 11" stroke="currentColor" strokeWidth="1.5" /></svg></button>
        </div>
      )}
    </header>
  );
}

export function Splash({ ms, onDone }: { ms: number; onDone: () => void }) {
  useEffect(() => {
    const reduce = typeof window !== "undefined" && window.matchMedia?.("(prefers-reduced-motion: reduce)").matches;
    const t = setTimeout(onDone, reduce ? Math.min(ms, 600) : ms);
    return () => clearTimeout(t);
  }, [ms, onDone]);
  return (
    <div className="splash" role="img" aria-label="Soups Unified Launcher, Team27" onClick={onDone}>
      <img src="logo.png" alt="" />
    </div>
  );
}

export function BackgroundLayer() {
  const { bg, wallpapers, custom } = useSettings();
  let media: ReactNode = null;
  if (bg.mode === "custom" && custom) {
    media = isVideoType(custom.type)
      ? <video src={custom.url} autoPlay loop muted playsInline />
      : <img src={custom.url} alt="" />;
  } else if (bg.mode === "gallery") {
    const w = wallpapers.find((x) => x.id === bg.galleryId);
    if (w) media = <img src={w.file} alt="" />;
  }
  return (
    <div className="bg-layer" aria-hidden>
      {media}
      {media && <div className="bg-dim" style={{ opacity: bg.overlay }} />}
    </div>
  );
}

export function Dialog({ title, children, onClose, actions }: { title: string; children: ReactNode; onClose: () => void; actions: ReactNode }) {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    ref.current?.querySelector<HTMLElement>("button")?.focus();
    const onKey = (e: KeyboardEvent) => { if (e.key === "Escape") onClose(); };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);
  return (
    <div className="scrim" onClick={onClose}>
      <div className="dialog" role="dialog" aria-modal="true" aria-label={title} ref={ref} onClick={(e) => e.stopPropagation()}>
        <h3>{title}</h3>
        <div>{children}</div>
        <div className="dialog-actions">{actions}</div>
      </div>
    </div>
  );
}
