import { useEffect, useMemo, useState, useSyncExternalStore } from "react";
import { Dialog } from "./Dialog";
import { StatusBadge } from "./StatusBadge";
import { formatSize } from "../lib/mods";
import {
  EMPTY_REGISTER,
  REDISTRIBUTION_LABEL,
  ToolsController,
  describeRecord,
  describeToolStatus,
  shortHash,
  tauriToolsApi,
  toolName,
  workspaceKey,
  workspaceLabel,
  type FinishedRun,
  type Redistribution,
  type RegisterForm,
  type ToolRecord,
  type ToolRunRecord,
  type ToolStatus,
  type ToolsApi,
  type ToolsView,
  type WorkspaceRef,
} from "../lib/tools";

/** Rows of a record shown before "and N more"; the record itself keeps every file. */
const SHOWN_ROWS = 200;

/** How often a running tool is asked whether it has finished. */
export const POLL_MS = 1500;

function Rows({ items, label }: { items: string[]; label: string }) {
  if (items.length === 0) return null;
  const shown = items.slice(0, SHOWN_ROWS);
  return (
    <>
      <ul aria-label={label}>
        {shown.map((m, i) => (
          <li key={i}>
            <code>{m}</code>
          </li>
        ))}
      </ul>
      {items.length > shown.length && <p className="muted">…and {items.length - shown.length} more.</p>}
    </>
  );
}

/** What a finished run did to its workspace folder, and what it cannot say. */
function RecordBody({ record }: { record: ToolRunRecord }) {
  const started = new Date(record.started_unix * 1000).toISOString().slice(0, 16).replace("T", " ");
  return (
    <>
      <p>
        {describeRecord(record)} Started {started} UTC; tool version {record.tool_version} (file <code>{shortHash(record.tool_sha256)}</code>).
      </p>
      {record.args.length > 0 && <Rows items={record.args} label="Arguments it was given" />}
      {Object.keys(record.settings).length > 0 && (
        <p className="muted">
          Settings you wrote down (only recorded, not passed to the tool):{" "}
          {Object.entries(record.settings)
            .map(([k, v]) => `${k} = ${v}`)
            .join("; ")}
        </p>
      )}
      {record.comparison_error !== null && (
        <p className="matrix-error" role="alert">
          The workspace folder could not be compared: {record.comparison_error}. The lists of added, changed and removed files are empty because nothing could be seen, not because nothing happened.
        </p>
      )}
      {record.comparison_error === null && (
        <>
          <Rows items={record.added.map((f) => `${f.path} (${formatSize(f.size)})`)} label="Files added" />
          <Rows items={record.changed.map((f) => f.path)} label="Files changed" />
          <Rows items={record.removed} label="Files removed" />
        </>
      )}
      <p className="muted">{record.limits}</p>
    </>
  );
}

/**
 * External tools (spec 2.4). `api` is the backend to use; `undefined` means the real one and `null` means none (a plain
 * browser, or a test). `pollMs` is how often a running tool is asked whether it has finished.
 *
 * SUL runs only a program the owner recorded, only while its file is still the recorded one, and only after a question
 * that shows exactly what would start. The core plans again when it runs and refuses a blocked plan whatever this page sends.
 */
export function ToolsPanel({ api, pollMs = POLL_MS }: { api?: ToolsApi | null; pollMs?: number }) {
  const controller = useMemo(() => new ToolsController(api === undefined ? tauriToolsApi() : api), [api]);
  const state = useSyncExternalStore(controller.subscribe, controller.getSnapshot, controller.getSnapshot);
  const view = state.view;
  const busy = state.busy !== null;
  const running = view?.running.length ?? 0;

  useEffect(() => {
    void controller.load();
  }, [controller]);

  // While a tool is running, ask now and then whether it has finished.
  useEffect(() => {
    if (running === 0) return;
    const timer = setInterval(() => void controller.pollRunning(), pollMs);
    return () => clearInterval(timer);
  }, [controller, running, pollMs]);

  if (!controller.hasBackend) {
    return (
      <section className="card managed" aria-label="External tools">
        <header className="game-card-head">
          <h3>External tools</h3>
          <StatusBadge status="experimental" />
        </header>
        <p className="muted">Browser preview: external tools need the desktop app, so nothing is listed here.</p>
      </section>
    );
  }

  return (
    <section className="managed tools" aria-label="External tools">
      <div className="card">
        <header className="game-card-head">
          <h3>External tools</h3>
          <StatusBadge status="experimental" />
        </header>
        <p className="muted">
          Experimental: tested with made-up programs only. SUL starts a tool only if you installed and recorded it yourself; it downloads, copies and bundles nothing, and it
          will not start a tool whose file is no longer the recorded one. SUL cannot confine a program: a tool can write anywhere your Windows account can, and the record
          of a run lists only the files inside the workspace folder it was given. A tool keeps running if this window is closed. SUL has never run any real tool.
        </p>
        <p className="muted" role="status" aria-live="polite">
          {busy ? "Working…" : (state.notice ?? "")}
        </p>
        {state.error && (
          <p className="matrix-error" role="alert">
            {state.error}
          </p>
        )}
        {view && view.run_errors.length > 0 && (
          <ul className="matrix-error" role="alert" aria-label="Things that could not be listed">
            {view.run_errors.map((m, i) => (
              <li key={i}>{m}</li>
            ))}
          </ul>
        )}
        {view && view.running.length > 0 && (
          <div aria-label="Tools running now">
            <h4>Running now</h4>
            <ul className="trust-list">
              {view.running.map((r) => (
                <li key={r.run}>
                  <span className="trust-title">{toolName(view, r.tool_id)}</span> · process {r.pid} · on {workspaceLabel(view, r.workspace)}
                </li>
              ))}
            </ul>
          </div>
        )}
      </div>

      {view && (
        <>
          <Known view={view} />
          <Recorded controller={controller} view={view} busy={busy} checks={state.checks} />
          <Register controller={controller} busy={busy} />
          <RunATool controller={controller} view={view} busy={busy} />
          <FinishedRuns view={view} finished={state.finished} />
        </>
      )}

      {state.preview && view && <PreviewDialog controller={controller} view={view} busy={busy} />}
      {state.records && view && <RecordsDialog controller={controller} view={view} />}
    </section>
  );
}

// ---------------------------------------------------------------------------
// What is known
// ---------------------------------------------------------------------------

function Known({ view }: { view: ToolsView }) {
  return (
    <div className="card" aria-label="Tools the spec names">
      <h3>The tools the spec names</h3>
      <p className="muted">What was found out by reading public pages, and what was not. Nothing here was run, so none of it says a tool works.</p>
      <ul className="trust-list">
        {view.known.map((k) => (
          <li key={k.id}>
            <span className="trust-title">{k.name}</span> <StatusBadge status={k.status} />
            <p className="muted">{k.finding}</p>
            <p className="muted">Not established: {k.not_established}</p>
          </li>
        ))}
      </ul>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Recorded tools
// ---------------------------------------------------------------------------

function Recorded({ controller, view, busy, checks }: { controller: ToolsController; view: ToolsView; busy: boolean; checks: Record<string, ToolStatus> }) {
  return (
    <div className="card" aria-label="Recorded tools">
      <h3>Recorded tools</h3>
      {view.tools.length === 0 && <p className="muted">No tool is recorded yet. Install one yourself, then record where it is below.</p>}
      <ul className="trust-list">
        {view.tools.map((t) => (
          <ToolRow key={t.id} t={t} busy={busy} status={checks[t.id]} controller={controller} />
        ))}
      </ul>
    </div>
  );
}

function ToolRow({ t, busy, status, controller }: { t: ToolRecord; busy: boolean; status: ToolStatus | undefined; controller: ToolsController }) {
  const [version, setVersion] = useState("");
  const [asking, setAsking] = useState(false);
  return (
    <li aria-label={`Tool ${t.display_name}`}>
      <span className="trust-title">{t.display_name}</span> <span className="tag">{t.version}</span>
      <p className="muted">
        <code>{t.id}</code> · <code>{t.exe_path}</code> · {formatSize(t.exe_size)} · SHA-256 <code>{shortHash(t.exe_sha256)}</code>
      </p>
      <p className="muted">
        Redistribution: {REDISTRIBUTION_LABEL[t.redistribution]}. SUL bundles nothing whatever this says.
        {t.license_note !== "" && <> Licence note (your words, not checked): {t.license_note}.</>}
        {t.source_note !== "" && <> From: {t.source_note}.</>}
      </p>
      {t.history.length > 0 && <p className="muted">{t.history.length === 1 ? "1 earlier version is" : `${t.history.length} earlier versions are`} kept in its history.</p>}
      {status && (
        <p className={status.state === "matches" ? "muted" : "matrix-error"} role={status.state === "matches" ? undefined : "alert"}>
          {describeToolStatus(status)}
        </p>
      )}
      <div className="row">
        <button type="button" className="btn btn-ghost" disabled={busy} aria-label={`Check the file of ${t.display_name}`} onClick={() => void controller.checkTool(t.id)}>
          Check file
        </button>
      </div>
      {status?.state === "changed" && (
        <form className="row" aria-label={`Accept the changed file of ${t.display_name}`} onSubmit={(e) => e.preventDefault()}>
          <label className="field">
            Version of the file you are accepting
            <input type="text" value={version} onChange={(e) => setVersion(e.target.value)} />
          </label>
          <button type="button" className="btn" disabled={busy} onClick={() => setAsking(true)}>
            Accept this file…
          </button>
        </form>
      )}
      {asking && status?.state === "changed" && (
        <Dialog
          title={`Accept the changed file as ${t.display_name}?`}
          onClose={() => setAsking(false)}
          actions={
            <>
              <button type="button" className="btn btn-ghost" onClick={() => setAsking(false)}>
                Cancel
              </button>
              <button
                type="button"
                className="btn"
                onClick={() => {
                  setAsking(false);
                  void controller.acceptChange(t.id, version).then((ok) => ok && setVersion(""));
                }}
              >
                Accept it
              </button>
            </>
          }
        >
          <p>
            The file at <code>{t.exe_path}</code> is no longer the one SUL recorded. If you accept it, SUL will treat the file with SHA-256 <code>{shortHash(status.exe_sha256)}</code> (
            {formatSize(status.exe_size)}) as {t.display_name} {version.trim() === "" ? "(write its version first)" : version.trim()} and may start it. SUL cannot tell whether the change is
            wanted: only accept a file you know where it came from. The recorded one is kept in the tool&apos;s history.
          </p>
        </Dialog>
      )}
    </li>
  );
}

// ---------------------------------------------------------------------------
// Recording a tool
// ---------------------------------------------------------------------------

function Register({ controller, busy }: { controller: ToolsController; busy: boolean }) {
  const [form, setForm] = useState<RegisterForm>(EMPTY_REGISTER);
  const edit = (patch: Partial<RegisterForm>) => setForm((f) => ({ ...f, ...patch }));
  return (
    <div className="card" aria-label="Record a tool">
      <h3>Record a tool</h3>
      <p className="muted">
        Recording reads the program once to hash it and writes its details to SUL&apos;s own list. The version, the licence and where it came from are your words: SUL does not
        read them from the program or check them.
      </p>
      <form className="row" aria-label="Record a tool form" onSubmit={(e) => e.preventDefault()}>
        <label className="field">
          Name
          <input type="text" value={form.display_name} onChange={(e) => edit({ display_name: e.target.value })} />
        </label>
        <label className="field">
          Version you installed
          <input type="text" value={form.version} onChange={(e) => edit({ version: e.target.value })} />
        </label>
        <label className="field">
          Full path of the program
          <input type="text" value={form.exe_path} onChange={(e) => edit({ exe_path: e.target.value })} />
        </label>
        <label className="field">
          May it be redistributed?
          <select value={form.redistribution} onChange={(e) => edit({ redistribution: e.target.value as Redistribution })}>
            {(Object.keys(REDISTRIBUTION_LABEL) as Redistribution[]).map((k) => (
              <option key={k} value={k}>
                {REDISTRIBUTION_LABEL[k]}
              </option>
            ))}
          </select>
        </label>
        <label className="field">
          Licence or permission
          <input type="text" value={form.license_note} onChange={(e) => edit({ license_note: e.target.value })} />
        </label>
        <label className="field">
          Where you got it
          <input type="text" value={form.source_note} onChange={(e) => edit({ source_note: e.target.value })} />
        </label>
        <button type="button" className="btn" disabled={busy} onClick={() => void controller.registerTool(form).then((ok) => ok && setForm(EMPTY_REGISTER))}>
          Record the tool
        </button>
      </form>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Running a tool
// ---------------------------------------------------------------------------

function RunATool({ controller, view, busy }: { controller: ToolsController; view: ToolsView; busy: boolean }) {
  const [toolId, setToolId] = useState("");
  const [workspaceId, setWorkspaceId] = useState("");
  const [args, setArgs] = useState("");
  const [settings, setSettings] = useState("");
  const tool = view.tools.find((t) => t.id === toolId) ?? view.tools[0];
  const workspace: WorkspaceRef | null = view.workspaces.find((w) => workspaceKey(w) === workspaceId) ?? view.workspaces[0] ?? null;
  const ref = workspace ? { recipe_id: workspace.recipe_id, root_id: workspace.root_id, run_id: workspace.run_id } : null;
  return (
    <div className="card" aria-label="Run a tool">
      <h3>Run a tool</h3>
      <p className="muted">
        A tool works in the files of a finished recipe run (built on the Builder part of this page), never in a game&apos;s folder. SUL shows exactly what it would start and waits
        for your answer. Afterwards it compares that workspace folder before and after and keeps a record beside the run.
      </p>
      <form className="row" aria-label="Run a tool form" onSubmit={(e) => e.preventDefault()}>
        <label className="field">
          Tool
          <select value={tool?.id ?? ""} onChange={(e) => setToolId(e.target.value)}>
            {view.tools.length === 0 && <option value="">(record a tool first)</option>}
            {view.tools.map((t) => (
              <option key={t.id} value={t.id}>
                {t.display_name} {t.version}
              </option>
            ))}
          </select>
        </label>
        <label className="field">
          Work in the files of
          <select value={workspace ? workspaceKey(workspace) : ""} onChange={(e) => setWorkspaceId(e.target.value)}>
            {view.workspaces.length === 0 && <option value="">(no finished recipe run yet)</option>}
            {view.workspaces.map((w) => (
              <option key={workspaceKey(w)} value={workspaceKey(w)}>
                {workspaceLabel(view, w)}
              </option>
            ))}
          </select>
        </label>
        <label className="field">
          Arguments, one per line
          <textarea rows={3} value={args} onChange={(e) => setArgs(e.target.value)} />
        </label>
        <label className="field">
          Settings to write in the record, as name = value, one per line
          <textarea rows={2} value={settings} onChange={(e) => setSettings(e.target.value)} />
        </label>
        <button type="button" className="btn" disabled={busy} onClick={() => void controller.previewRun(tool?.id ?? "", ref, args, settings)}>
          Preview the run…
        </button>
        <button type="button" className="btn btn-ghost" disabled={busy || !ref} onClick={() => ref && void controller.loadRecords(ref)}>
          Show the records kept for this run
        </button>
      </form>
    </div>
  );
}

function PreviewDialog({ controller, view, busy }: { controller: ToolsController; view: ToolsView; busy: boolean }) {
  const state = useSyncExternalStore(controller.subscribe, controller.getSnapshot, controller.getSnapshot);
  const p = state.preview;
  if (!p) return null;
  const blocked = p.plan.problems.length > 0;
  return (
    <Dialog
      title={`Start “${toolName(view, p.request.tool_id)}”?`}
      onClose={() => controller.closePreview()}
      actions={
        <>
          <button type="button" className="btn btn-ghost" onClick={() => controller.closePreview()}>
            Cancel
          </button>
          <button type="button" className="btn" disabled={busy || blocked} onClick={() => void controller.confirmRun()}>
            Start the tool
          </button>
        </>
      }
    >
      {blocked && (
        <ul className="matrix-error" role="alert" aria-label="Problems that block this run">
          {p.plan.problems.map((m, i) => (
            <li key={i}>{m}</li>
          ))}
        </ul>
      )}
      <p>
        SUL will start <code>{p.plan.exe}</code> (version {p.plan.tool_version}, SHA-256 <code>{shortHash(p.plan.tool_sha256)}</code>) in the folder <code>{p.plan.cwd}</code>, for{" "}
        {workspaceLabel(view, p.request.workspace)}.
      </p>
      {p.plan.args.length > 0 ? <Rows items={p.plan.args} label="Arguments it will be given" /> : <p className="muted">It will be given no arguments.</p>}
      <p className="muted">
        SUL cannot confine a program. Once started it can do anything your Windows account can do, including writing outside that folder, and SUL cannot cancel it from here. The preview is
        not trusted: SUL checks the program&apos;s file again when you confirm and refuses to start it if it changed.
      </p>
    </Dialog>
  );
}

// ---------------------------------------------------------------------------
// Records
// ---------------------------------------------------------------------------

function FinishedRuns({ view, finished }: { view: ToolsView; finished: FinishedRun[] }) {
  if (finished.length === 0) return null;
  return (
    <div className="card" aria-label="Runs finished in this window">
      <h3>Finished in this window</h3>
      <ul className="trust-list">
        {finished.map((f, i) => (
          <li key={`${f.record.started_unix}-${i}`}>
            <span className="trust-title">{toolName(view, f.tool_id)}</span> · {workspaceLabel(view, f.workspace)}
            <RecordBody record={f.record} />
            {f.saved_as !== null && <p className="muted">Record kept as <code>{f.saved_as}</code> beside the run.</p>}
            {f.save_error !== null && (
              <p className="matrix-error" role="alert">
                The record could not be kept: {f.save_error}. What is shown here is the true record.
              </p>
            )}
          </li>
        ))}
      </ul>
    </div>
  );
}

function RecordsDialog({ controller, view }: { controller: ToolsController; view: ToolsView }) {
  const state = useSyncExternalStore(controller.subscribe, controller.getSnapshot, controller.getSnapshot);
  const r = state.records;
  if (!r) return null;
  return (
    <Dialog
      title="Records kept for this run"
      onClose={() => controller.closeRecords()}
      actions={
        <button type="button" className="btn" onClick={() => controller.closeRecords()}>
          Close
        </button>
      }
    >
      <p>
        {workspaceLabel(view, r.workspace)}: {r.list.total === 0 ? "no record is kept yet." : `${r.list.total} ${r.list.total === 1 ? "record is" : "records are"} kept${r.list.total > r.list.records.length ? `; the newest ${r.list.records.length} are shown` : ""}.`}
      </p>
      {r.list.unreadable.length > 0 && <Rows items={r.list.unreadable} label="Record files that could not be read" />}
      <ul aria-label="Kept records">
        {r.list.records.map((s) => (
          <li key={s.name}>
            <code>{s.name}</code> · {toolName(view, s.record.tool_id)}
            <RecordBody record={s.record} />
          </li>
        ))}
      </ul>
    </Dialog>
  );
}
