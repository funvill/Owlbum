// The toast queue. Any part of the page may call showToast; <Toasts /> draws
// the queue. docs/09-visual-design.md, "Toasts".

import { create } from "zustand";

export type ToastKind = "info" | "success" | "error";

export interface ToastAction {
  label: string;
  onAction: () => void;
}

export interface ToastInput {
  kind: ToastKind;
  message: string;
  /** At most one action, such as Undo. */
  action?: ToastAction;
}

export interface Toast extends ToastInput {
  /** Rises with each toast, so the newest is drawn last. */
  id: number;
}

/** At most this many show at once; a new one pushes out the oldest. */
export const maxToasts = 3;

interface ToastState {
  toasts: Toast[];
}

export const useToasts = create<ToastState>(() => ({ toasts: [] }));

let nextId = 1;

/** Whether two toasts say the same thing (same kind, words and action). */
function same(a: ToastInput, b: ToastInput): boolean {
  return a.kind === b.kind && a.message === b.message && a.action?.label === b.action?.label;
}

/**
 * Shows a toast and returns its id. An identical toast already on screen is
 * not repeated: it is replaced by this one, which restarts its timer.
 */
export function showToast(input: ToastInput): number {
  const id = nextId++;
  useToasts.setState((s) => ({
    toasts: [...s.toasts.filter((t) => !same(t, input)), { ...input, id }].slice(-maxToasts),
  }));
  return id;
}

/** Removes a toast. */
export function dismissToast(id: number): void {
  useToasts.setState((s) => ({ toasts: s.toasts.filter((t) => t.id !== id) }));
}
