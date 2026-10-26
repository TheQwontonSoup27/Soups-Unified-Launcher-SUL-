import { useState } from "react";
import { MAX_ASSET_KEY } from "../discord";
import { usePresence } from "../presence";

/** Discord Rich Presence settings: the switch itself is in the sidebar; this holds the options. */
export function DiscordPanel() {
  const { on, message, smallImage, toggle, setSmallImage } = usePresence();
  const [draft, setDraft] = useState(smallImage);
  const dirty = draft.trim() !== smallImage;

  return (
    <section className="card">
      <h3>Discord Rich Presence</h3>
      <p className="muted">
        Off by default. When on, Discord shows that you are browsing the launcher; turning it off clears it. Games whose
        capability status is Unsupported or Not investigated never publish. Capability: Experimental, not yet tested against a live Discord client.
      </p>
      <div className="row">
        <button type="button" className="primary inline" role="switch" aria-checked={on} onClick={() => void toggle()}>
          {on ? "Turn presence off" : "Turn presence on"}
        </button>
        <span>{on ? "On" : "Off"}</span>
      </div>
      {message && <p role="status" className="muted">{message}</p>}

      <h4>Small corner image</h4>
      <p className="muted">
        Type the asset key of an image uploaded for the application in the Discord developer portal (for example a Team27 logo).
        Leave empty for none. Discord decides how large the corner image is drawn: the presence interface has no size setting,
        so a scale control cannot be offered.
      </p>
      <form className="row" onSubmit={(e) => { e.preventDefault(); void setSmallImage(draft); }}>
        <label className="row">
          <span>Asset key</span>
          <input type="text" value={draft} maxLength={MAX_ASSET_KEY} spellCheck={false} autoComplete="off" onChange={(e) => setDraft(e.target.value)} aria-label="Small image asset key" />
        </label>
        <button type="submit" className="primary inline" disabled={!dirty}>Save</button>
      </form>
    </section>
  );
}
