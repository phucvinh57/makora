import { type PointerEvent, useRef, useState } from "react";

export const DEFAULT_COLUMN_WIDTHS = {
  app: 120,
  category: 224,
  site: 208,
  title: 280,
  start: 160,
  duration: 80,
} as const;

const MIN_COLUMN_WIDTHS: Record<ResizableColumn, number> = {
  app: 96,
  category: 144,
  site: 128,
  title: 160,
  start: 128,
  duration: 64,
};

export type ResizableColumn = keyof typeof DEFAULT_COLUMN_WIDTHS;

type HandleEvent = PointerEvent<HTMLButtonElement>;

/** Drag-to-resize column widths; double-click a handle to put the column back. */
export function useColumnResize() {
  const [widths, setWidths] = useState<Record<ResizableColumn, number>>(
    DEFAULT_COLUMN_WIDTHS,
  );
  const [resizing, setResizing] = useState<ResizableColumn | null>(null);
  const drag = useRef<{
    column: ResizableColumn;
    pointerId: number;
    startX: number;
    startWidth: number;
  } | null>(null);

  function start(column: ResizableColumn, event: HandleEvent) {
    if (event.button !== 0) return;

    event.preventDefault();
    event.currentTarget.setPointerCapture(event.pointerId);
    drag.current = {
      column,
      pointerId: event.pointerId,
      startX: event.clientX,
      startWidth: widths[column],
    };
    setResizing(column);
  }

  function move(event: HandleEvent) {
    const current = drag.current;
    if (!current || current.pointerId !== event.pointerId) return;

    event.preventDefault();
    setWidths((prev) => ({
      ...prev,
      [current.column]: Math.max(
        MIN_COLUMN_WIDTHS[current.column],
        current.startWidth + event.clientX - current.startX,
      ),
    }));
  }

  function end(event: HandleEvent) {
    if (drag.current?.pointerId !== event.pointerId) return;

    if (event.currentTarget.hasPointerCapture(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId);
    }
    drag.current = null;
    setResizing(null);
  }

  function reset(column: ResizableColumn) {
    setWidths((prev) => ({ ...prev, [column]: DEFAULT_COLUMN_WIDTHS[column] }));
  }

  return {
    widths,
    handleProps: (column: ResizableColumn) => ({
      "aria-label": `Resize ${column} column`,
      "data-resizing": resizing === column || undefined,
      onPointerDown: (event: HandleEvent) => start(column, event),
      onPointerMove: move,
      onPointerUp: end,
      onPointerCancel: end,
      onDoubleClick: () => reset(column),
    }),
  };
}
