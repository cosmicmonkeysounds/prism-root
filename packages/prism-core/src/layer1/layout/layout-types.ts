import type { PageModel } from "./page-model.js";

export interface SerializedPage {
  id: string;
  target: unknown;
  objectId: string | null;
  viewMode: string;
  activeTab: string;
  selectedIds: string[];
}

export type PageModelEvent =
  | { kind: "viewMode"; mode: string }
  | { kind: "tab"; tab: string }
  | { kind: "disposed" };

export type PageModelListener = (event: PageModelEvent) => void;

export interface PageModelOptions<TTarget> {
  id: string;
  target: TTarget;
  objectId: string | null;
  defaultViewMode: string;
  defaultTab: string;
}

export type SelectionEvent = { selected: ReadonlySet<string>; primary: string | null };
export type SelectionListener = (event: SelectionEvent) => void;

export type WorkspaceSlotEvent<TTarget> =
  | { kind: "navigated"; target: TTarget; page: PageModel<TTarget> }
  | { kind: "back"; target: TTarget; page: PageModel<TTarget> }
  | { kind: "forward"; target: TTarget; page: PageModel<TTarget> };

export type WorkspaceSlotListener<TTarget> = (event: WorkspaceSlotEvent<TTarget>) => void;

export type WorkspaceManagerEvent =
  | { kind: "slot-opened"; slotId: string }
  | { kind: "slot-closed"; slotId: string }
  | { kind: "slot-focused"; slotId: string };

export type WorkspaceManagerListener = (
  event: WorkspaceManagerEvent,
) => void;
