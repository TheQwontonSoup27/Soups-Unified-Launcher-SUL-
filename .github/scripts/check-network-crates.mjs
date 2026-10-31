// Fails when a network-capable crate is compiled into the shipped Windows app.
//
// It asks cargo for the exact set of crates that go into `sul-core` and `sul-tauri` for the Windows target
// (normal dependencies only: build scripts and test-only crates are not in the app), and checks it against a list of
// HTTP, socket, TLS and WebSocket crates. It also checks two feature switches that would add networking without
// adding a crate (checked for the app as shipped and for the optional `steam` feature): tokio's `net` feature and the Windows API binding features for WinHTTP, WinINet, WinSock and URLMon.
//
// What this is NOT: a proof that nothing in the app touches a network. It does not look inside Microsoft's WebView2,
// the Rust standard library, or Steam. See docs/investigations/security-and-trust-2026-10-31.md.
//
// Run: `node .github/scripts/check-network-crates.mjs`. The pure functions are tested in ui/tests/network-crates.test.ts.
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";

export const TARGET = "x86_64-pc-windows-msvc";
export const PACKAGES = ["sul-core", "sul-tauri"];
/** The app as shipped, plus the one optional feature (real Steamworks bindings) that pulls in another crate. */
export const BUILDS = [
  { pkg: "sul-core", features: [] },
  { pkg: "sul-tauri", features: [] },
  { pkg: "sul-tauri", features: ["steam"] },
];
const label = ({ pkg, features }) => (features.length === 0 ? pkg : `${pkg} (${features.join(",")})`);

/** Crates whose job is HTTP, sockets, DNS, TLS or WebSockets. A match is a build that can reach a network. */
export const NETWORK_CRATES = [
  "reqwest", "hyper", "hyper-util", "hyper-tls", "hyper-rustls", "h2", "http-body-util", "tower-http",
  "mio", "socket2", "net2", "async-std", "smol", "async-net", "tokio-util-net",
  "native-tls", "openssl", "openssl-sys", "rustls", "rustls-pemfile", "tokio-rustls", "tokio-native-tls", "schannel",
  "ureq", "curl", "curl-sys", "isahc", "attohttpc", "surf", "minreq", "http-client", "awc", "actix-http",
  "tungstenite", "tokio-tungstenite", "websocket", "quinn", "quinn-proto",
  "hickory-resolver", "trust-dns-resolver", "c-ares", "dns-lookup",
];

const NETWORK_WINDOWS_FEATURES = /\bfeature "(Win32_Networking[A-Za-z0-9_]*|Win32_System_Com_Urlmon|Win32_NetworkManagement[A-Za-z0-9_]*)"/g;

/** Package names in the output of `cargo tree --prefix none --format "{p}"`. */
export function packageNames(treeText) {
  const names = new Set();
  for (const line of treeText.split(/\r?\n/)) {
    const name = line.trim().split(/\s+/)[0];
    if (name) names.add(name);
  }
  return names;
}

/** Everything wrong with one package's graph, as readable lines. Empty means nothing was found. */
export function findProblems(pkg, packagesText, featuresText) {
  const problems = [];
  const names = packageNames(packagesText);
  for (const crate of NETWORK_CRATES) if (names.has(crate)) problems.push(`${pkg}: the crate "${crate}" is compiled into the app`);
  if (/\btokio feature "net"/.test(featuresText)) problems.push(`${pkg}: tokio's "net" feature is on`);
  for (const m of featuresText.matchAll(NETWORK_WINDOWS_FEATURES)) problems.push(`${pkg}: the Windows networking feature "${m[1]}" is on`);
  return problems;
}

export function cargoTree(pkg, extra) {
  return execFileSync("cargo", ["tree", "-p", pkg, "--target", TARGET, "-e", "normal", ...extra], { encoding: "utf8", maxBuffer: 64 * 1024 * 1024 });
}

/** Runs the check and returns the exit code (0 clean, 1 problems). `tree` is how cargo is asked; tests pass a fake. */
export function main(tree = cargoTree) {
  const inCI = process.env.GITHUB_ACTIONS === "true";
  const all = [];
  const summary = [];
  for (const build of BUILDS) {
    const flags = build.features.length === 0 ? [] : ["--features", build.features.join(",")];
    const packagesText = tree(build.pkg, [...flags, "--prefix", "none", "--format", "{p}"]);
    const featuresText = tree(build.pkg, [...flags, "-e", "normal,features"]);
    const problems = findProblems(label(build), packagesText, featuresText);
    all.push(...problems);
    summary.push(`${label(build)}: ${packageNames(packagesText).size} crates`);
  }
  const line = `Windows target ${TARGET}, normal dependencies. ${summary.join("; ")}. ` +
    (all.length === 0 ? "No network crate, no tokio net feature, no Windows networking feature." : `${all.length} problem(s).`);
  console.log(line);
  for (const p of all) console.error(inCI ? `::error title=network crate::${p}` : `PROBLEM ${p}`);
  if (inCI && all.length === 0) console.log(`::notice title=network crates::${line}`);
  return all.length === 0 ? 0 : 1;
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) process.exitCode = main();
