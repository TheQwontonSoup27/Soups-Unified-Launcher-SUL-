// A backend for the Play panel that lives in memory. It is NOT the core: it keeps the shape of the real answers and
// mirrors the few refusals a test needs (no such build, no adapter, not ready, already running). What the core really
// refuses, and when a program is really started, is tested in `crates/sul-core/src/runtime.rs`,
// `crates/sul-core/src/manager/playing.rs` and `src-tauri/src/ipc_tests.rs`.
import type { GameLaunch, LaunchRecord, LaunchView, NotStarted, PlayApi, PlayBuild, Refreshed, Started } from "../src/lib/playing";

/** The sentences the core gives for each refusal (see `NotStarted::explain` in `crates/sul-core/src/runtime.rs`). */
export const NOT_STARTED_TEXT: Record<NotStarted["reason"], string> = {
  no_such_build: "SUL has no build with that id.",
  already_running: "This game is already running. SUL starts one copy of a game at a time.",
  build_not_ready: "The build is not ready. Verify or repair it first; SUL only starts a build that has been checked.",
  no_adapter: "There is no adapter registered for this game, so SUL does not know how to start it.",
  adapter_refused: "The game's adapter could not make a launch plan.",
  plan_rejected: "The launch plan was refused as unsafe or unusable.",
  program_changed: "The game's program no longer matches what SUL recorded, so it was not started.",
  could_not_start: "Windows could not start the program.",
};

export function aBuild(id = "gog-1-experimental-1", extra: Partial<PlayBuild> = {}): PlayBuild {
  return { build_id: id, game_id: "gog-1", slot: "experimental", state: "ready", managed: true, active: true, has_adapter: true, ...extra };
}

export function aView(builds: PlayBuild[] = [aBuild()], extra: Partial<LaunchView> = {}): LaunchView {
  const ids = [...new Set(builds.map((b) => b.game_id))];
  const games: GameLaunch[] = ids.map((game_id) => ({ game_id, state: { state: "not_running" } }));
  return { builds, games, history: [], ...extra };
}

export function aRefusal(buildId: string, why: NotStarted, id = "launch-refused"): LaunchRecord {
  return {
    launch_id: id,
    game_id: "gog-1",
    build_id: buildId,
    started_unix: 1_700_000_000,
    ended_unix: 1_700_000_000,
    outcome: { kind: "not_started", why, explanation: NOT_STARTED_TEXT[why.reason] },
    log_file: null,
    warnings: [],
  };
}

export interface FakeCall {
  op: string;
  args: unknown[];
}

export function fakePlay(initial: LaunchView = aView()) {
  const calls: FakeCall[] = [];
  let view: LaunchView = structuredClone(initial);
  const failures = new Map<string, string>();
  let gate: Promise<void> | null = null;
  let launches = 0;
  /** Refreshes a running game gets before it ends, by game id, and the exit code it ends with. */
  const endings = new Map<string, { after: number; code: number }>();
  let warnings: string[] = [];
  let log: string | null = null;

  const enter = async (op: string, ...args: unknown[]) => {
    calls.push({ op, args });
    if (gate) await gate;
    const failure = failures.get(op);
    if (failure !== undefined) {
      failures.delete(op);
      throw failure; // `invoke` rejects with a plain string
    }
  };
  const snapshot = (): LaunchView => structuredClone(view);
  const setGame = (gameId: string, state: GameLaunch["state"]) => {
    view = { ...view, games: view.games.map((g) => (g.game_id === gameId ? { ...g, state } : g)) };
  };
  const remember = (r: LaunchRecord) => {
    view = { ...view, history: [r, ...view.history] };
  };

  const api: PlayApi = {
    async view() {
      await enter("view");
      return snapshot();
    },
    async start(buildId, captureOutput): Promise<Started> {
      await enter("start", buildId, captureOutput);
      launches += 1;
      const launch_id = `launch-${launches}`;
      const refuse = (why: NotStarted): Started => {
        const record = aRefusal(buildId, why, launch_id);
        const b = view.builds.find((x) => x.build_id === buildId);
        if (b) setGame(b.game_id, { state: "last_not_started", explanation: record.outcome.kind === "not_started" ? record.outcome.explanation : "" });
        remember(record);
        return { record, view: snapshot() };
      };
      const b = view.builds.find((x) => x.build_id === buildId);
      if (!b) return refuse({ reason: "no_such_build" });
      const running = view.games.find((g) => g.game_id === b.game_id)?.state;
      if (running?.state === "running") return refuse({ reason: "already_running", build_id: running.build_id });
      if (b.state !== "ready") return refuse({ reason: "build_not_ready", state: b.state });
      if (!b.has_adapter) return refuse({ reason: "no_adapter" });
      const record: LaunchRecord = {
        launch_id,
        game_id: b.game_id,
        build_id: buildId,
        started_unix: 1_700_000_000,
        ended_unix: null,
        outcome: { kind: "running", pid: 4242 },
        log_file: null,
        warnings,
      };
      setGame(b.game_id, { state: "running", pid: 4242, build_id: buildId, since_unix: 1_700_000_000 });
      return { record, view: snapshot() };
    },
    async refresh(): Promise<Refreshed> {
      await enter("refresh");
      const finished: LaunchRecord[] = [];
      for (const g of view.games) {
        if (g.state.state !== "running") continue;
        const end = endings.get(g.game_id) ?? { after: 0, code: 0 };
        if (end.after > 0) {
          endings.set(g.game_id, { ...end, after: end.after - 1 });
          continue;
        }
        const record: LaunchRecord = {
          launch_id: `launch-${launches}`,
          game_id: g.game_id,
          build_id: g.state.build_id,
          started_unix: g.state.since_unix,
          ended_unix: 1_700_000_100,
          outcome: { kind: "exited", code: end.code, success: end.code === 0, explanation: end.code === 0 ? null : "The program ended soon after it started. SUL cannot tell why." },
          log_file: log,
          warnings: [],
        };
        setGame(g.game_id, { state: "last_exited", code: end.code, success: end.code === 0, ended_unix: 1_700_000_100 });
        remember(record);
        finished.push(record);
      }
      return { finished, view: snapshot() };
    },
  };

  return {
    api,
    calls,
    /** Change a build behind the page's back, as if the core's state moved on after the page last asked. */
    change(buildId: string, patch: Partial<PlayBuild>) {
      view = { ...view, builds: view.builds.map((b) => (b.build_id === buildId ? { ...b, ...patch } : b)) };
    },
    /** The calls that start something. Everything else only reads. */
    starts: () => calls.filter((c) => c.op === "start"),
    failNext(op: string, message: string) {
      failures.set(op, message);
    },
    /** The running game ends after this many refreshes with this code. */
    endsAfter(gameId: string, polls: number, code = 0) {
      endings.set(gameId, { after: polls, code });
    },
    /** What the next started launch reports as warnings, and where the next ended launch says it kept output. */
    warnWith(w: string[]) {
      warnings = w;
    },
    keepLog(path: string | null) {
      log = path;
    },
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
