import { useEffect, useState } from "react";

export type DialogName =
  | "add-category"
  | "add-rule"
  | "add-ignore-rule"
  | "add-query-whitelist"
  | "start-work-session"
  | "edit-work-session";

function eventName(name: DialogName) {
  return `makora:${name}`;
}

export function useDialog<D = undefined>(name: DialogName) {
  const [open, setOpen] = useState(false);
  const [data, setData] = useState<D | undefined>(undefined);

  useEffect(() => {
    const handleOpen = (event: CustomEvent<D>) => {
      setData(event.detail);
      setOpen(true);
    };

    window.addEventListener(eventName(name), handleOpen as EventListener);
    return () => {
      window.removeEventListener(eventName(name), handleOpen as EventListener);
    };
  }, [name]);

  return { open, data, setOpen };
}

export function openDialog<D = undefined>(
  name: DialogName,
  ...args: [D] extends [undefined] ? [] : [data: D]
) {
  const data = args[0];
  const event = new CustomEvent(
    eventName(name),
    data !== undefined ? { detail: data } : undefined,
  );
  window.dispatchEvent(event);
}
