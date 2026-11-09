// A backend for the Tools tab that lives in memory. It is NOT the core: it keeps the shape of the real answers and
// refuses the few things a test needs refused, with plain-text errors like a failing command. What the core really
// refuses (a changed tool file, a workspace that is not a finished run, a program that fails to start) is tested in
// `crates/sul-core/src/builder/tools.rs`, `crates/sul-core/src/manager/tooling.rs` and `src-tauri/src/ipc_tests.rs`.
import type {
  KnownTool,
  RunningTool,
  StoredRunRecords,
  ToolRecord,
  ToolRunRecord,
  ToolRunRequest,
  ToolStatus,
  ToolsApi,
  ToolsView,
  WorkspaceView,
} from "../src/lib/tools";
import { modRoot } from "./mods-fake";

export { modRoot };

export const HEX = "ab".repeat(32);
export const OTHER_HEX = "cd".repeat(32);

export const EMPTY_TOOLS_VIEW: ToolsView = { tools: [], known: [], workspaces: [], mod_roots: [], running: [], run_errors: [] };

export function aTool(id = "audio", extra: Partial<ToolRecord> = {}): ToolRecord {
  return {
    id,
    display_name: id.toUpperCase(),
    version: "1.0",
    exe_path: `D:\\Tools\\${id}\\${id}.exe`,
    exe_sha256: HEX,
    exe_size: 2048,
    registered_unix: 1_700_000_000,
    redistribution: "unknown",
    license_note: "",
    source_note: "",
    history: [],
    ...extra,
  };
}

export function aWorkspace(recipeId = "loud", runId = "run-1", rootId = "modlib", name = "Loud"): WorkspaceView {
  return { recipe_id: recipeId, recipe_name: name, root_id: rootId, run_id: runId };
}

export function aKnown(id = "hd2-audio-modder", extra: Partial<KnownTool> = {}): KnownTool {
  return { id, name: `Tool ${id}`, status: "not_investigated", finding: "Read a public page.", not_established: "That it works.", ...extra };
}

export function aRunRecord(extra: Partial<ToolRunRecord> = {}): ToolRunRecord {
  return {
    tool_id: "audio",
    tool_version: "1.0",
    tool_sha256: HEX,
    args: [],
    env_names: [],
    settings: {},
    started_unix: 1_700_000_000,
    finished_unix: 1_700_000_005,
    exit_code: 0,
    success: true,
    added: [{ path: "data/new.bin", sha256: OTHER_HEX, size: 12 }],
    changed: [],
    removed: [],
    comparison_error: null,
    limits: "Only files inside the workspace folder are compared. SUL cannot confine a program, so anything it wrote elsewhere is not in this record.",
    ...extra,
  };
}

export interface FakeCall {
  op: string;
  args: unknown[];
}

export function fakeTools(initial: Partial<ToolsView> = {}) {
  const calls: FakeCall[] = [];
  let view: ToolsView = { ...EMPTY_TOOLS_VIEW, ...initial };
  const failures = new Map<string, string>();
  let gate: Promise<void> | null = null;
  const fileState = new Map<string, ToolStatus>();
  const finishing = new Map<number, ToolRunRecord>();
  let planProblems: string[] = [];
  let stored: StoredRunRecords = { records: [], total: 0, unreadable: [] };
  let nextRun = 1;

  const enter = async (op: string, ...args: unknown[]) => {
    calls.push({ op, args });
    if (gate) await gate;
    const failure = failures.get(op);
    if (failure !== undefined) {
      failures.delete(op);
      throw failure; // `invoke` rejects with a plain string
    }
  };
  const snapshot = (): ToolsView => structuredClone(view);
  const toolOf = (id: string): ToolRecord => {
    const t = view.tools.find((x) => x.id === id);
    if (!t) throw `no tool “${id}” is recorded`;
    return t;
  };
  const statusOf = (id: string): ToolStatus => fileState.get(id) ?? { state: "matches" };
  const workspaceOf = (r: ToolRunRequest) => {
    const w = view.workspaces.find((x) => x.recipe_id === r.workspace.recipe_id && x.root_id === r.workspace.root_id && x.run_id === r.workspace.run_id);
    if (!w) throw `“${r.workspace.run_id}” is not a finished run of “${r.workspace.recipe_id}” in “${r.workspace.root_id}”`;
    return w;
  };
  const plan = (r: ToolRunRequest) => {
    const t = toolOf(r.tool_id);
    workspaceOf(r);
    const problems = [...planProblems];
    if (statusOf(t.id).state !== "matches") problems.push("the tool's file is not the one that was recorded, so it will not be started");
    if (view.running.length >= 4) problems.push("4 tools are already running; wait for one to finish");
    if (view.running.some((x) => x.workspace.recipe_id === r.workspace.recipe_id && x.workspace.root_id === r.workspace.root_id && x.workspace.run_id === r.workspace.run_id)) {
      problems.push("a tool is already running on this run's files; wait for it to finish");
    }
    return { exe: t.exe_path, args: [...r.args], cwd: `D:\\SUL\\workspaces\\${r.workspace.run_id}\\files`, tool_version: t.version, tool_sha256: t.exe_sha256, problems };
  };

  const api: ToolsApi = {
    async view() {
      await enter("view");
      return snapshot();
    },
    async register(request) {
      await enter("register", request);
      if (view.tools.some((t) => t.id === request.id)) throw `a tool “${request.id}” is already recorded`;
      const t = aTool(request.id, { display_name: request.display_name, version: request.version, exe_path: request.exe_path, redistribution: request.redistribution, license_note: request.license_note, source_note: request.source_note });
      view = { ...view, tools: [...view.tools, t] };
      return snapshot();
    },
    async check(id) {
      await enter("check", id);
      toolOf(id);
      return { id, status: statusOf(id) };
    },
    async acceptChange(id, newVersion, seenSha256) {
      await enter("acceptChange", id, newVersion, seenSha256);
      const t = toolOf(id);
      const s = statusOf(id);
      if (s.state === "matches") throw "the tool's file is the one that was recorded, so there is nothing to accept";
      if (s.state === "missing") throw "the tool's file is missing, so there is nothing to accept";
      if (s.exe_sha256 !== seenSha256) throw "the file is no longer the one you were shown; check it again";
      view = {
        ...view,
        tools: view.tools.map((x) =>
          x.id === id ? { ...t, version: newVersion, exe_sha256: s.exe_sha256, exe_size: s.exe_size, history: [...t.history, { version: t.version, exe_sha256: t.exe_sha256, exe_size: t.exe_size, recorded_unix: 1_700_000_100 }] } : x,
        ),
      };
      fileState.delete(id);
      return snapshot();
    },
    async planRun(request) {
      await enter("planRun", request);
      return plan(request);
    },
    async startRun(request) {
      await enter("startRun", request);
      const p = plan(request);
      if (p.problems.length > 0) throw `the tool cannot be run: ${p.problems.join("; ")}`;
      const run: RunningTool = { run: nextRun, tool_id: request.tool_id, pid: 4000 + nextRun, started_unix: 1_700_000_200, args: request.args, workspace: request.workspace };
      nextRun += 1;
      view = { ...view, running: [...view.running, run] };
      return { run, view: snapshot() };
    },
    async pollRun(run) {
      await enter("pollRun", run);
      const r = view.running.find((x) => x.run === run);
      if (!r) throw `no tool run ${run} is being followed (SUL does not follow a program across a restart of the app)`;
      const record = finishing.get(run);
      if (!record) return { state: "running", pid: r.pid };
      finishing.delete(run);
      view = { ...view, running: view.running.filter((x) => x.run !== run) };
      return { state: "finished", record, saved_as: `run-${run}.json`, save_error: null, view: snapshot() };
    },
    async runRecords(workspace) {
      await enter("runRecords", workspace);
      if (!view.workspaces.some((w) => w.recipe_id === workspace.recipe_id && w.root_id === workspace.root_id && w.run_id === workspace.run_id)) {
        throw `“${workspace.run_id}” is not a finished run of “${workspace.recipe_id}” in “${workspace.root_id}”`;
      }
      return structuredClone(stored);
    },
  };

  return {
    api,
    calls,
    /** The calls that change something or start a program. Everything else only reads. */
    changes: () => calls.filter((c) => ["register", "acceptChange", "startRun"].includes(c.op)),
    failNext(op: string, message: string) {
      failures.set(op, message);
    },
    /** What the next checks say about a tool's file. */
    setFile(id: string, s: ToolStatus) {
      fileState.set(id, s);
    },
    /** Problems every next plan has, as if the core found them. */
    setPlanProblems(p: string[]) {
      planProblems = p;
    },
    /** The next poll of this run says it finished, with this record. */
    finish(run: number, record: ToolRunRecord = aRunRecord()) {
      finishing.set(run, record);
    },
    setStored(s: StoredRunRecords) {
      stored = s;
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
