import { invoke } from "@tauri-apps/api/core";
import type { SessionsPage } from "./types";

export function listSessions(page: number, pageSize: number): Promise<SessionsPage> {
  return invoke("list_sessions", { page, pageSize });
}
