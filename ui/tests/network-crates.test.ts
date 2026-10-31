// The checker that keeps "no network crate is compiled into the Windows app" true (.github/scripts/check-network-crates.mjs).
// The real run against cargo happens in CI and by hand; here the decision logic is tested on inline fixtures, including
// the case the Security & Trust investigation found: reqwest is in Tauri's mobile-only dependencies, so it is in the lock file and in a
// mobile target's graph, but not in the Windows graph.
import assert from "node:assert/strict";
import { describe, mock, test } from "node:test";
import { BUILDS, NETWORK_CRATES, PACKAGES, TARGET, findProblems, main, packageNames } from "../../.github/scripts/check-network-crates.mjs";

const WINDOWS_TREE = ["sul-tauri v0.1.0 (C:\\repo\\src-tauri)", "tauri v2.11.6", "tokio v1.54.1", "http v1.5.0", "url v2.5.8", "windows v0.61.3", "wry v0.55.1"].join("\r\n");
const ANDROID_TREE = [WINDOWS_TREE, "reqwest v0.13.5", "hyper v1.12.0", "hyper-util v0.1.21", "mio v1.2.4", "socket2 v0.6.5"].join("\n");
const CLEAN_FEATURES = ['tokio feature "rt"', 'tokio feature "sync"', 'windows feature "Win32_UI"', 'windows feature "Win32_System_Registry"'].join("\n");

describe("what is checked", () => {
  test("the target is Windows and both of SUL's own packages are covered", () => {
    assert.equal(TARGET, "x86_64-pc-windows-msvc");
    assert.deepEqual(PACKAGES, ["sul-core", "sul-tauri"]);
  });

  test("the shipped app and the optional steam feature are both checked", () => {
    assert.deepEqual(BUILDS, [
      { pkg: "sul-core", features: [] },
      { pkg: "sul-tauri", features: [] },
      { pkg: "sul-tauri", features: ["steam"] },
    ]);
  });

  test("the deny list covers the common HTTP, socket, TLS and WebSocket crates", () => {
    for (const c of ["reqwest", "hyper", "mio", "socket2", "ureq", "curl", "native-tls", "rustls", "openssl", "tungstenite", "h2", "quinn"]) {
      assert.ok(NETWORK_CRATES.includes(c), c);
    }
    assert.equal(new Set(NETWORK_CRATES).size, NETWORK_CRATES.length, "no duplicates");
  });

  test("package names are read from cargo's one-per-line output, with either line ending", () => {
    assert.deepEqual([...packageNames(WINDOWS_TREE)].sort(), ["http", "sul-tauri", "tauri", "tokio", "url", "windows", "wry"]);
    assert.deepEqual([...packageNames("")], []);
    assert.deepEqual([...packageNames("  tokio v1.0.0 (*)\n\n")], ["tokio"]);
  });
});

describe("what is reported", () => {
  test("the Windows graph as checked in the Security & Trust investigation is clean", () => {
    assert.deepEqual(findProblems("sul-tauri", WINDOWS_TREE, CLEAN_FEATURES), []);
  });

  test("a mobile graph, which does contain reqwest, is flagged crate by crate", () => {
    const problems = findProblems("sul-tauri", ANDROID_TREE, CLEAN_FEATURES);
    assert.deepEqual(problems.map((p) => /"([^"]+)"/.exec(p)?.[1]).sort(), ["hyper", "hyper-util", "mio", "reqwest", "socket2"]);
    assert.match(problems[0]!, /^sul-tauri: the crate "[a-z0-9-]+" is compiled into the app$/);
  });

  test("a crate whose name only contains a listed name is not a match", () => {
    assert.deepEqual(findProblems("p", ["hyperlink v1.0.0", "mioxide v1.0.0", "reqwest-like v0.1.0"].join("\n"), ""), []);
  });

  test("tokio's net feature is flagged, its other features are not", () => {
    assert.deepEqual(findProblems("p", WINDOWS_TREE, 'tokio feature "rt"\ntokio feature "net"\n'), ['p: tokio\'s "net" feature is on']);
    assert.deepEqual(findProblems("p", WINDOWS_TREE, 'tokio feature "network-ish"\nmio feature "net"\n'), []);
  });

  test("Windows API features for the network stack are flagged, other Win32 features are not", () => {
    const features = [
      'windows feature "Win32_Networking_WinHttp"', 'windows-sys feature "Win32_Networking_WinSock"', 'windows feature "Win32_Networking"',
      'windows feature "Win32_System_Com_Urlmon"', 'windows feature "Win32_NetworkManagement_IpHelper"',
      'windows feature "Win32_System_Com"', 'windows feature "Win32_UI_Shell"', 'windows feature "Win32_Storage_FileSystem"',
    ].join("\n");
    const found = findProblems("p", WINDOWS_TREE, features).map((p) => /feature "([^"]+)"/.exec(p)?.[1]);
    assert.deepEqual(found, ["Win32_Networking_WinHttp", "Win32_Networking_WinSock", "Win32_Networking", "Win32_System_Com_Urlmon", "Win32_NetworkManagement_IpHelper"]);
  });

  test("every problem names the package it was found in", () => {
    for (const pkg of PACKAGES) for (const p of findProblems(pkg, ANDROID_TREE, 'tokio feature "net"')) assert.ok(p.startsWith(`${pkg}: `), p);
  });
});

describe("the command", () => {
  const fake = (packages: string, features: string) => (_pkg: string, extra: string[]) => (extra.includes("--prefix") ? packages : features);
  const quietly = <T,>(run: () => T): { result: T; out: string[]; err: string[] } => {
    const out: string[] = [];
    const err: string[] = [];
    const log = mock.method(console, "log", (...a: unknown[]) => void out.push(a.join(" ")));
    const error = mock.method(console, "error", (...a: unknown[]) => void err.push(a.join(" ")));
    try {
      return { result: run(), out, err };
    } finally {
      log.mock.restore();
      error.mock.restore();
    }
  };

  test("a clean graph exits 0 and says what was checked", () => {
    const { result, out, err } = quietly(() => main(fake(WINDOWS_TREE, CLEAN_FEATURES)));
    assert.equal(result, 0);
    assert.deepEqual(err, []);
    assert.match(out.join("\n"), /x86_64-pc-windows-msvc, normal dependencies\. sul-core: 7 crates; sul-tauri: 7 crates; sul-tauri \(steam\): 7 crates\. No network crate, no tokio net feature, no Windows networking feature\./);
  });

  test("a graph with a network crate exits 1 and names it", () => {
    const { result, err } = quietly(() => main(fake(ANDROID_TREE, CLEAN_FEATURES)));
    assert.equal(result, 1);
    assert.ok(err.some((l) => l.includes('the crate "reqwest" is compiled into the app')), err.join("\n"));
  });

  test("a feature problem alone is enough to exit 1", () => {
    assert.equal(quietly(() => main(fake(WINDOWS_TREE, 'tokio feature "net"'))).result, 1);
    assert.equal(quietly(() => main(fake(WINDOWS_TREE, 'windows feature "Win32_Networking_WinHttp"'))).result, 1);
  });

  test("in GitHub Actions the result is written as annotations, because raw logs cannot be downloaded", () => {
    const before = process.env.GITHUB_ACTIONS;
    process.env.GITHUB_ACTIONS = "true";
    try {
      const bad = quietly(() => main(fake(ANDROID_TREE, CLEAN_FEATURES)));
      assert.ok(bad.err.every((l) => l.startsWith("::error title=network crate::")), bad.err.join("\n"));
      const good = quietly(() => main(fake(WINDOWS_TREE, CLEAN_FEATURES)));
      assert.ok(good.out.some((l) => l.startsWith("::notice title=network crates::")), good.out.join("\n"));
    } finally {
      if (before === undefined) delete process.env.GITHUB_ACTIONS;
      else process.env.GITHUB_ACTIONS = before;
    }
  });
});

