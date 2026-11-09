import { invoke, isTauri } from "@tauri-apps/api/core";
import { messageOf } from "./library";
import { BUILD_STATE_LABEL, type BuildState } from "./manager";

// ---------------------------------------------------------------------------
// Wire types. They mirror `sul_core::runtime` and the `playing` half of `sul_core::manager` (serde). The enums are
// tagged: `state` for `LaunchState`, `kind` for `Outcome`, `reason` for `NotStarted`. `src-tauri/src/ipc_tests.rs`
// pins the field names from the Rust side.
// ---------------------------------------------------------------------------

export interface PlayBuild {
  build_id: string;
  game_id: string;
  slot: string;
  state: BuildState;
  /** A copy SUL manages (true) or an install that stays where it is (false). */
  managed: boolean;
  active: boolean;
  /** Without an adapter for the game, nothing can be started. */
  has_adapter: boolean;
}

export type LaunchState =
  | { state: "not_running" }
  | { state: "running"; pid: number; build_id: string; since_unix: number }
  | { state: "last_exited"; code: number | null; success: boolean; ended_unix: number }
  | { state: "last_not_started"; explanation: string };

export interface GameLaunch {
  game_id: string;
  state: LaunchState;
}

export type NotStarted =
  | { reason: "no_such_build" }
  | { reason: "already_running"; build_id: string }
  | { reason: "build_not_ready"; state: BuildState }
  | { reason: "no_adapter" }
  | { reason: "adapter_refused"; why: string }
  | { reason: "plan_rejected"; why: string }
  | { reason: "program_changed"; why: string }
  | { reason: "could_not_start"; why: string };

export type Outcome =
  | { kind: "running"; pid: number }
  | { kind: "exited"; code: number | null; success: boolean; explanation: string | null }
  | { kind: "not_started"; why: NotStarted; explanation: string };

export interface LaunchRecord {
  launch_id: string;
  game_id: string;
  build_id: string;
  started_unix: number;
  ended_unix: number | null;
  outcome: Outcome;
  /** Where the program's output was written, when it was asked for and there was some. */
  log_file: string | null;
  warnings: string[];
}

export interface LaunchView {
  builds: PlayBuild[];
  games: GameLaunch[];
  /** Newest first. */
  history: LaunchRecord[];
}

export interface Started {
  record: LaunchRecord;
  view: LaunchView;
}

export interface Refreshed {
  finished: LaunchRecord[];
  view: LaunchView;
}

export interface PlayApi {
  view(): Promise<LaunchView>;
  /** A refusal comes back as a record with `outcome.kind === "not_started"`, not as a rejection. */
  start(buildId: string, captureOutput: boolean): Promise<Started>;
  refresh(): Promise<Refreshed>;
}

const UNEXPECTED = "The app sent launch data in a shape this page does not understand, so it was not used.";

const isObject = (v: unknown): v is Record<string, unknown> => typeof v === "object" && v !== null;
const arrays = (o: Record<string, unknown>, keys: string[]) => keys.every((k) => Array.isArray(o[k]));

/** Whether the backend sent a view of the shape the page reads. A wrong shape would otherwise crash the page. */
export function isLaunchView(v: unknown): v is LaunchView {
  return isObject(v) && arrays(v, ["builds", "games", "history"]);
}

export const isLaunchRecord = (v: unknown): v is LaunchRecord =>
  isObject(v) && typeof v.launch_id === "string" && typeof v.build_id === "string" && isObject(v.outcome) && typeof v.outcome.kind === "string" && Array.isArray(v.warnings);

function expectView(v: unknown): LaunchView {
  if (!isLaunchView(v)) throw new Error(UNEXPECTED);
  return v;
}

function expectStarted(v: unknown): Started {
  if (!isObject(v) || !isLaunchRecord(v.record) || !isLaunchView(v.view)) throw new Error(UNEXPECTED);
  return v as unknown as Started;
}

function expectRefreshed(v: unknown): Refreshed {
  if (!isObject(v) || !Array.isArray(v.finished) || !v.finished.every(isLaunchRecord) || !isLaunchView(v.view)) throw new Error(UNEXPECTED);
  return v as unknown as Refreshed;
}

/** The real backend, or null in a plain browser where there is none. */
export function tauriPlayApi(): PlayApi | null {
  if (!isTauri()) return null;
  return {
    view: async () => expectView(await invoke<unknown>("launch_view")),
    start: async (buildId, captureOutput) => expectStarted(await invoke<unknown>("launch_start", { buildId, captureOutput })),
    refresh: async () => expectRefreshed(await invoke<unknown>("launch_refresh")),
  };
}

// ---------------------------------------------------------------------------
// Words and small rules the page shows (the core enforces the real ones)
// ---------------------------------------------------------------------------

/** UTC, to the minute, so the same record reads the same everywhere. */
export function formatUtc(unix: number): string {
  return `${new Date(unix * 1000).toISOString().slice(0, 16).replace("T", " ")} UTC`;
}

export function describeLaunchState(s: LaunchState): string {
  switch (s.state) {
    case "not_running":
      return "Not running";
    case "running":
      return `Running (process ${s.pid}) since ${formatUtc(s.since_unix)}`;
    case "last_exited":
      return s.success ? `Last run ended normally at ${formatUtc(s.ended_unix)}` : `Last run ended with ${s.code === null ? "no exit code" : `exit code ${s.code}`} at ${formatUtc(s.ended_unix)}`;
    case "last_not_started":
      return "Last start was refused";
  }
}

/** One line for a record. A refusal gives the core's own sentence. */
export function describeRecord(r: LaunchRecord): string {
  const o = r.outcome;
  switch (o.kind) {
    case "running":
      return `Started build ${r.build_id} (process ${o.pid}).`;
    case "exited":
      return o.success ? `Build ${r.build_id} ended normally.` : `Build ${r.build_id} ended with ${o.code === null ? "no exit code" : `exit code ${o.code}`}.${o.explanation ? ` ${o.explanation}` : ""}`;
    case "not_started":
      return `Build ${r.build_id} was not started: ${o.explanation}`;
  }
}

/** Why the Play button is held for a build, or null when it can be pressed. The core decides again when it is pressed. */
export function holdReason(b: PlayBuild, view: LaunchView): string | null {
  if (!b.has_adapter) return "SUL has no adapter for this game, so it does not know how to start it.";
  if (b.state !== "ready") return `Not ready: ${BUILD_STATE_LABEL[b.state].toLowerCase()}. Check it on the builds list first.`;
  const running = view.games.find((g) => g.game_id === b.game_id)?.state;
  if (running?.state === "running") return running.build_id === b.build_id ? "Running." : `Build ${running.build_id} of this game is already running.`;
  return null;
}

export const isAnythingRunning = (v: LaunchView | null): boolean => v?.games.some((g) => g.state.state === "running") ?? false;

/** Most history rows shown. The core keeps the last few dozen. */
export const MAX_HISTORY_SHOWN = 25;
export const MAX_FINISHED_SHOWN = 10;

// ---------------------------------------------------------------------------
// The controller
// ---------------------------------------------------------------------------

export interface PlayState {
  loaded: boolean;
  view: LaunchView | null;
  /** What is running, or null. */
  busy: string | null;
  error: string | null;
  notice: string | null;
  /** Warnings from the latest start that went ahead. */
  warnings: string[];
  /** Launches that ended while this page was open, newest first. */
  finished: LaunchRecord[];
}

export const EMPTY_PLAY: PlayState = { loaded: false, view: null, busy: null, error: null, notice: null, warnings: [], finished: [] };

export class PlayController {
  private state: PlayState = EMPTY_PLAY;
  private readonly listeners = new Set<() => void>();
  private polling = false;

  /** `api` is null when there is no backend (a plain browser). */
  constructor(private readonly api: PlayApi | null) {}

  get hasBackend(): boolean {
    return this.api !== null;
  }

  subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  };

  getSnapshot = (): PlayState => this.state;

  private set(patch: Partial<PlayState>): void {
    this.state = { ...this.state, ...patch };
    for (const listener of this.listeners) listener();
  }

  async load(): Promise<void> {
    if (!this.api) {
      this.set({ loaded: true });
      return;
    }
    try {
      this.set({ view: await this.api.view(), loaded: true, error: null });
    } catch (error) {
      this.set({ loaded: true, error: messageOf(error) });
    }
  }

  /**
   * Start a build. The core's refusal (no adapter, not ready, already running, a program that changed) comes back as a
   * record and is shown as the error, in the core's own words. Nothing is started by the page itself.
   */
  async start(buildId: string, captureOutput: boolean): Promise<boolean> {
    const api = this.api;
    if (!api || this.state.busy !== null) return false;
    if (buildId.trim() === "") {
      this.set({ error: "Pick a build to start.", notice: null });
      return false;
    }
    this.set({ busy: "start", error: null, notice: null, warnings: [] });
    try {
      const s = await api.start(buildId, captureOutput);
      if (s.record.outcome.kind === "not_started") {
        this.set({ view: s.view, busy: null, error: s.record.outcome.explanation });
        return false;
      }
      this.set({ view: s.view, busy: null, notice: describeRecord(s.record), warnings: s.record.warnings });
      return true;
    } catch (error) {
      this.set({ busy: null, error: messageOf(error) });
      return false;
    }
  }

  /**
   * Ask whether anything running has ended. Not one of the single operations: the page calls this on a timer, it is
   * skipped while a start or an earlier poll is still open, and a failure is shown without discarding the view.
   */
  async pollRunning(): Promise<void> {
    const api = this.api;
    if (!api || this.polling || this.state.busy !== null || !isAnythingRunning(this.state.view)) return;
    this.polling = true;
    try {
      const r = await api.refresh();
      if (r.finished.length === 0) {
        this.set({ view: r.view });
      } else {
        this.set({
          view: r.view,
          finished: [...r.finished.slice().reverse(), ...this.state.finished].slice(0, MAX_FINISHED_SHOWN),
          notice: r.finished.map(describeRecord).join(" "),
        });
      }
    } catch (error) {
      this.set({ error: messageOf(error) });
    } finally {
      this.polling = false;
    }
  }
}
