import { useCallback, useEffect, useState } from "react";

const OPEN_EVENT = "makora:open-dialog";

/** Every dialog in the app. A new one is added here, then mounted in `Dialogs`. */
export type DialogName =
  | "add-category"
  | "add-model"
  | "delete-category"
  | "reset-categorization"
  | "rule";

interface OpenDialogDetail {
  name: DialogName;
  data: unknown;
}

/**
 * Asks the dialog registered under `name` to open, handing it `data`. Whoever calls this needs
 * no reference to the dialog, only its name and the shape of its data.
 */
export function openDialog<T = undefined>(name: DialogName, data?: T) {
  window.dispatchEvent(
    new CustomEvent<OpenDialogDetail>(OPEN_EVENT, { detail: { name, data } }),
  );
}

/**
 * The state of the dialog `name`: it opens when `openDialog(name, data)` is called anywhere.
 * `data` outlives `open`, so the dialog keeps its content while it fades out. It is
 * `undefined` until the first open. Each name should have exactly one dialog mounted.
 */
export function useDialog<T = undefined>(name: DialogName) {
  const [state, setState] = useState<{ open: boolean; data: T | undefined }>({
    open: false,
    data: undefined,
  });

  useEffect(() => {
    function handleOpen(event: Event) {
      const { detail } = event as CustomEvent<OpenDialogDetail>;
      if (detail.name === name) {
        setState({ open: true, data: detail.data as T | undefined });
      }
    }
    window.addEventListener(OPEN_EVENT, handleOpen);
    return () => window.removeEventListener(OPEN_EVENT, handleOpen);
  }, [name]);

  const setOpen = useCallback(
    (open: boolean) => setState((prev) => ({ ...prev, open })),
    [],
  );

  return { open: state.open, setOpen, data: state.data };
}
