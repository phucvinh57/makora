import { useCallback, useState } from "react";
import { readStorage, writeStorage } from "@/lib/storage";

export interface StorageCodec<T> {
  /** `undefined` rejects the stored text, so the fallback is used instead. */
  parse: (raw: string) => T | undefined;
  serialize: (value: T) => string;
}

export const FLAG_CODEC: StorageCodec<boolean> = {
  parse: (raw) => (raw === "1" ? true : raw === "0" ? false : undefined),
  serialize: (value) => (value ? "1" : "0"),
};

/** Like `useState`, but remembered across launches. The codec should be a module-level constant. */
export function usePersistentState<T>(
  key: string,
  fallback: T,
  codec: StorageCodec<T>,
): [T, (next: T) => void] {
  const [value, setValue] = useState<T>(() => {
    const raw = readStorage(key);
    return (raw === null ? undefined : codec.parse(raw)) ?? fallback;
  });

  const set = useCallback(
    (next: T) => {
      setValue(next);
      writeStorage(key, codec.serialize(next));
    },
    [key, codec],
  );

  return [value, set];
}
