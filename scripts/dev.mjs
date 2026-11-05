// The `tauri dev` frontend command. Tauri builds only the app binary, so this builds the
// browser bridge alongside it while Vite starts. Listing both packages resolves the same
// dependency features as the app build, so the two share compiled dependencies.
import { spawn } from "node:child_process";

spawn(
  "cargo",
  [
    "build",
    "--package",
    "makora",
    "--package",
    "makora-core",
    "--bin",
    "makora-bridge",
  ],
  { stdio: "inherit" },
);
spawn("bun", ["run", "dev"], { stdio: "inherit" }).on("exit", (code) =>
  process.exit(code ?? 1),
);
