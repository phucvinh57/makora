// Builds the release installers: deb and rpm on Linux, app and dmg on macOS, nsis and msi on
// Windows. The app registers the makora-bridge next to its own executable, so every package
// ships both binaries, built first in one cargo invocation. Accepts `--target <triple>`.
// Linux packages land in <target dir>/debian and <target dir>/generate-rpm (under
// <triple>/ with --target), the others in <target dir>/packager.
import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync } from "node:fs";
import path from "node:path";

const root = path.join(import.meta.dirname, "..");
const args = process.argv.slice(2);
const targetIndex = args.indexOf("--target");
const target = targetIndex >= 0 ? args[targetIndex + 1] : undefined;
if (targetIndex >= 0 && !target) {
  throw new Error("--target needs a target triple");
}
const targetArgs = target ? ["--target", target] : [];
const triple =
  target ??
  /^host: (.+)$/m.exec(execFileSync("rustc", ["-vV"], { encoding: "utf8" }))[1];
const targetDir = path.resolve(root, process.env.CARGO_TARGET_DIR ?? "target");
const env = { ...process.env };
let buildDrive;

// llama.cpp's nested Vulkan build exceeds MSVC's path limit in long checkouts.
if (process.platform === "win32") {
  for (const letter of "ZYXWVUTSRQPONMLKJIHGFED") {
    const drive = `${letter}:`;
    if (existsSync(`${drive}\\`)) continue;
    const buildDir = path.join(targetDir, `bundle-${letter.toLowerCase()}`);
    mkdirSync(buildDir, { recursive: true });
    try {
      execFileSync("subst", [drive, buildDir], { stdio: "pipe" });
      buildDrive = drive;
      env.CARGO_TARGET_DIR = `${drive}\\`;
      break;
    } catch {
      // A drive can be reserved without a mounted filesystem.
    }
  }
  if (!buildDrive)
    throw new Error("No free drive for the Windows build directory");
}

const cargo = (...commandArgs) =>
  execFileSync("cargo", commandArgs, { cwd: root, stdio: "inherit", env });

try {
  cargo("build", "--release", "--workspace", "--bins", ...targetArgs);

  if (triple.includes("linux")) {
    cargo("deb", "--package", "makora", "--no-build", ...targetArgs);
    cargo("generate-rpm", "--package", "crates/makora", ...targetArgs);
  } else if (triple.includes("apple-darwin") || triple.includes("windows")) {
    const formats = triple.includes("windows") ? "nsis,wix" : "app,dmg";
    cargo(
      "packager",
      "--release",
      "--packages",
      "makora",
      "--formats",
      formats,
      "--out-dir",
      path.join(targetDir, "packager"),
      ...targetArgs,
    );
  } else {
    throw new Error(`No installer packaging for ${triple}`);
  }
} finally {
  if (buildDrive) execFileSync("subst", [buildDrive, "/D"], { stdio: "pipe" });
}
