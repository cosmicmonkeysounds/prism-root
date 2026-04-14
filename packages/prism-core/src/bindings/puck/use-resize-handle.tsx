/**
 * Resize handle primitive.
 *
 * Generic drag-to-resize hook + a visual `ResizeHandle` component. Used by
 * the unified `Shell` widget to make any of its six bars resizable, but
 * exported so users can attach the same interaction to their own panes
 * without importing the full `Shell` Puck component.
 *
 * The implementation is ref-based on purpose: mutable drag inputs (axis,
 * direction, onCommit, current value) all live in refs so the global
 * pointer-listener effect can depend solely on `dragging`. If these were
 * in the dep array, `setValue` during a drag would remount the effect
 * mid-gesture, lose the pointerup listener, and leave the handle stuck
 * to the cursor after release.
 */

import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type CSSProperties,
  type PointerEvent as ReactPointerEvent,
} from "react";

const MIN_BAR = 0;
const MAX_BAR = 4000;

function clampBar(value: number): number {
  if (!Number.isFinite(value)) return 0;
  if (value < MIN_BAR) return MIN_BAR;
  if (value > MAX_BAR) return MAX_BAR;
  return Math.round(value);
}

export interface UseResizeHandleResult {
  /** Current (possibly mid-drag) value. */
  value: number;
  /** Sync the hook's internal value from props when not dragging. */
  setValueFromProps: (v: number) => void;
  /** Attach to the handle element's `onPointerDown`. */
  onPointerDown: (e: ReactPointerEvent) => void;
  /** True while the user is actively dragging. */
  dragging: boolean;
}

/**
 * Drag-to-resize hook.
 *
 * @param initial     Initial pixel value.
 * @param axis        "x" for column-resize, "y" for row-resize.
 * @param direction   +1 when dragging right/down grows the bar, -1 when it
 *                    shrinks (e.g. right-edge bars, bottom-edge bars).
 * @param onCommit    Fired on pointerup with the final clamped value. Pair
 *                    with a persistence call to write the new size back.
 */
export function useResizeHandle(
  initial: number,
  axis: "x" | "y",
  direction: 1 | -1,
  onCommit: ((value: number) => void) | undefined,
): UseResizeHandleResult {
  const [value, setValue] = useState<number>(initial);
  const [dragging, setDragging] = useState(false);
  const startRef = useRef<{ pointer: number; base: number } | null>(null);

  const axisRef = useRef(axis);
  const directionRef = useRef(direction);
  const onCommitRef = useRef(onCommit);
  const valueRef = useRef(value);
  axisRef.current = axis;
  directionRef.current = direction;
  onCommitRef.current = onCommit;
  valueRef.current = value;

  const setValueFromProps = useCallback((v: number) => {
    if (!startRef.current) setValue(v);
  }, []);

  const onPointerDown = useCallback((e: ReactPointerEvent) => {
    e.preventDefault();
    e.stopPropagation();
    (e.currentTarget as Element).setPointerCapture?.(e.pointerId);
    startRef.current = {
      pointer: axisRef.current === "x" ? e.clientX : e.clientY,
      base: valueRef.current,
    };
    setDragging(true);
  }, []);

  useEffect(() => {
    if (!dragging) return;
    const handleMove = (e: PointerEvent) => {
      const start = startRef.current;
      if (!start) return;
      const pointer = axisRef.current === "x" ? e.clientX : e.clientY;
      const delta = (pointer - start.pointer) * directionRef.current;
      const next = clampBar(start.base + delta);
      valueRef.current = next;
      setValue(next);
    };
    const handleUp = () => {
      const final = clampBar(valueRef.current);
      startRef.current = null;
      setDragging(false);
      onCommitRef.current?.(final);
    };
    window.addEventListener("pointermove", handleMove);
    window.addEventListener("pointerup", handleUp);
    window.addEventListener("pointercancel", handleUp);
    return () => {
      window.removeEventListener("pointermove", handleMove);
      window.removeEventListener("pointerup", handleUp);
      window.removeEventListener("pointercancel", handleUp);
    };
  }, [dragging]);

  return { value, setValueFromProps, onPointerDown, dragging };
}

export interface ResizeHandleProps {
  orientation: "horizontal" | "vertical";
  onPointerDown: (e: ReactPointerEvent) => void;
  active: boolean;
  style?: CSSProperties;
  /** Override the default active/inactive colours. */
  activeColor?: string;
  inactiveColor?: string;
}

/**
 * Visual drag handle paired with `useResizeHandle`. Positions itself
 * absolutely — the caller provides the edge offsets via `style`.
 */
export function ResizeHandle(props: ResizeHandleProps) {
  const {
    orientation,
    onPointerDown,
    active,
    style,
    activeColor = "#3b82f6",
    inactiveColor = "transparent",
  } = props;
  const isHorizontal = orientation === "horizontal";
  return (
    <div
      role="separator"
      aria-orientation={isHorizontal ? "vertical" : "horizontal"}
      onPointerDown={onPointerDown}
      data-testid={`shell-resize-${orientation}`}
      style={{
        position: "absolute",
        background: active ? activeColor : inactiveColor,
        transition: active ? "none" : "background 120ms",
        touchAction: "none",
        userSelect: "none",
        cursor: isHorizontal ? "col-resize" : "row-resize",
        zIndex: 5,
        ...style,
      }}
    />
  );
}
