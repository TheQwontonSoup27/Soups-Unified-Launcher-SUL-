// A backend for the folders-and-builds panel that lives in memory. It is NOT the core: it only keeps the shape of
// the real answers and refuses the few things a test needs refused, with plain-text errors like a failing command.
// What the core really refuses is tested in `crates/sul-core/src/manager.rs` and `src-tauri/src/ipc_tests.rs`.
import type { BuildRequest, BuildView, BuiltView, ManagerApi, ManagerView, RootKind, RootView, VerifiedView } from "../src/lib/manager";

export const EMPTY_VIEW: ManagerView = { roots: [], builds: [], slots: [], retired: [], interrupted: [] };

export function root(id: string, label: string, kind: RootKind = "builds", extra: Partial<RootView> = {}): RootView {
  return { id, label, path: `D:\\SUL\\${label}`, kind, present: true, marker_ok: true, free_bytes: 500_000_000_000, ...extra };
}

export function build(id: string, gameId: string, slot: string, extra: Partial<BuildView> = {}): BuildView {
  return {
    id,
    game_id: gameId,
    slot,
    variant: slot === "original" || slot === "optimized" || slot === "legacy" || slot === "experimental" ? slot : { custom: slot },
    location: { kind: "managed", root_id: "main", dir: id },
    state: "ready",
    active: false,
    ...extra,
  };
}

export interface FakeCall {
  op: string;
  args: unknown[];
}

export function fakeManager(initial: Partial<ManagerView> = {}) {
  const calls: FakeCall[] = [];
  let view: ManagerView = { ...EMPTY_VIEW, ...initial };
  const failures = new Map<string, string>();
  /** Build ids whose check comes back as changed. */
  const drifted = new Set<string>();
  let gate: Promise<void> | null = null;
  let next = 1;

  const enter = async (op: string, ...args: unknown[]) => {
    calls.push({ op, args });
    if (gate) await gate;
    const failure = failures.get(op);
    if (failure !== undefined) {
      failures.delete(op);
      throw failure; // `invoke` rejects with a plain string
    }
  };
  const snapshot = (): ManagerView => structuredClone(view);

  const makeBuild = (req: BuildRequest, location: BuildView["location"]): BuiltView => {
    const slot = typeof req.variant === "string" ? req.variant : req.variant.custom;
    const id = `${req.gameId}-${slot}-${next++}`;
    view = { ...view, builds: [...view.builds, build(id, req.gameId, slot, { location })] };
    return { build_id: id, view: snapshot() };
  };

  const api: ManagerApi = {
    async view() {
      await enter("view");
      return snapshot();
    },
    async addRoot(id, label, path, kind) {
      await enter("addRoot", id, label, path, kind);
      if (view.roots.some((r) => r.id === id)) throw `a library folder with the id “${id}” already exists`;
      view = { ...view, roots: [...view.roots, root(id, label, kind, { path })] };
      return snapshot();
    },
    async relinkRoot(id, path) {
      await enter("relinkRoot", id, path);
      view = { ...view, roots: view.roots.map((r) => (r.id === id ? { ...r, path, present: true, marker_ok: true } : r)) };
      return snapshot();
    },
    async forgetRoot(id) {
      await enter("forgetRoot", id);
      const held = view.builds.filter((b) => b.location.kind === "managed" && b.location.root_id === id).length;
      if (held > 0) throw `the folder still holds ${held} build${held === 1 ? "" : "s"}, so SUL will not stop listing it`;
      view = { ...view, roots: view.roots.filter((r) => r.id !== id) };
      return snapshot();
    },
    async importBuild(req, rootId) {
      await enter("importBuild", req, rootId);
      return makeBuild(req, { kind: "managed", root_id: rootId, dir: "copy" });
    },
    async registerInPlace(req) {
      await enter("registerInPlace", req);
      return makeBuild(req, { kind: "external_install", path: "D:\\Games\\original" });
    },
    async verify(buildId): Promise<VerifiedView> {
      await enter("verify", buildId);
      const bad = drifted.has(buildId);
      return {
        check: { state: bad ? "failed" : "ready", report: { missing: [], modified: bad ? ["data/a.bin"] : [], extra: [] } },
        view: snapshot(),
      };
    },
    async activate(buildId) {
      await enter("activate", buildId);
      const target = view.builds.find((b) => b.id === buildId);
      if (!target) throw "no such build";
      const before = view.slots.find((s) => s.game_id === target.game_id && s.slot === target.slot);
      const history = before ? [...before.history, before.build_id] : [];
      view = {
        ...view,
        builds: view.builds.map((b) => (b.game_id === target.game_id && b.slot === target.slot ? { ...b, active: b.id === buildId } : b)),
        slots: [...view.slots.filter((s) => s !== before), { game_id: target.game_id, slot: target.slot, build_id: buildId, history }],
      };
      return snapshot();
    },
    async rollback(gameId, slot) {
      await enter("rollback", gameId, slot);
      const s = view.slots.find((x) => x.game_id === gameId && x.slot === slot);
      if (!s || s.history.length === 0) throw "there is no earlier build to go back to";
      const back = s.history[s.history.length - 1]!;
      view = {
        ...view,
        builds: view.builds.map((b) => (b.game_id === gameId && b.slot === slot ? { ...b, active: b.id === back } : b)),
        slots: view.slots.map((x) => (x === s ? { ...x, build_id: back, history: s.history.slice(0, -1) } : x)),
      };
      return snapshot();
    },
  };

  return {
    api,
    calls,
    /** The calls that change something (everything but `view` and `verify`). */
    changes: () => calls.filter((c) => c.op !== "view" && c.op !== "verify"),
    failNext(op: string, message: string) {
      failures.set(op, message);
    },
    drift(buildId: string) {
      drifted.add(buildId);
    },
    /** Hold every call open until `release()`. */
    hold(): () => void {
      let release!: () => void;
      gate = new Promise<void>((resolve) => (release = resolve));
      return () => {
        gate = null;
        release();
      };
    },
    current: () => snapshot(),
  };
}
