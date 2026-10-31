import { useCallback, useEffect, useState } from "react";
import type { AddCategoryDialogData } from "@/components/dialogs/AddCategoryDialog";
import type { AddModelDialogData } from "@/components/dialogs/AddModelDialog";
import type { DeleteCategoryDialogData } from "@/components/dialogs/DeleteCategoryDialog";
import type { EditCategoryDialogData } from "@/components/dialogs/EditCategoryDialog";
import type { ProviderDialogData } from "@/components/dialogs/ProviderDialog";
import type { RemoveProviderDialogData } from "@/components/dialogs/RemoveProviderDialog";
import type { RuleDialogData } from "@/components/dialogs/RuleDialog";

const OPEN_EVENT = "makora:open-dialog";

/** What each dialog is opened with; `undefined` for a dialog that takes nothing. */
export interface DialogDataMap {
  "add-category": AddCategoryDialogData;
  "add-model": AddModelDialogData;
  "delete-category": DeleteCategoryDialogData;
  "edit-category": EditCategoryDialogData;
  provider: ProviderDialogData;
  "remove-provider": RemoveProviderDialogData;
  "reset-categorization": undefined;
  rule: RuleDialogData;
}

export type DialogName = keyof DialogDataMap;

type OpenDialogArgs<N extends DialogName> = DialogDataMap[N] extends undefined
  ? []
  : [data: DialogDataMap[N]];

interface OpenDialogDetail {
  name: DialogName;
  data: unknown;
}

export function openDialog<N extends DialogName>(
  name: N,
  ...[data]: OpenDialogArgs<N>
) {
  window.dispatchEvent(
    new CustomEvent<OpenDialogDetail>(OPEN_EVENT, { detail: { name, data } }),
  );
}

/**
 * The state of the dialog `name`: it opens when `openDialog(name, data)` is called anywhere.
 * `data` outlives `open`, so the dialog keeps its content while it fades out. It is
 * `undefined` until the first open. Each name should have exactly one dialog mounted.
 */
export function useDialog<N extends DialogName>(name: N) {
  const [state, setState] = useState<{
    open: boolean;
    data: DialogDataMap[N] | undefined;
  }>({
    open: false,
    data: undefined,
  });

  useEffect(() => {
    function handleOpen(event: Event) {
      const { detail } = event as CustomEvent<OpenDialogDetail>;
      if (detail.name === name) {
        // openDialog's signature ties `data` to `name`, so this one is the map's entry.
        setState({ open: true, data: detail.data as DialogDataMap[N] });
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
