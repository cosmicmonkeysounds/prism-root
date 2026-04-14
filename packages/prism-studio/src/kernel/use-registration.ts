/**
 * `useRegistration` — the one canonical "validate → exists? → register →
 * notify → reset" flow that a handful of Studio builder panels all used to
 * re-implement by hand.
 *
 * Before this hook existed, `entity-builder-panel.tsx`,
 * `relationship-builder-panel.tsx`, and `schema-designer-panel.tsx` each
 * carried ~15 lines of the same sequence:
 *
 *   1. Trim-check the form fields and bail with a warning toast.
 *   2. Look the identity up in the relevant registry; if it's taken,
 *      bail with a warning toast.
 *   3. Build the def and hand it to the registry.
 *   4. Emit a success toast.
 *   5. Reset the form state.
 *
 * The only per-panel variance was *which* registry accessor to call
 * (`kernel.registry.get` vs `getEdgeType`, `register` vs `registerEdge`)
 * and the wording of the toasts. Everything else was copy-paste, so
 * cross-cutting changes (e.g. a new notification field) required three
 * edits. This hook takes those variances as callbacks and centralises
 * the flow.
 */

import { useCallback } from "react";
import { useKernel } from "./kernel-context.js";

/**
 * Minimal kernel surface `buildRegistration` actually touches. The real
 * `StudioKernel` satisfies this shape by subset, and tests can hand in a
 * trivial `{ notifications: { add } }` stub without mounting React.
 */
export interface RegistrationKernel {
  readonly notifications: {
    add(notification: { title: string; kind: "warning" | "success" }): void;
  };
}

/**
 * Configuration for `useRegistration` / `buildRegistration`. Generic
 * over `TDef` so the flow works against `EntityDef`, `EdgeTypeDef`, or
 * any future registry entry type without an extra adapter layer.
 */
export interface UseRegistrationOptions<TDef> {
  /**
   * Short noun used in success / conflict notifications, e.g.
   * `"entity type"` or `"relationship"`. Falls back to `"entry"`.
   */
  readonly noun?: string;
  /**
   * Return a human-friendly name for a def. Used in the notification
   * copy (`Registered <noun> "<name>"`).
   */
  readonly name: (def: TDef) => string;
  /**
   * Optional pre-flight validator run before the existence check.
   * Return a non-empty string to block with a warning toast, or
   * `null`/`undefined` to proceed.
   */
  readonly validate?: (def: TDef) => string | null | undefined;
  /** Imperative existence check: does the registry already hold this id? */
  readonly exists: (def: TDef) => boolean;
  /** Imperative register call — wires the def into the real registry. */
  readonly register: (def: TDef) => void;
  /**
   * Optional post-success callback. Typical use: reset form state so the
   * panel is ready for the next entry. Not called on validation failure.
   */
  readonly onSuccess?: (def: TDef) => void;
}

/**
 * Pure factory that builds the registration callback against any kernel
 * surface that speaks the `RegistrationKernel` shape. Exported so tests
 * can drive the pipeline without mounting a React tree.
 *
 * The returned `register(def)` returns `true` on success and `false`
 * when validation or the uniqueness check blocked the write.
 */
export function buildRegistration<TDef>(
  kernel: RegistrationKernel,
  options: UseRegistrationOptions<TDef>,
): (def: TDef) => boolean {
  const { noun, name, validate, exists, register, onSuccess } = options;
  return (def: TDef) => {
    const error = validate?.(def);
    if (error) {
      kernel.notifications.add({ title: error, kind: "warning" });
      return false;
    }
    if (exists(def)) {
      kernel.notifications.add({
        title: `${capitalise(noun ?? "entry")} "${name(def)}" already exists`,
        kind: "warning",
      });
      return false;
    }
    register(def);
    kernel.notifications.add({
      title: `Registered ${noun ?? "entry"} "${name(def)}"`,
      kind: "success",
    });
    onSuccess?.(def);
    return true;
  };
}

/**
 * React hook wrapper over `buildRegistration`. Pulls the kernel from
 * context and memoises the resulting callback so stable-identity
 * consumers don't churn between renders.
 */
export function useRegistration<TDef>(
  options: UseRegistrationOptions<TDef>,
): (def: TDef) => boolean {
  const kernel = useKernel();
  const { noun, name, validate, exists, register, onSuccess } = options;
  return useCallback(
    (def: TDef) => {
      const run = buildRegistration<TDef>(kernel, {
        ...(noun !== undefined ? { noun } : {}),
        name,
        ...(validate !== undefined ? { validate } : {}),
        exists,
        register,
        ...(onSuccess !== undefined ? { onSuccess } : {}),
      });
      return run(def);
    },
    [kernel, noun, name, validate, exists, register, onSuccess],
  );
}

function capitalise(s: string): string {
  const first = s[0];
  if (first === undefined) return s;
  return first.toUpperCase() + s.slice(1);
}
