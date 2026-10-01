import { useCallback, useState } from "react";

/**
 * State that starts over whenever `scope` changes — a selection that should not outlive the
 * filter or page it was made on. `initial` must be a stable value (a module-level constant).
 */
export function useScopedState<T>(
  scope: string,
  initial: T,
): [T, (update: (prev: T) => T) => void] {
  const [state, setState] = useState({ scope, value: initial });

  // Adjusting state while rendering is the supported way to derive a reset from a prop.
  if (state.scope !== scope) setState({ scope, value: initial });
  const value = state.scope === scope ? state.value : initial;

  const update = useCallback(
    (fn: (prev: T) => T) =>
      setState((prev) => ({
        scope,
        value: fn(prev.scope === scope ? prev.value : initial),
      })),
    [scope, initial],
  );

  return [value, update];
}
