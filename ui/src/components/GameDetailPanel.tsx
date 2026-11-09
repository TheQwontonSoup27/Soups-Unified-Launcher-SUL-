import { useState } from "react";
import { StatusBadge } from "./StatusBadge";
import type { CatalogueController, CatalogueState } from "../lib/catalogue";
import { plainPath } from "../lib/catalogue";
import { BUILD_STATE_LABEL } from "../lib/manager";
import { describeLaunchState } from "../lib/playing";

/**
 * Everything SUL knows about one game (spec 1.1), opened from the game's card: its builds and which one is active, its
 * adapter and the weakest capability label, whether it is running, plain notes about what is missing, and the owner's own
 * categories and artwork reference.
 *
 * Categories and the artwork reference are the owner's labels: changing them touches no game file. Artwork is only the
 * place of an image file the owner already has; SUL does not copy it, and it is not shown here because showing a local
 * image needs a permission in the app's configuration that has not been granted (an owner decision).
 */
export function GameDetailPanel({ controller, state, name }: { controller: CatalogueController; state: CatalogueState; name: string }) {
  const d = state.detail;
  const busy = state.busy !== null;
  const [category, setCategory] = useState("");
  const [artwork, setArtwork] = useState("");

  return (
    <section className="game-detail" aria-label={`Details of ${name}`}>
      {state.detailError && (
        <p className="matrix-error" role="alert">
          {state.detailError}
        </p>
      )}
      <p className="muted" role="status" aria-live="polite">
        {d === null && state.detailError === null ? "Loading details…" : busy ? "Saving…" : (state.notice ?? "")}
      </p>

      {d !== null && (
        <>
          <p>
            <StatusBadge status={d.capability} /> <span className="muted">Weakest label of everything that applies to this game ({d.capability_label}).</span>
          </p>
          <p>
            <strong>Adapter:</strong> {d.adapter ? `${d.adapter.id} ${d.adapter.version}` : "none registered"}
          </p>
          <p>
            <strong>Now:</strong> {describeLaunchState(d.launch)}
          </p>

          <div aria-label="Builds of this game">
            <h4>Builds</h4>
            {d.builds.length === 0 ? (
              <p className="muted">None yet.</p>
            ) : (
              <ul className="trust-list">
                {d.builds.map((b) => (
                  <li key={b.build_id} aria-label={`Build ${b.build_id}`}>
                    <span className="trust-title">{b.build_id}</span> <span className="tag">{b.slot}</span> <span className="tag">{b.managed ? "managed copy" : "install stays in place"}</span>{" "}
                    {b.active && <span className="tag">active</span>} <span className="tag">{BUILD_STATE_LABEL[b.state]}</span>
                  </li>
                ))}
              </ul>
            )}
          </div>

          {d.notes.length > 0 && (
            <ul className="muted" aria-label="Notes">
              {d.notes.map((n, i) => (
                <li key={i}>{n}</li>
              ))}
            </ul>
          )}

          <div aria-label="Categories of this game">
            <h4>Categories</h4>
            {d.categories.length === 0 ? (
              <p className="muted">None. Your own labels, saved on this computer; they change no game file.</p>
            ) : (
              <ul className="tag-list">
                {d.categories.map((c) => (
                  <li key={c}>
                    <span className="tag">{c}</span>
                    <button type="button" className="btn btn-ghost" disabled={busy} aria-label={`Remove category ${c}`} onClick={() => void controller.removeCategory(c)}>
                      Remove
                    </button>
                  </li>
                ))}
              </ul>
            )}
            <form
              className="row"
              onSubmit={(e) => {
                e.preventDefault();
                void controller.addCategory(category).then((ok) => ok && setCategory(""));
              }}
            >
              <label className="field">
                <span>Add a category (up to 40 characters; up to 20 per game)</span>
                <input type="text" value={category} onChange={(e) => setCategory(e.target.value)} />
              </label>
              <button type="submit" className="btn" disabled={busy} aria-label="Add category">
                Add
              </button>
            </form>
          </div>

          <div aria-label="Artwork of this game">
            <h4>Artwork</h4>
            {d.artwork === null ? (
              <p className="muted">No image chosen.</p>
            ) : (
              <p>
                Image file: <code>{plainPath(d.artwork)}</code>
              </p>
            )}
            <p className="muted">
              SUL only remembers where the image is (a .png, .jpg, .jpeg or .webp file you already have); it does not copy it. It cannot be shown here yet: showing a local image needs a permission
              in the app that has not been granted.
            </p>
            <form
              className="row"
              onSubmit={(e) => {
                e.preventDefault();
                void controller.setArtwork(artwork).then((ok) => ok && setArtwork(""));
              }}
            >
              <label className="field">
                <span>Path of an image file</span>
                <input type="text" value={artwork} onChange={(e) => setArtwork(e.target.value)} />
              </label>
              <button type="submit" className="btn" disabled={busy} aria-label="Use this image">
                Use this image
              </button>
              {d.artwork !== null && (
                <button type="button" className="btn btn-ghost" disabled={busy} aria-label="Clear artwork" onClick={() => void controller.clearArtwork()}>
                  Clear
                </button>
              )}
            </form>
          </div>
        </>
      )}

      <div className="row">
        <button type="button" className="btn btn-ghost" disabled={busy} aria-label="Refresh details" onClick={() => void controller.refreshDetail()}>
          Refresh
        </button>
        <button type="button" className="btn btn-ghost" aria-label="Close details" onClick={() => controller.close()}>
          Close
        </button>
      </div>
    </section>
  );
}
