// Builds the release installers with the browser bridge bundled as a Tauri sidecar.
// One `tauri build` cannot do it: tauri-build fails when a declared sidecar is missing, and
// the bridge is a binary of the very crate it would be building. So build every binary
// first, stage the bridge under the name the bundler expects, then bundle with the sidecar
// declared in tauri.bundle.conf.json. Accepts `--target <triple>`.
import { execFileSync } from "node:child_process";
import { copyFileSync, mkdirSync } from "node:fs";
import path from "node:path";

const root = path.join(import.meta.dirname, "..");
const args = process.argv.slice(2);
const targetIndex = args.indexOf("--target");
const target = targetIndex >= 0 ? args[targetIndex + 1] : undefined;
const targetArgs = target ? ["--target", target] : [];
const triple =
  target ??
  /^host: (.+)$/m.exec(execFileSync("rustc", ["-vV"], { encoding: "utf8" }))[1];
const exe = triple.includes("windows") ? ".exe" : "";
const targetDir =
  process.env.CARGO_TARGET_DIR ?? path.join(root, "src-tauri", "target");

const tauri = (...command) =>
  execFileSync("bun", ["tauri", ...command], { cwd: root, stdio: "inherit" });

tauri("build", "--no-bundle", ...targetArgs);

const binaries = path.join(root, "src-tauri", "binaries");
mkdirSync(binaries, { recursive: true });
copyFileSync(
  path.join(targetDir, target ?? "", "release", `makora-bridge${exe}`),
  path.join(binaries, `makora-bridge-${triple}${exe}`),
);

tauri(
  "bundle",
  "--config",
  path.join("src-tauri", "tauri.bundle.conf.json"),
  ...targetArgs,
);
