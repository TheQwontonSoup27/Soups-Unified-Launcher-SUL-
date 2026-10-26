import { getWindowControls } from "../lib/backend";

function previewControls(): boolean {
  // `?desktop=1` shows the window buttons in a plain browser, for design review only.
  return typeof window !== "undefined" && new URLSearchParams(window.location.search).get("desktop") === "1";
}

/** Custom title bar: the window is borderless (`decorations: false`), so this row drags and closes it. */
export function TitleBar() {
  const win = getWindowControls();
  const controls = win !== null || previewControls();
  return (
    <header className="titlebar" data-tauri-drag-region>
      <img className="titlebar-icon" src="logo.png" alt="" width={24} height={24} data-tauri-drag-region />
      <strong data-tauri-drag-region>Soups Unified Launcher</strong>
      <span className="titlebar-note" data-tauri-drag-region>Early development • no features are tested yet</span>
      {controls && (
        <div className="wincontrols">
          <button type="button" aria-label="Minimize" onClick={() => void win?.minimize()}><svg width="12" height="12" viewBox="0 0 12 12" aria-hidden><path d="M1 6h10" stroke="currentColor" strokeWidth="1.5" /></svg></button>
          <button type="button" aria-label="Maximize or restore" onClick={() => void win?.toggleMaximize()}><svg width="12" height="12" viewBox="0 0 12 12" aria-hidden><rect x="1.5" y="1.5" width="9" height="9" fill="none" stroke="currentColor" strokeWidth="1.5" /></svg></button>
          <button type="button" aria-label="Close" className="close" onClick={() => void win?.close()}><svg width="12" height="12" viewBox="0 0 12 12" aria-hidden><path d="M1 1l10 10M11 1L1 11" stroke="currentColor" strokeWidth="1.5" /></svg></button>
        </div>
      )}
    </header>
  );
}
