import { useEffect } from "react";
import { toast } from "sonner";
import { settings } from "@/api";

/** Tells the user, once, that their settings file could not be read at startup. */
export function useSettingsLoadIssue() {
  useEffect(() => {
    void settings.takeLoadIssue().then((issue) => {
      if (issue) toast.error(issue, { duration: Number.POSITIVE_INFINITY });
    });
  }, []);
}
