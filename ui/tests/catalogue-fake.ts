// A backend for the catalogue and the game details that lives in memory. It is NOT the core: it keeps the shape of the real
// answers and mirrors the few rules a test needs (names are trimmed and a repeat that differs only in ASCII case is dropped;
// a blank or over-long name, a path that is not an image file and a blank path are refused with the core's sentences).
// What the core really enforces is tested in `crates/sul-core/src/catalogue.rs`, `crates/sul-core/src/manager/cataloguing.rs`
// and `src-tauri/src/ipc_tests.rs`.
import type { CatalogueApi, CatalogueView, CategoryView, Edited, EntryView, GameDetail } from "../src/lib/catalogue";
import type { GameView } from "../src/lib/library";

const NO_ADAPTER = "No adapter is registered for this game, so launching and mod support are not available and nothing about it has been tested.";
const NO_BUILD = "SUL has no build of this game yet. Import or register an install to manage it.";

const lowerAscii = (s: string) => s.replace(/[A-Z]/g, (c) => c.toLowerCase());

export function aDetail(game: GameView, extra: Partial<GameDetail> = {}): GameDetail {
  return {
    game_id: game.id,
    display_name: game.name,
    categories: [],
    artwork: null,
    builds: [],
    adapter: null,
    capability: "not_investigated",
    capability_label: "Not investigated",
    launch: { state: "not_running" },
    notes: [NO_ADAPTER, NO_BUILD],
    ...extra,
  };
}

export interface FakeCall {
  op: string;
  args: unknown[];
}

export function fakeCatalogue(games: GameView[], initial: Record<string, string[]> = {}) {
  const calls: FakeCall[] = [];
  const categories = new Map<string, string[]>(Object.entries(initial).map(([id, c]) => [id, [...c]]));
  const artwork = new Map<string, string>();
  const overrides = new Map<string, Partial<GameDetail>>();
  const failures = new Map<string, string>();
  const heldDetails = new Map<string, Promise<void>>();
  let gate: Promise<void> | null = null;

  const view = (): CatalogueView => {
    const names: string[] = [];
    for (const g of games) {
      for (const c of categories.get(g.id) ?? []) if (!names.some((n) => lowerAscii(n) === lowerAscii(c))) names.push(c);
    }
    names.sort((a, b) => (lowerAscii(a) < lowerAscii(b) ? -1 : lowerAscii(a) > lowerAscii(b) ? 1 : 0));
    const cats: CategoryView[] = names.map((name) => {
      const game_ids = games.filter((g) => (categories.get(g.id) ?? []).some((c) => lowerAscii(c) === lowerAscii(name))).map((g) => g.id);
      return { name, count: game_ids.length, game_ids };
    });
    const entries: EntryView[] = games.filter((g) => (categories.get(g.id) ?? []).length > 0).map((g) => ({ game_id: g.id, categories: [...(categories.get(g.id) ?? [])] }));
    return { categories: cats, entries };
  };
  const detail = (id: string): GameDetail => {
    const g = games.find((x) => x.id === id);
    if (!g) throw `the library has no game "${id}"`;
    return structuredClone(aDetail(g, { categories: [...(categories.get(id) ?? [])], artwork: artwork.get(id) ?? null, ...(overrides.get(id) ?? {}) }));
  };
  const enter = async (op: string, ...args: unknown[]) => {
    calls.push({ op, args });
    // A failure belongs to the next call of that kind, taken when the call is made; it is thrown after any wait, so a call
    // that is held and then fails is a late failure.
    const failure = failures.get(op);
    if (failure !== undefined) failures.delete(op);
    if (gate) await gate;
    if (op === "detail") {
      const held = heldDetails.get(String(args[0]));
      if (held) {
        heldDetails.delete(String(args[0]));
        await held;
      }
    }
    if (failure !== undefined) throw failure; // `invoke` rejects with a plain string
  };
  const edited = (id: string): Edited => ({ detail: detail(id), view: view() });

  const api: CatalogueApi = {
    async view() {
      await enter("view");
      return view();
    },
    async detail(gameId) {
      await enter("detail", gameId);
      return detail(gameId);
    },
    async setCategories(gameId, list) {
      await enter("setCategories", gameId, list);
      const clean: string[] = [];
      for (const raw of list) {
        const c = raw.trim();
        if (c === "" || c.length > 40) throw `a category must be 1 to 40 characters with no control characters: "${raw}"`;
        if (!clean.some((x) => lowerAscii(x) === lowerAscii(c))) clean.push(c);
      }
      if (clean.length > 20) throw "at most 20 categories per game";
      if (clean.length === 0) categories.delete(gameId);
      else categories.set(gameId, clean);
      return edited(gameId);
    },
    async setArtwork(gameId, path) {
      await enter("setArtwork", gameId, path);
      if (path === null) {
        artwork.delete(gameId);
        return edited(gameId);
      }
      if (path.trim() === "") throw "type the path of an image file, or clear the artwork";
      if (!/\.(png|jpe?g|webp)$/i.test(path.trim()) || path.includes("missing")) throw `artwork must be a .png, .jpg, .jpeg or .webp file: ${path}`;
      // The core stores the resolved path; on Windows that is `\\?\C:\…`.
      artwork.set(gameId, path.trim().startsWith("C:\\") ? `\\\\?\\${path.trim()}` : path.trim());
      return edited(gameId);
    },
  };

  return {
    api,
    calls,
    /** The calls that change something. Everything else only reads. */
    edits: () => calls.filter((c) => c.op === "setCategories" || c.op === "setArtwork"),
    /** Change what the next `detail` for a game says (a build, an adapter, a launch state, notes). */
    setDetail(gameId: string, patch: Partial<GameDetail>) {
      overrides.set(gameId, { ...(overrides.get(gameId) ?? {}), ...patch });
    },
    /** Put categories on games behind the page's back. */
    give(gameId: string, list: string[]) {
      categories.set(gameId, [...list]);
    },
    failNext(op: string, message: string) {
      failures.set(op, message);
    },
    /** The next `detail` call for this game waits until the returned function is called. */
    holdDetail(gameId: string): () => void {
      let release!: () => void;
      heldDetails.set(gameId, new Promise<void>((resolve) => (release = resolve)));
      return release;
    },
    hold(): () => void {
      let release!: () => void;
      gate = new Promise<void>((resolve) => (release = resolve));
      return () => {
        gate = null;
        release();
      };
    },
    view,
    detail,
  };
}
