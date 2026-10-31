import { invoke } from "@tauri-apps/api/core";
import { useEffect } from "react";
import { toast } from "sonner";

/** Tells the user, once, that their settings file could not be read at startup. */
export function useSettingsLoadIssue() {
  useEffect(() => {
    void invoke<string | null>("settings_take_load_issue").then((issue) => {
      if (issue) toast.error(issue, { duration: Number.POSITIVE_INFINITY });
    });
  }, []);
}
