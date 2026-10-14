import { clsx } from "clsx";
import { twMerge } from "tailwind-merge";

/** Class-name helper shadcn-svelte components expect. */
export function cn(...inputs) {
  return twMerge(clsx(inputs));
}
