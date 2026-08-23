// Bundles the extension into browser-extension/dist/, which is the directory
// browsers actually load — src/ and tsconfig.json are build inputs and are
// deliberately not shipped. Run with --watch for a rebuild-on-save loop.
//
// esbuild only strips types; run `bun run ext:check` to actually typecheck.
import { cp, mkdir, rm } from "node:fs/promises";
import path from "node:path";
import * as esbuild from "esbuild";

const root = import.meta.dirname;
const dist = path.join(root, "dist");
const watch = process.argv.includes("--watch");

async function copyStatic() {
  await cp(path.join(root, "manifest.json"), path.join(dist, "manifest.json"));
  await cp(path.join(root, "icons"), path.join(dist, "icons"), {
    recursive: true,
  });
}

await rm(dist, { recursive: true, force: true });
await mkdir(dist, { recursive: true });
await copyStatic();

/** @type {import("esbuild").BuildOptions} */
const options = {
  entryPoints: [path.join(root, "src/background.ts")],
  outfile: path.join(dist, "background.js"),
  bundle: true,
  // iife, not esm, so the worker needs no "type": "module" in the manifest.
  format: "iife",
  // Matches the oldest browser the manifest claims to support (Firefox 109).
  target: ["chrome109", "firefox109", "safari16"],
  logLevel: "info",
};

if (watch) {
  const ctx = await esbuild.context(options);
  await ctx.watch();
  console.log(`watching ${path.relative(process.cwd(), root)} ...`);
} else {
  await esbuild.build(options);
}
