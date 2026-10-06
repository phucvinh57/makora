import { useNavigate } from "@tanstack/react-router";
import { listen } from "@tauri-apps/api/event";
import { useEffect } from "react";

/** Follows the backend's `navigate` events, sent when a notification is clicked while the window is open. */
export function useNavigationRequests() {
  const navigate = useNavigate();

  useEffect(() => {
    const unlisten = listen<string>("navigate", ({ payload }) => {
      void navigate({ href: payload });
    });
    return () => {
      void unlisten.then((off) => off());
    };
  }, [navigate]);
}
