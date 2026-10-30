// The Security & Trust page says SUL's own code makes no network requests. This test keeps that sentence true: it
// fails if network code appears in SUL's own source. It reads source text, so it is a tripwire and not a proof
// (the Tauri framework's own dependencies are outside it, which the page also says). The window's content policy and
// permissions have their own tests in dom-wire.test.tsx.
import assert from "node:assert/strict";
import { readdirSync, readFileSync, statSync } from "node:fs";
import { dirname, join, relative } from "node:path";
import { describe, test } from "node:test";
import { fileURLToPath } from "node:url";

const repo = join(dirname(fileURLToPath(import.meta.url)), "..", "..");

function files(dir: string, ext: RegExp): string[] {
  return readdirSync(dir).flatMap((name) => {
    const path = join(dir, name);
    if (name === "node_modules" || name === "target" || name === "dist") return [];
    return statSync(path).isDirectory() ? files(path, ext) : ext.test(name) ? [path] : [];
  });
}
const read = (path: string) => readFileSync(path, "utf8");
const rel = (path: string) => relative(repo, path).replace(/\\/g, "/");

const NETWORK_CRATES = ["reqwest", "hyper", "ureq", "curl", "isahc", "attohttpc", "surf", "tungstenite", "tokio-tungstenite", "websocket", "mio", "socket2", "native-tls", "rustls", "openssl", "h2", "quinn"];

describe("SUL's own Rust code", () => {
  const rust = [...files(join(repo, "crates"), /\.rs$/), ...files(join(repo, "src-tauri", "src"), /\.rs$/)];

  test("the search covers the source it is meant to", () => {
    assert.ok(rust.length >= 15, `found ${rust.length} Rust files`);
    assert.ok(rust.some((p) => rel(p) === "crates/sul-core/src/discord.rs"));
    assert.ok(rust.some((p) => rel(p) === "src-tauri/src/lib.rs"));
  });

  test("no network API is used", () => {
    const forbidden = /std::net|TcpStream|TcpListener|UdpSocket|ToSocketAddrs|\buse\s+(?:reqwest|hyper|ureq|curl|tokio)\b|\b(?:reqwest|hyper|ureq|curl)::/;
    for (const path of rust) assert.doesNotMatch(read(path), forbidden, rel(path));
  });

  test("no HTTP or socket crate is declared as a dependency", () => {
    const manifests = ["Cargo.toml", "crates/sul-core/Cargo.toml", "src-tauri/Cargo.toml"];
    for (const m of manifests) {
      const names = [...read(join(repo, m)).matchAll(/^\s*([A-Za-z0-9_-]+)\s*(?:=|\.)/gm)].map((x) => x[1]!.toLowerCase());
      for (const crate of NETWORK_CRATES) assert.ok(!names.includes(crate), `${m} declares ${crate}`);
    }
  });
});

describe("SUL's own window code", () => {
  const sources = files(join(repo, "ui", "src"), /\.(ts|tsx|css|html)$/);

  test("the search covers the source it is meant to", () => {
    assert.ok(sources.length >= 25, `found ${sources.length} files`);
  });

  test("the only request is for a file the app ships (the wallpaper list)", () => {
    const calls = sources.flatMap((p) => [...read(p).matchAll(/\bfetch\s*\(\s*([^)]*)\)/g)].map((m) => `${rel(p)}: ${m[1]!.trim()}`));
    assert.deepEqual(calls, ['ui/src/background.ts: "wallpapers/manifest.json"']);
  });

  test("no other way to reach a network is used, and no address is written into the code", () => {
    for (const path of sources) {
      const text = read(path);
      assert.doesNotMatch(text, /XMLHttpRequest|new\s+WebSocket|new\s+EventSource|sendBeacon|navigator\.connection|RTCPeerConnection/, rel(path));
      assert.doesNotMatch(text, /\b(?:https?|wss?|ftp):\/\//, `${rel(path)} contains an address`);
    }
    assert.doesNotMatch(read(join(repo, "ui", "index.html")), /\b(?:https?|wss?):\/\/|<link[^>]+href=["'](?!\/?[a-z])/i);
  });
});
