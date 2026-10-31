// Bundles the extension into extensions/browser/dist/, which is the directory
// browsers actually load — src/ and tsconfig.json are build inputs and are
// deliberately not shipped. Run with --watch for a rebuild-on-save loop.
//
// Dev builds pin the unpacked extension's ID with DEV_PUBLIC_KEY so Makora's
// native-messaging host manifest can allow it (DEV_EXTENSION_ID in
// src-tauri/src/tracking/adapters/native_host.rs). --store leaves the key out:
// the Chrome Web Store assigns its own ID and rejects uploads that carry one.
//
// esbuild only strips types; run `bun run ext:check` to actually typecheck.
import { cp, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import path from "node:path";
import * as esbuild from "esbuild";

const root = import.meta.dirname;
const dist = path.join(root, "dist");
const watch = process.argv.includes("--watch");
const store = process.argv.includes("--store");

const DEV_PUBLIC_KEY =
  "MIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEAjS6067T+eBbOEQOfJOad3UO5w8bTlyPF2ArJeBn2d7KlROIUqUU4HM4mgVOfnOSWFmKzBbjJusY9FAZgxZnrOsMHuCyhYeDH2P2+3sH4KKiN9mw5lSK+KBJ2lMIlYiP8KlvpBeTuCyC5QEq65E0TtRdR5JdRy6FGlRd98c91kl3tfVCb5aSb3c0KH22RpVaRxNmuL4VcQRCdnChNFT6ho2lxqalgluI3IOLuTkE7eGpyoxJ2ITTh4mOZIoIROWdrW5lO2SvVfft4702Bf8bQ3juoZWj496SrWpS3dETM94P/GNFcuNO39cUUlFjbTWnOS29jBIgGGosn+w0Pe03zRQIDAQAB";

async function copyStatic() {
  const manifest = JSON.parse(
    await readFile(path.join(root, "manifest.json"), "utf8"),
  );
  if (!store) manifest.key = DEV_PUBLIC_KEY;
  await writeFile(
    path.join(dist, "manifest.json"),
    `${JSON.stringify(manifest, null, 2)}\n`,
  );
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
  // Matches the oldest browsers the manifest claims to support.
  target: ["chrome121", "firefox109", "safari16"],
  logLevel: "info",
};

if (watch) {
  const ctx = await esbuild.context(options);
  await ctx.watch();
  console.log(`watching ${path.relative(process.cwd(), root)} ...`);
} else {
  await esbuild.build(options);
}
