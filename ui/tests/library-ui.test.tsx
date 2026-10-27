import { test } from "node:test";
import assert from "node:assert/strict";
import { renderToStaticMarkup } from "react-dom/server";
import { CompatibilityMatrixOverlay } from "../src/components/CompatibilityMatrixOverlay";
import { GameLibraryGrid } from "../src/components/GameLibraryGrid";
import { EMPTY_LIBRARY, pendingKey, type LibraryActions, type LibraryState } from "../src/lib/library";
import GameLibrary from "../src/views/GameLibrary";
import { FEATURES, gogGame, matrixFor, steamGame } from "./fixtures";

const noop = () => {};
const actions: LibraryActions = { toggleExpanded: noop, setTick: noop, play: noop };
const withState = (patch: Partial<LibraryState>): LibraryState => ({ ...EMPTY_LIBRARY, loaded: true, ...patch });

/** The opening tag of the first element whose markup contains `marker`, or fails the test. */
function tagContaining(html: string, marker: string): string {
  const at = html.indexOf(marker);
  assert.ok(at >= 0, `${marker} not found`);
  const start = html.lastIndexOf("<", at);
  return html.slice(start, html.indexOf(">", at) + 1);
}

/** The `<li>` of one feature row. */
function row(html: string, label: string): string {
  const at = html.indexOf(`<span>${label}</span>`);
  assert.ok(at >= 0, `row ${label} not found`);
  return html.slice(html.lastIndexOf("<li", at), html.indexOf("</li>", at));
}

// ---------------------------------------------------------------------------
// CompatibilityMatrixOverlay
// ---------------------------------------------------------------------------

test("the matrix lists all eight features in order, each with a checkbox and both badges", () => {
  const html = renderToStaticMarkup(
    <CompatibilityMatrixOverlay gameName="Helldivers 2" matrix={matrixFor(steamGame)} onToggle={noop} />,
  );
  assert.match(html, /aria-label="Compatibility matrix for Helldivers 2"/);
  assert.equal((html.match(/<input type="checkbox"/g) ?? []).length, 8);
  const positions = FEATURES.map(([, label]) => html.indexOf(`<span>${label}</span>`));
  assert.ok(positions.every((p) => p >= 0));
  assert.deepEqual(positions, [...positions].sort((a, b) => a - b), "rows are in the core's order");
  // Colour is never the only signal: the verdict and the capability label are written out.
  for (const [, label] of FEATURES) {
    const r = row(html, label);
    assert.match(r, /badge-compat[^>]*>(Unverified|Not applicable)</);
    assert.match(r, /class="badge badge-(not_investigated|unsupported)"/);
  }
});

test("ticks show what was saved", () => {
  const html = renderToStaticMarkup(
    <CompatibilityMatrixOverlay gameName="Helldivers 2" matrix={matrixFor(steamGame, ["save_isolation", "mods"])} onToggle={noop} />,
  );
  assert.match(row(html, "Save isolation"), /<input[^>]*checked=""/);
  assert.match(row(html, "Mods"), /<input[^>]*checked=""/);
  assert.doesNotMatch(row(html, "Console overlay"), /checked=""/);
});

test("a feature that cannot apply cannot be ticked, and says why", () => {
  const html = renderToStaticMarkup(
    <CompatibilityMatrixOverlay gameName="Alpha" matrix={matrixFor(gogGame)} onToggle={noop} id="matrix-gog-1" />,
  );
  const r = row(html, "Steam headless launch");
  assert.match(r, /<input[^>]*disabled=""/);
  assert.match(r, /badge-compat-not_applicable[^>]*>Not applicable</);
  assert.match(r, /class="badge badge-unsupported"/);
  assert.match(r, /Steam titles only/);
  assert.match(r, /aria-describedby="matrix-gog-1-steam_headless-note"/);
  assert.match(r, /id="matrix-gog-1-steam_headless-note"/);
  // Every other row is free to tick.
  assert.doesNotMatch(row(html, "Mods"), /disabled=""/);
});

test("a tick that is already on can always be removed, even if the verdict has since turned against it", () => {
  const matrix = matrixFor(steamGame, ["overlay"]);
  const overlay = matrix.rows.find((r) => r.feature === "overlay")!;
  overlay.compatibility = "incompatible";
  overlay.status = "unsupported";
  overlay.can_enable = false;
  const html = renderToStaticMarkup(<CompatibilityMatrixOverlay gameName="Helldivers 2" matrix={matrix} onToggle={noop} />);
  const r = row(html, "Console overlay");
  assert.match(r, /<input[^>]*checked=""/);
  assert.doesNotMatch(r, /<input[^>]*disabled=""/, "unticking stays possible");
  assert.match(r, /badge-compat-incompatible[^>]*>Incompatible</);

  // The same verdict on an unticked row blocks ticking.
  overlay.enabled = false;
  const off = renderToStaticMarkup(<CompatibilityMatrixOverlay gameName="Helldivers 2" matrix={matrix} onToggle={noop} />);
  assert.match(row(off, "Console overlay"), /<input[^>]*disabled=""/);
});

test("a row is locked while its change is in flight, and only that row", () => {
  const html = renderToStaticMarkup(
    <CompatibilityMatrixOverlay gameName="Helldivers 2" matrix={matrixFor(steamGame)} isPending={(f) => f === "mods"} onToggle={noop} />,
  );
  assert.match(row(html, "Mods"), /<input[^>]*disabled=""/);
  assert.doesNotMatch(row(html, "Diagnostics"), /disabled=""/);
});

test("the matrix says it is loading, and shows a problem as an alert", () => {
  const loading = renderToStaticMarkup(<CompatibilityMatrixOverlay gameName="X" matrix={null} onToggle={noop} />);
  assert.match(loading, /Reading the matrix/);
  assert.doesNotMatch(loading, /<input/);

  const failed = renderToStaticMarkup(
    <CompatibilityMatrixOverlay gameName="X" matrix={null} error="unknown game: x" onToggle={noop} />,
  );
  assert.match(failed, /role="alert"[^>]*>unknown game: x</);
  assert.doesNotMatch(failed, /Reading the matrix/);

  const refused = renderToStaticMarkup(
    <CompatibilityMatrixOverlay gameName="Alpha" matrix={matrixFor(gogGame)} error="steam_headless is not available for this game" onToggle={noop} />,
  );
  assert.match(refused, /role="alert"/);
  assert.equal((refused.match(/<input/g) ?? []).length, 8, "the rows stay visible next to the problem");
});

// ---------------------------------------------------------------------------
// GameLibraryGrid
// ---------------------------------------------------------------------------

test("the grid shows nothing before there are games", () => {
  assert.equal(renderToStaticMarkup(<GameLibraryGrid state={withState({})} actions={actions} />), "");
});

test("each scanned game gets its own card, side by side in one grid", () => {
  const html = renderToStaticMarkup(
    <GameLibraryGrid state={withState({ games: [steamGame, gogGame] })} actions={actions} />,
  );
  assert.equal((html.match(/<section class="game-library-grid"/g) ?? []).length, 1);
  assert.equal((html.match(/<article class="card game-card/g) ?? []).length, 2);
  assert.match(html, /<h3 id="game-steam-553850">Helldivers 2<\/h3>/);
  assert.match(html, /<h3 id="game-gog-1">Alpha<\/h3>/);
  assert.match(html, /class="tag">Steam</);
  assert.match(html, /class="tag">GOG</);
  assert.match(html, /Alpha Studio/);
  assert.match(html, /class="badge badge-not_investigated"/);
  assert.match(html, /class="badge badge-unsupported"/);
});

test("Play is available for a Steam game and explained as unavailable for the rest", () => {
  const html = renderToStaticMarkup(
    <GameLibraryGrid state={withState({ games: [steamGame, gogGame, { ...steamGame, id: "steam-1", name: "Gone", present: false }] })} actions={actions} />,
  );
  const cards = html.split("<article").slice(1);
  const play = (card: string) => card.match(/<button[^>]*>(?:Play|Starting…)<\/button>/)![0];

  assert.doesNotMatch(play(cards[0]!), /disabled/);
  assert.match(play(cards[1]!), /disabled=""/);
  assert.match(cards[1]!, /Only Steam games can be started from here for now\./);
  assert.match(play(cards[2]!), /disabled=""/);
  assert.match(cards[2]!, /Not found on the last scan\./);
  assert.match(cards[2]!, /tag tag-warn">Not found on the last scan</);
  assert.match(cards[2]!, /game-card-missing/);
  // The hint is tied to the button for screen readers.
  assert.match(cards[1]!, /aria-describedby="play-hint-gog-1"/);
  assert.match(cards[1]!, /id="play-hint-gog-1"/);
});

test("Play shows it is working while Steam is being asked", () => {
  const html = renderToStaticMarkup(
    <GameLibraryGrid state={withState({ games: [steamGame], pending: { [pendingKey(steamGame.id, "play")]: true } })} actions={actions} />,
  );
  assert.match(html, /<button[^>]*disabled=""[^>]*>Starting…<\/button>/);
});

test("the matrix opens inside its own card only", () => {
  const html = renderToStaticMarkup(
    <GameLibraryGrid
      state={withState({ games: [steamGame, gogGame], expanded: { "gog-1": true }, matrices: { "gog-1": matrixFor(gogGame) } })}
      actions={actions}
    />,
  );
  const [steamCard, gogCard] = html.split("<article").slice(1) as [string, string];
  assert.match(steamCard, /aria-expanded="false"[^>]*aria-controls="matrix-steam-553850"/);
  assert.doesNotMatch(steamCard, /class="matrix"/);
  assert.match(steamCard, />Compatibility<\/button>/);

  assert.match(gogCard, /aria-expanded="true"[^>]*aria-controls="matrix-gog-1"/);
  assert.match(gogCard, /<section class="matrix" id="matrix-gog-1" aria-label="Compatibility matrix for Alpha"/);
  assert.match(gogCard, />Hide compatibility<\/button>/);
  assert.equal((gogCard.match(/<input type="checkbox"/g) ?? []).length, 8);
});

test("messages appear on the card they belong to", () => {
  const html = renderToStaticMarkup(
    <GameLibraryGrid
      state={withState({
        games: [steamGame, gogGame],
        notices: { "steam-553850": "Asked Steam to start Helldivers 2." },
        gameErrors: { "gog-1": "cmd /C start failed" },
      })}
      actions={actions}
    />,
  );
  const [steamCard, gogCard] = html.split("<article").slice(1) as [string, string];
  assert.match(steamCard, /role="status"[^>]*>Asked Steam to start Helldivers 2\./);
  assert.doesNotMatch(steamCard, /cmd \/C start failed/);
  assert.match(gogCard, /role="alert"[^>]*>cmd \/C start failed/);
  assert.doesNotMatch(gogCard, /Asked Steam/);
});

test("a card's problem is not shown twice when its matrix is open", () => {
  const html = renderToStaticMarkup(
    <GameLibraryGrid
      state={withState({ games: [gogGame], expanded: { "gog-1": true }, matrices: { "gog-1": matrixFor(gogGame) }, gameErrors: { "gog-1": "refused" } })}
      actions={actions}
    />,
  );
  assert.equal((html.match(/>refused</g) ?? []).length, 1);
});

// ---------------------------------------------------------------------------
// The page
// ---------------------------------------------------------------------------

test("without a backend the page says scanning needs the desktop app and offers no scan", () => {
  const html = renderToStaticMarkup(<GameLibrary api={null} />);
  assert.match(html, /Browser preview: scanning needs the desktop app, so no games are listed\./);
  assert.match(tagContaining(html, ">Scan for games<"), /disabled=""/);
  assert.doesNotMatch(html, /class="game-library-grid"/);
  // The development-data cards are labelled as such, apart from real games.
  assert.match(html, /Planned game adapters/);
  assert.match(html, /Development data: nothing has been tested yet\./);
  assert.match(html, /Supported and tested/);
});

test("with a backend the scan button is live and the page explains scans only read", () => {
  const html = renderToStaticMarkup(<GameLibrary api={{ list: async () => [], scan: async () => ({ games: [], added: 0, updated: 0, returned: 0, missing: 0 }), matrix: async () => matrixFor(steamGame), tick: async () => matrixFor(steamGame), play: async () => {} }} />);
  assert.doesNotMatch(tagContaining(html, ">Scan for games<"), /disabled=""/);
  assert.match(html, /Scans only read: nothing is installed, moved or changed\./);
});
