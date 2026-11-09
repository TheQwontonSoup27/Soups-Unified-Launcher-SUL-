import { invoke, isTauri } from "@tauri-apps/api/core";
import { messageOf } from "./library";
import { slugId, type RootView } from "./manager";
import { formatSize } from "./mods";
import type { Status } from "../types";

// ---------------------------------------------------------------------------
// Wire types. They mirror `sul_core::builder::tools` and `sul_core::manager::tooling` (serde, snake_case; the status
// words are tagged with `state`). `src-tauri/src/ipc_tests.rs` pins the field names from the Rust side.
// ---------------------------------------------------------------------------

export type Redistribution = "unknown" | "not_permitted" | "permitted";

export interface ToolHistory {
  version: string;
  exe_sha256: string;
  exe_size: number;
  recorded_unix: number;
}

export interface ToolRecord {
  id: string;
  display_name: string;
  /** As the owner typed it. SUL does not read it from the program. */
  version: string;
  exe_path: string;
  exe_sha256: string;
  exe_size: number;
  registered_unix: number;
  redistribution: Redistribution;
  /** The licence or permission in the owner's words. Not checked. */
  license_note: string;
  source_note: string;
  history: ToolHistory[];
}

export interface KnownTool {
  id: string;
  name: string;
  status: Status;
  finding: string;
  not_established: string;
}

export interface WorkspaceRef {
  recipe_id: string;
  root_id: string;
  run_id: string;
}

export interface WorkspaceView extends WorkspaceRef {
  recipe_name: string;
}

export interface RunningTool {
  /** What to poll. Only meaningful until the app closes. */
  run: number;
  tool_id: string;
  pid: number;
  started_unix: number;
  args: string[];
  workspace: WorkspaceRef;
}

export interface ToolsView {
  tools: ToolRecord[];
  known: KnownTool[];
  workspaces: WorkspaceView[];
  mod_roots: RootView[];
  running: RunningTool[];
  run_errors: string[];
}

export interface ToolRegisterRequest {
  id: string;
  display_name: string;
  version: string;
  exe_path: string;
  redistribution: Redistribution;
  license_note: string;
  source_note: string;
}

export interface ToolRunRequest {
  tool_id: string;
  args: string[];
  settings: Record<string, string>;
  workspace: WorkspaceRef;
}

export type ToolStatus = { state: "matches" } | { state: "changed"; exe_sha256: string; exe_size: number } | { state: "missing" };

export interface ToolChecked {
  id: string;
  status: ToolStatus;
}

export interface ToolRunPlan {
  exe: string;
  args: string[];
  cwd: string;
  tool_version: string;
  tool_sha256: string;
  problems: string[];
}

export interface FileEntry {
  path: string;
  sha256: string;
  size: number;
}

export interface ChangedFile {
  path: string;
  before_sha256: string;
  after_sha256: string;
}

export interface ToolRunRecord {
  tool_id: string;
  tool_version: string;
  tool_sha256: string;
  args: string[];
  env_names: string[];
  settings: Record<string, string>;
  started_unix: number;
  finished_unix: number;
  exit_code: number | null;
  success: boolean;
  added: FileEntry[];
  changed: ChangedFile[];
  removed: string[];
  /** When set, the three lists above say nothing: the folder could not be compared. */
  comparison_error: string | null;
  /** Always the same sentence: what the record cannot see. */
  limits: string;
}

export interface ToolStarted {
  run: RunningTool;
  view: ToolsView;
}

export type ToolPolled =
  | { state: "running"; pid: number }
  | { state: "finished"; record: ToolRunRecord; saved_as: string | null; save_error: string | null; view: ToolsView };

export interface StoredRunRecord {
  name: string;
  record: ToolRunRecord;
}

export interface StoredRunRecords {
  records: StoredRunRecord[];
  total: number;
  unreadable: string[];
}

export interface ToolsApi {
  view(): Promise<ToolsView>;
  register(request: ToolRegisterRequest): Promise<ToolsView>;
  check(id: string): Promise<ToolChecked>;
  acceptChange(id: string, newVersion: string, seenSha256: string): Promise<ToolsView>;
  planRun(request: ToolRunRequest): Promise<ToolRunPlan>;
  startRun(request: ToolRunRequest): Promise<ToolStarted>;
  pollRun(run: number): Promise<ToolPolled>;
  runRecords(workspace: WorkspaceRef): Promise<StoredRunRecords>;
}

const UNEXPECTED = "The app sent tool data in a shape this page does not understand, so it was not used.";

const isObject = (v: unknown): v is Record<string, unknown> => typeof v === "object" && v !== null;
const arrays = (o: Record<string, unknown>, keys: string[]) => keys.every((k) => Array.isArray(o[k]));

/** Whether the backend sent a view of the shape the page reads. A wrong shape would otherwise crash the page. */
export function isToolsView(v: unknown): v is ToolsView {
  return isObject(v) && arrays(v, ["tools", "known", "workspaces", "mod_roots", "running", "run_errors"]);
}

const isRunRecord = (v: unknown): v is ToolRunRecord => isObject(v) && typeof v.tool_id === "string" && typeof v.success === "boolean" && arrays(v, ["args", "env_names", "added", "changed", "removed"]) && isObject(v.settings);

const expectView = (v: unknown): ToolsView => {
  if (!isToolsView(v)) throw new Error(UNEXPECTED);
  return v;
};

function expectChecked(v: unknown): ToolChecked {
  if (!isObject(v) || typeof v.id !== "string" || !isObject(v.status)) throw new Error(UNEXPECTED);
  const s = v.status;
  const ok = s.state === "matches" || s.state === "missing" || (s.state === "changed" && typeof s.exe_sha256 === "string" && typeof s.exe_size === "number");
  if (!ok) throw new Error(UNEXPECTED);
  return v as unknown as ToolChecked;
}

function expectPlan(v: unknown): ToolRunPlan {
  if (!isObject(v) || typeof v.exe !== "string" || typeof v.cwd !== "string" || !arrays(v, ["args", "problems"])) throw new Error(UNEXPECTED);
  return v as unknown as ToolRunPlan;
}

function expectStarted(v: unknown): ToolStarted {
  if (!isObject(v) || !isObject(v.run) || typeof v.run.run !== "number" || !isToolsView(v.view)) throw new Error(UNEXPECTED);
  return v as unknown as ToolStarted;
}

function expectPolled(v: unknown): ToolPolled {
  if (!isObject(v)) throw new Error(UNEXPECTED);
  if (v.state === "running" && typeof v.pid === "number") return v as unknown as ToolPolled;
  if (v.state === "finished" && isRunRecord(v.record) && isToolsView(v.view)) return v as unknown as ToolPolled;
  throw new Error(UNEXPECTED);
}

function expectRecords(v: unknown): StoredRunRecords {
  if (!isObject(v) || !arrays(v, ["records", "unreadable"]) || typeof v.total !== "number") throw new Error(UNEXPECTED);
  for (const r of v.records as unknown[]) if (!isObject(r) || typeof r.name !== "string" || !isRunRecord(r.record)) throw new Error(UNEXPECTED);
  return v as unknown as StoredRunRecords;
}

/** The real backend, or null in a plain browser where there is none. */
export function tauriToolsApi(): ToolsApi | null {
  if (!isTauri()) return null;
  return {
    view: async () => expectView(await invoke<unknown>("tools_view")),
    register: async (request) => expectView(await invoke<unknown>("tools_register", { request })),
    check: async (id) => expectChecked(await invoke<unknown>("tools_check", { id })),
    acceptChange: async (id, newVersion, seenSha256) => expectView(await invoke<unknown>("tools_accept_change", { id, newVersion, seenSha256 })),
    planRun: async (request) => expectPlan(await invoke<unknown>("tools_plan_run", { request })),
    startRun: async (request) => expectStarted(await invoke<unknown>("tools_start_run", { request })),
    pollRun: async (run) => expectPolled(await invoke<unknown>("tools_poll_run", { run })),
    runRecords: async (workspace) => expectRecords(await invoke<unknown>("tools_run_records", { workspace })),
  };
}

// ---------------------------------------------------------------------------
// Words and small rules the page shows (the core enforces the real ones)
// ---------------------------------------------------------------------------

export const REDISTRIBUTION_LABEL: Record<Redistribution, string> = {
  unknown: "Not known (SUL assumes nothing)",
  not_permitted: "Not permitted",
  permitted: "Permitted (your word; SUL does not check)",
};

export const workspaceKey = (w: WorkspaceRef): string => `${w.recipe_id}\u0000${w.root_id}\u0000${w.run_id}`;

function count(n: number, one: string, many = `${one}s`): string {
  return `${n} ${n === 1 ? one : many}`;
}

export const shortHash = (sha: string): string => (sha.length > 12 ? `${sha.slice(0, 12)}…` : sha);

/** Arguments typed one per line. Blank lines are dropped; every other line is one argument, spaces and all, untouched. */
export function parseArgs(text: string): string[] {
  return text
    .split("\n")
    .map((l) => l.replace(/\r$/, ""))
    .filter((l) => l.trim() !== "");
}

/**
 * Settings typed as `name = value`, one per line. These are only written into the record: nothing is passed to the tool.
 * Read into a map first, so a name like `__proto__` is kept as a setting instead of being lost to the object's own
 * prototype setter.
 */
export function parseSettings(text: string): { settings: Record<string, string> } | { bad: string } {
  const found = new Map<string, string>();
  const lines = text.split("\n");
  for (let i = 0; i < lines.length; i += 1) {
    const line = lines[i]!.replace(/\r$/, "");
    if (line.trim() === "") continue;
    const at = line.indexOf("=");
    if (at < 0) return { bad: `Line ${i + 1}: write a name, an equals sign and a value, like  bank = music.` };
    const name = line.slice(0, at).trim();
    if (name === "") return { bad: `Line ${i + 1}: the name before the equals sign is empty.` };
    if (found.has(name)) return { bad: `Line ${i + 1}: “${name}” is written twice.` };
    found.set(name, line.slice(at + 1).trim());
  }
  return { settings: Object.fromEntries(found) };
}

export function describeToolStatus(s: ToolStatus): string {
  switch (s.state) {
    case "matches":
      return "The file is the one that was recorded.";
    case "missing":
      return "There is no file at the recorded path any more.";
    case "changed":
      return `The file is NOT the one that was recorded (it is now ${formatSize(s.exe_size)}, SHA-256 starting ${shortHash(s.exe_sha256)}). SUL will not run it until you accept it.`;
  }
}

export function describeRecord(r: ToolRunRecord): string {
  const how = r.success ? "Finished with success" : r.exit_code === null ? "Ended without an exit code" : `Failed (exit code ${r.exit_code})`;
  if (r.comparison_error !== null) return `${how}. The workspace folder could not be compared, so this record does not say what changed.`;
  return `${how}. In the workspace folder: ${count(r.added.length, "file")} added, ${count(r.changed.length, "file")} changed, ${count(r.removed.length, "file")} removed.`;
}

/** The tool that a person would pick for a run: the first one recorded. */
export const toolName = (view: ToolsView, id: string): string => view.tools.find((t) => t.id === id)?.display_name ?? id;

export const workspaceLabel = (view: ToolsView, w: WorkspaceRef): string => {
  const known = view.workspaces.find((x) => workspaceKey(x) === workspaceKey(w));
  const root = view.mod_roots.find((r) => r.id === w.root_id)?.label ?? w.root_id;
  return `${known?.recipe_name ?? w.recipe_id}, run ${w.run_id} in “${root}”`;
};

// ---------------------------------------------------------------------------
// The registration form
// ---------------------------------------------------------------------------

export interface RegisterForm {
  display_name: string;
  version: string;
  exe_path: string;
  redistribution: Redistribution;
  license_note: string;
  source_note: string;
}

export const EMPTY_REGISTER: RegisterForm = { display_name: "", version: "", exe_path: "", redistribution: "unknown", license_note: "", source_note: "" };

/** The request a form makes, or what to fix. The core checks the file, the path and the id again. */
export function requestOf(f: RegisterForm): { request: ToolRegisterRequest } | { bad: string } {
  const id = slugId(f.display_name);
  if (f.display_name.trim() === "" || id === "") return { bad: "Give the tool a name that has some letters or digits in it." };
  if (f.version.trim() === "") return { bad: "Write the version you installed. SUL does not read it from the program." };
  if (f.exe_path.trim() === "") return { bad: "Write the full path of the program, for example D:\\Tools\\audio\\tool.exe." };
  return {
    request: {
      id,
      display_name: f.display_name.trim(),
      version: f.version.trim(),
      exe_path: f.exe_path.trim(),
      redistribution: f.redistribution,
      license_note: f.license_note.trim(),
      source_note: f.source_note.trim(),
    },
  };
}

// ---------------------------------------------------------------------------
// The controller
// ---------------------------------------------------------------------------

export interface RunPreview {
  request: ToolRunRequest;
  plan: ToolRunPlan;
}

/** A run that finished while this window was open. */
export interface FinishedRun {
  tool_id: string;
  workspace: WorkspaceRef;
  record: ToolRunRecord;
  saved_as: string | null;
  save_error: string | null;
}

export interface ToolsState {
  loaded: boolean;
  view: ToolsView | null;
  /** What is running, or null. */
  busy: string | null;
  error: string | null;
  notice: string | null;
  /** The latest check of each tool, by id. */
  checks: Record<string, ToolStatus>;
  /** A preview of a run, waiting for the person to confirm or cancel. */
  preview: RunPreview | null;
  finished: FinishedRun[];
  /** The records kept beside a finished recipe run, last asked for. */
  records: { workspace: WorkspaceRef; list: StoredRunRecords } | null;
}

export const EMPTY_TOOLS: ToolsState = { loaded: false, view: null, busy: null, error: null, notice: null, checks: {}, preview: null, finished: [], records: null };

/** Most finished runs kept in the window's list. The records themselves are kept on disk by the core. */
export const MAX_FINISHED_SHOWN = 20;

export class ToolsController {
  private state: ToolsState = EMPTY_TOOLS;
  private readonly listeners = new Set<() => void>();
  private polling = false;

  /** `api` is null when there is no backend (a plain browser). */
  constructor(private readonly api: ToolsApi | null) {}

  get hasBackend(): boolean {
    return this.api !== null;
  }

  subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  };

  getSnapshot = (): ToolsState => this.state;

  private set(patch: Partial<ToolsState>): void {
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

  /** Run one backend operation, one at a time. A failure leaves the data as it was and says why. */
  private async run<T>(what: string, op: (api: ToolsApi) => Promise<T>, done: (result: T) => Partial<ToolsState>): Promise<boolean> {
    if (!this.api || this.state.busy !== null) return false;
    this.set({ busy: what, error: null, notice: null });
    try {
      const result = await op(this.api);
      this.set({ ...done(result), busy: null });
      return true;
    } catch (error) {
      this.set({ busy: null, error: messageOf(error) });
      return false;
    }
  }

  private refuse(message: string): Promise<boolean> {
    this.set({ error: message, notice: null });
    return Promise.resolve(false);
  }

  /** Record a program the owner installed. SUL reads the file once to hash it; it copies, downloads and starts nothing. */
  registerTool(form: RegisterForm): Promise<boolean> {
    const r = requestOf(form);
    if ("bad" in r) return this.refuse(r.bad);
    return this.run("register", (api) => api.register(r.request), (view) => ({
      view,
      notice: `Recorded “${r.request.display_name}” ${r.request.version}. SUL read the file once to hash it. Nothing was copied, downloaded or started.`,
    }));
  }

  /** Read a tool's file and say whether it is still the recorded one. */
  checkTool(id: string): Promise<boolean> {
    return this.run("check", (api) => api.check(id), (c) => ({ checks: { ...this.state.checks, [c.id]: c.status } }));
  }

  /** Accept the changed file as the tool. Only offered after a check has shown what changed: the hash it showed is sent back. */
  acceptChange(id: string, newVersion: string): Promise<boolean> {
    const seen = this.state.checks[id];
    if (!seen || seen.state !== "changed") return this.refuse("Check the tool first, so you see exactly which file you are accepting.");
    if (newVersion.trim() === "") return this.refuse("Write the version of the file you are accepting.");
    return this.run("accept", (api) => api.acceptChange(id, newVersion.trim(), seen.exe_sha256), (view) => {
      const { [id]: _gone, ...checks } = this.state.checks;
      return { view, checks, notice: `The file is now recorded as version ${newVersion.trim()}. The earlier one stays in the tool's history.` };
    });
  }

  /** What the run would start. Starts nothing. Nothing is started until `confirmRun`. */
  previewRun(toolId: string, workspace: WorkspaceRef | null, argsText: string, settingsText: string): Promise<boolean> {
    if (toolId === "") return this.refuse("Record a tool first, then pick it here.");
    if (!workspace) return this.refuse("Pick the finished run whose files the tool will work in. A recipe has to be run first, on the Builder part of this page.");
    const s = parseSettings(settingsText);
    if ("bad" in s) return this.refuse(s.bad);
    const request: ToolRunRequest = { tool_id: toolId, args: parseArgs(argsText), settings: s.settings, workspace };
    return this.run("preview", (api) => api.planRun(request), (plan) => ({ preview: { request, plan } }));
  }

  closePreview(): void {
    this.set({ preview: null });
  }

  /** Start what the preview showed. The core plans again and refuses a blocked plan or a tool whose file changed. */
  async confirmRun(): Promise<boolean> {
    const p = this.state.preview;
    if (!p) return false;
    if (p.plan.problems.length > 0) return this.refuse("That run is blocked, so nothing was started. The preview lists what blocks it.");
    return this.run("start", (api) => api.startRun(p.request), (s) => ({
      view: s.view,
      preview: null,
      notice: `Started “${toolName(s.view, s.run.tool_id)}” (process ${s.run.pid}). It runs on its own: SUL cannot cancel it from here, and it keeps going if this window is closed.`,
    }));
  }

  /**
   * Ask about every tool that is running. Not one of the single operations: the page calls this on a timer, it is skipped
   * while another request or an earlier poll is still open, and a failure is shown without closing anything.
   */
  async pollRunning(): Promise<void> {
    const api = this.api;
    const view = this.state.view;
    if (!api || !view || this.polling || this.state.busy !== null || view.running.length === 0) return;
    this.polling = true;
    try {
      for (const r of [...view.running]) {
        try {
          const polled = await api.pollRun(r.run);
          if (polled.state === "finished") {
            const f: FinishedRun = { tool_id: r.tool_id, workspace: r.workspace, record: polled.record, saved_as: polled.saved_as, save_error: polled.save_error };
            this.set({
              view: polled.view,
              finished: [f, ...this.state.finished].slice(0, MAX_FINISHED_SHOWN),
              notice: `“${toolName(polled.view, r.tool_id)}” finished. ${describeRecord(polled.record)}`,
              error: polled.save_error === null ? this.state.error : `The record of that run could not be kept: ${polled.save_error}`,
            });
          }
        } catch (error) {
          // The core no longer follows a run it cannot ask about: say why, and list what is still followed.
          this.set({ error: messageOf(error) });
          try {
            this.set({ view: await api.view() });
          } catch {
            /* the message above stands */
          }
        }
      }
    } finally {
      this.polling = false;
    }
  }

  /** The records kept beside a finished recipe run. */
  loadRecords(workspace: WorkspaceRef): Promise<boolean> {
    return this.run("records", (api) => api.runRecords(workspace), (list) => ({ records: { workspace, list } }));
  }

  closeRecords(): void {
    this.set({ records: null });
  }
}
