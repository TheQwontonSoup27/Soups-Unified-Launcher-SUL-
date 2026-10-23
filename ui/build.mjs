import { build, context } from "esbuild";
import { mkdirSync, copyFileSync, cpSync, existsSync } from "node:fs";

mkdirSync("dist", { recursive: true });
copyFileSync("index.html", "dist/index.html");
if (existsSync("public")) cpSync("public", "dist", { recursive: true });
const opts = {
  entryPoints: ["src/main.tsx"],
  bundle: true,
  outfile: "dist/app.js",
  jsx: "automatic",
  target: "es2022",
  minify: !process.argv.includes("--watch"),
  sourcemap: true,
  loader: { ".css": "css" },
  define: { "process.env.NODE_ENV": '"production"' },
  logLevel: "info",
};
if (process.argv.includes("--watch")) {
  const ctx = await context(opts);
  await ctx.watch();
  console.log("watching…");
} else {
  await build(opts);
}
