// A backend for the Mods page that lives in memory. It is NOT the core: it keeps the shape of the real answers and
// refuses the few things a test needs refused, with plain-text errors like a failing command. What the core really
// refuses (a forbidden folder, a file changed since it was deployed, a game with no adapter) is tested in
// `crates/sul-core/src/manager/modding.rs` and `src-tauri/src/ipc_tests.rs`.
import type {
  BuildModsView,
  DeployPlan,
  ModView,
  ModsApi,
  ModsView,
  Problem,
  ProfileEdit,
  ProfileView,
  PurgeItem,
  PurgePlan,
  Requirement,
} from "../src/lib/mods";

export const EMPTY_MODS_VIEW: ModsView = { mods: [], profiles: [], builds: [], mod_roots: [], deployable_games: [] };

export function modRoot(id = "modlib", label = "Mod library") {
  return { id, label, path: `D:\\SUL\\${label}`, kind: "mods" as const, present: true, marker_ok: true, free_bytes: 500_000_000_000 };
}

export function mod(gameId: string, modId: string, version: string, extra: Partial<ModView> = {}): ModView {
  return {
    game_id: gameId,
    mod_id: modId,
    name: `Mod ${modId}`,
    version,
    requires: [] as Requirement[],
    compatible_versions: [],
    file_count: 2,
    total_bytes: 7,
    source: "made by hand",
    license_note: "mine",
    imported_unix: 1_700_000_000,
    root_id: "modlib",
    ...extra,
  };
}

export function profile(id: string, gameId: string, buildId: string, mods: ProfileView["mods"] = [], extra: Partial<ProfileView> = {}): ProfileView {
  return { id, name: id.toUpperCase(), game_id: gameId, build_id: buildId, mods, config: {}, notes: "", created_unix: 1, updated_unix: 1, problems: [], ...extra };
}

export function buildMods(buildId: string, gameId: string, slot: string, extra: Partial<BuildModsView> = {}): BuildModsView {
  return { build_id: buildId, game_id: gameId, slot, deployment: null, undoable_purge: null, history: [], error: null, ...extra };
}

export function problem(message: string, severity: Problem["severity"] = "error"): Problem {
  return { severity, kind: "x", mod_id: null, path: null, message };
}

export interface FakeCall {
  op: string;
  args: unknown[];
}

export function fakeMods(initial: Partial<ModsView> = {}) {
  const calls: FakeCall[] = [];
  let view: ModsView = { ...EMPTY_MODS_VIEW, ...initial };
  const failures = new Map<string, string>();
  let gate: Promise<void> | null = null;
  let plan: DeployPlan | null = null;
  let purgeItems: PurgeItem[] | null = null;
  let purgeCount = 0;

  const enter = async (op: string, ...args: unknown[]) => {
    calls.push({ op, args });
    if (gate) await gate;
    const failure = failures.get(op);
    if (failure !== undefined) {
      failures.delete(op);
      throw failure; // `invoke` rejects with a plain string
    }
  };
  const snapshot = (): ModsView => structuredClone(view);
  const profileOf = (id: string): ProfileView => {
    const p = view.profiles.find((x) => x.id === id);
    if (!p) throw `no profile with id “${id}”`;
    return p;
  };
  const setProfile = (id: string, change: (p: ProfileView) => ProfileView) => {
    view = { ...view, profiles: view.profiles.map((p) => (p.id === id ? change(p) : p)) };
  };
  const setBuild = (buildId: string, change: (b: BuildModsView) => BuildModsView) => {
    view = { ...view, builds: view.builds.map((b) => (b.build_id === buildId ? change(b) : b)) };
  };
  const defaultPlan = (id: string): DeployPlan => {
    const p = profileOf(id);
    return {
      profile_id: id,
      build_id: p.build_id,
      files: p.mods
        .filter((m) => m.enabled)
        .map((m) => ({ path: `data/${m.mod_id}.bin`, mod_id: m.mod_id, mod_version: m.version, action: "add" as const, sha256: "aa", size: 4, replaces: null, overrides: [] })),
      problems: [],
      total_bytes: 4 * p.mods.filter((m) => m.enabled).length,
    };
  };
  const needAdapter = (id: string) => {
    const p = profileOf(id);
    if (!view.deployable_games.includes(p.game_id)) throw `SUL has no adapter for “${p.game_id}”, so it does not know which folders mods may change in that game, and it deploys nothing without that. Nothing was changed.`;
    return p;
  };
  const deployInto = (p: ProfileView) => {
    const files = (plan ?? defaultPlan(p.id)).files;
    setBuild(p.build_id, (b) => ({
      ...b,
      deployment: { id: `dep-${p.id}`, profile_id: p.id, profile_name: p.name, status: "deployed", started_unix: 5, finished_unix: 6, files: files.length },
      undoable_purge: null,
    }));
    return { deployment_id: `dep-${p.id}`, added: files.map((f) => f.path), replaced: [], kept: [] as [string, string][], warnings: [] as Problem[] };
  };
  const purgeBuild = (buildId: string) => {
    const b = view.builds.find((x) => x.build_id === buildId);
    if (!b?.deployment) throw "nothing is deployed in this build";
    purgeCount += 1;
    const id = `purge-${purgeCount}`;
    const items = purgeItems ?? [];
    const left = items.filter((i) => i.action.startsWith("leave"));
    if (left.length === 0) {
      setBuild(buildId, (x) => ({ ...x, deployment: null, undoable_purge: { id, profile_name: b.deployment!.profile_name, unix: 9, files: b.deployment!.files } }));
    }
    return { purge_id: left.length === 0 ? id : null, removed: items.filter((i) => !i.action.startsWith("leave")).map((i) => i.path), restored: [] as string[], forgotten: [] as string[], left, recovery_dir: "D:\\recovery" };
  };

  const api: ModsApi = {
    async view() {
      await enter("view");
      return snapshot();
    },
    async importMod(source, rootId) {
      await enter("importMod", source, rootId);
      if (view.mods.some((m) => m.game_id === source.game_id && m.mod_id === source.mod_id && m.version === source.version)) throw `${source.mod_id}@${source.version} is already in the store; import it as a new version instead`;
      view = { ...view, mods: [...view.mods, mod(source.game_id, source.mod_id, source.version, { name: source.name, source: source.source, license_note: source.license_note, requires: source.requires, compatible_versions: source.compatible_versions, root_id: rootId })] };
      return snapshot();
    },
    async verifyMod(gameId, modId, version) {
      await enter("verifyMod", gameId, modId, version);
      return { report: { missing: [], modified: [], extra: [] }, view: snapshot() };
    },
    async createProfile(id, name, gameId, buildId) {
      await enter("createProfile", id, name, gameId, buildId);
      if (view.profiles.some((p) => p.id === id)) throw `a profile “${id}” already exists`;
      view = { ...view, profiles: [...view.profiles, profile(id, gameId, buildId, [], { name })] };
      return snapshot();
    },
    async editProfile(profileId, edit: ProfileEdit) {
      await enter("editProfile", profileId, edit);
      const p = profileOf(profileId);
      const must = (mod_id: string) => {
        if (!p.mods.some((m) => m.mod_id === mod_id)) throw `${mod_id} is not in this profile`;
      };
      switch (edit.action) {
        case "rename":
          setProfile(profileId, (x) => ({ ...x, name: edit.name.trim() }));
          break;
        case "notes":
          setProfile(profileId, (x) => ({ ...x, notes: edit.notes }));
          break;
        case "set_build":
          if (view.builds.some((b) => b.build_id === p.build_id && b.deployment?.profile_id === profileId)) throw "this profile is deployed in its build; purge it there before moving the profile to another build";
          setProfile(profileId, (x) => ({ ...x, build_id: edit.build_id }));
          break;
        case "add_mod":
          if (p.mods.some((m) => m.mod_id === edit.mod_id)) throw `${edit.mod_id} is already in this profile; change its version or move it`;
          setProfile(profileId, (x) => ({ ...x, mods: [...x.mods, { mod_id: edit.mod_id, version: edit.version, enabled: true }] }));
          break;
        case "remove_mod":
          must(edit.mod_id);
          setProfile(profileId, (x) => ({ ...x, mods: x.mods.filter((m) => m.mod_id !== edit.mod_id) }));
          break;
        case "set_enabled":
          must(edit.mod_id);
          setProfile(profileId, (x) => ({ ...x, mods: x.mods.map((m) => (m.mod_id === edit.mod_id ? { ...m, enabled: edit.enabled } : m)) }));
          break;
        case "set_version":
          must(edit.mod_id);
          setProfile(profileId, (x) => ({ ...x, mods: x.mods.map((m) => (m.mod_id === edit.mod_id ? { ...m, version: edit.version } : m)) }));
          break;
        case "move": {
          must(edit.mod_id);
          const moved = p.mods.find((m) => m.mod_id === edit.mod_id)!;
          const rest = p.mods.filter((m) => m !== moved);
          rest.splice(Math.min(edit.to_index, rest.length), 0, moved);
          setProfile(profileId, (x) => ({ ...x, mods: rest }));
          break;
        }
        case "set_config": {
          const config = { ...p.config };
          if (edit.value === null) delete config[edit.key];
          else config[edit.key] = edit.value;
          setProfile(profileId, (x) => ({ ...x, config }));
          break;
        }
      }
      return snapshot();
    },
    async planDeploy(profileId) {
      await enter("planDeploy", profileId);
      needAdapter(profileId);
      return structuredClone(plan ?? defaultPlan(profileId));
    },
    async deploy(profileId) {
      await enter("deploy", profileId);
      const p = needAdapter(profileId);
      const report = deployInto(p);
      return { report, view: snapshot() };
    },
    async planPurge(buildId): Promise<PurgePlan> {
      await enter("planPurge", buildId);
      const b = view.builds.find((x) => x.build_id === buildId);
      return {
        build_id: buildId,
        deployment: b?.deployment ? { id: b.deployment.id, profile_id: b.deployment.profile_id, profile_name: b.deployment.profile_name, started_unix: b.deployment.started_unix } : null,
        items: structuredClone(purgeItems ?? []),
      };
    },
    async purge(buildId) {
      await enter("purge", buildId);
      const report = purgeBuild(buildId);
      return { report, view: snapshot() };
    },
    async undoPurge(buildId, purgeId) {
      await enter("undoPurge", buildId, purgeId);
      const b = view.builds.find((x) => x.build_id === buildId);
      if (b?.undoable_purge?.id !== purgeId) throw `“${purgeId}” is not the latest purge of this build`;
      setBuild(buildId, (x) => ({ ...x, undoable_purge: null, deployment: { id: "dep-undone", profile_id: "main", profile_name: b.undoable_purge!.profile_name, status: "deployed", started_unix: 5, finished_unix: 6, files: b.undoable_purge!.files } }));
      return { report: { put_back: ["data/a.bin"], originals_moved_aside_again: [] }, view: snapshot() };
    },
    async purgeAndDeploy(profileId) {
      await enter("purgeAndDeploy", profileId);
      const p = needAdapter(profileId);
      const purge = purgeBuild(p.build_id);
      if (purge.left.length > 0) return { report: { purge, deploy: null }, view: snapshot() };
      return { report: { purge, deploy: deployInto(p) }, view: snapshot() };
    },
  };

  return {
    api,
    calls,
    /** The calls that change something (everything but the views, the checks and the previews). */
    changes: () => calls.filter((c) => !["view", "verifyMod", "planDeploy", "planPurge"].includes(c.op)),
    failNext(op: string, message: string) {
      failures.set(op, message);
    },
    /** What the next previews show. */
    setPlan(p: DeployPlan) {
      plan = p;
    },
    setPurgeItems(items: PurgeItem[]) {
      purgeItems = items;
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
