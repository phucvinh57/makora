import { type ClassValue, clsx } from "clsx";
import { twMerge } from "tailwind-merge";

/** `value` when it is one of `choices`, for narrowing a select's or a URL's string. */
export function parseOneOf<T extends string>(
  choices: readonly T[],
  value: unknown,
): T | undefined {
  return choices.find((choice) => choice === value);
}

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}
