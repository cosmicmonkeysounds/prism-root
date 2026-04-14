/**
 * BehaviorBoundary — wraps any Puck child and fires Luau behaviors.
 *
 * Listens for click / mount / change events on the wrapped subtree and
 * dispatches them through `kernel.behaviors.fire(objectId, trigger)`.
 * Used by `entity-puck-config.tsx` to attach behavior execution to
 * buttons, forms, and any other entity that declares interaction
 * triggers — the same code path runs in edit-mode preview and in
 * published runtime.
 *
 * The boundary swallows bubbling click events from its children by
 * catching them on the wrapper `<span>`. If a behavior script calls
 * `ui.navigate(...)` or similar, that runs via the injected globals
 * in `behavior-dispatcher.ts`. When no behaviors are bound to the
 * current object the wrapper renders children inline with no overhead.
 */

import { useCallback, useEffect, useRef } from "react";
import type { ReactNode } from "react";
import type { ObjectId } from "@prism/core/object-model";
import type { StudioKernel } from "../kernel/studio-kernel.js";

export interface BehaviorBoundaryProps {
  objectId: ObjectId | "";
  kernel: StudioKernel;
  children: ReactNode;
}

export function BehaviorBoundary({
  objectId,
  kernel,
  children,
}: BehaviorBoundaryProps) {
  const firedMount = useRef(false);

  // Fire onMount once per object id. StrictMode double-mount is
  // harmless because the dispatcher is idempotent on re-entry.
  useEffect(() => {
    if (!objectId || firedMount.current) return;
    firedMount.current = true;
    void kernel.behaviors.fire(objectId as ObjectId, "onMount");
  }, [objectId, kernel]);

  const onClickCapture = useCallback(
    (e: React.MouseEvent<HTMLSpanElement>) => {
      if (!objectId) return;
      const behaviors = kernel.behaviors.list(objectId as ObjectId, "onClick");
      if (behaviors.length === 0) return;
      // Prevent navigation/default submit so the Luau behavior owns the
      // click; ButtonRenderer already calls preventDefault() for its own
      // previews, but this guards against other wrapped targets.
      e.preventDefault();
      void kernel.behaviors.fire(objectId as ObjectId, "onClick", {
        clientX: e.clientX,
        clientY: e.clientY,
      });
    },
    [objectId, kernel],
  );

  return (
    <span data-behavior-boundary={objectId || undefined} onClickCapture={onClickCapture}>
      {children}
    </span>
  );
}
