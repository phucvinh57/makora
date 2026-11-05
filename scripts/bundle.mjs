// Builds the release binaries, makora and the makora-bridge it registers from its own
// directory, into <target>/release. Installer packaging (deb/rpm, app/dmg, nsis/msi) is not
// wired up yet, so after the build this exits with an error. Accepts `--target <triple>`.
import { execFileSync } from "node:child_process";
import path from "node:path";

const root = path.join(import.meta.dirname, "..");
const args = process.argv.slice(2);
const targetIndex = args.indexOf("--target");
const target = targetIndex >= 0 ? args[targetIndex + 1] : undefined;
const targetArgs = target ? ["--target", target] : [];

execFileSync(
  "cargo",
  ["build", "--release", "--workspace", "--bins", ...targetArgs],
  { cwd: root, stdio: "inherit" },
);

console.error(
  "Installer packaging is not implemented yet: only the release binaries were built.",
);
process.exit(1);
