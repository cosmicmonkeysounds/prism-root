import type {
  SerializedPage,
  PageModelEvent,
  PageModelListener,
  PageModelOptions,
} from "./layout-types.js";
import { SelectionModel } from "./selection-model.js";

export class PageModel<TTarget = unknown> {
  readonly id: string;
  readonly target: TTarget;
  readonly objectId: string | null;
  readonly inputScopeId: string;
  readonly selection: SelectionModel;

  private _viewMode: string;
  private _activeTab: string;
  private _disposed = false;
  private listeners = new Set<PageModelListener>();

  constructor(options: PageModelOptions<TTarget>) {
    this.id = options.id;
    this.target = options.target;
    this.objectId = options.objectId;
    this._viewMode = options.defaultViewMode;
    this._activeTab = options.defaultTab;
    this.inputScopeId = `page:${options.id}`;
    this.selection = new SelectionModel();
  }

  get viewMode(): string {
    return this._viewMode;
  }

  setViewMode(mode: string): void {
    if (this._disposed || mode === this._viewMode) return;
    this._viewMode = mode;
    this.emit({ kind: "viewMode", mode });
  }

  get activeTab(): string {
    return this._activeTab;
  }

  setTab(tab: string): void {
    if (this._disposed || tab === this._activeTab) return;
    this._activeTab = tab;
    this.emit({ kind: "tab", tab });
  }

  get isDisposed(): boolean {
    return this._disposed;
  }

  dispose(): void {
    if (this._disposed) return;
    this._disposed = true;
    this.emit({ kind: "disposed" });
    this.listeners.clear();
  }

  on(listener: PageModelListener): () => void {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  }

  persist(): SerializedPage {
    return {
      id: this.id,
      target: this.target,
      objectId: this.objectId,
      viewMode: this._viewMode,
      activeTab: this._activeTab,
      selectedIds: this.selection.selectedIds,
    };
  }

  static fromSerialized<T>(
    data: SerializedPage,
    options: { defaultViewMode: string; defaultTab: string },
  ): PageModel<T> {
    const page = new PageModel<T>({
      id: data.id,
      target: data.target as T,
      objectId: data.objectId,
      defaultViewMode: data.viewMode ?? options.defaultViewMode,
      defaultTab: data.activeTab ?? options.defaultTab,
    });
    if (data.selectedIds.length > 0) {
      page.selection.selectAll(data.selectedIds);
    }
    return page;
  }

  private emit(event: PageModelEvent): void {
    for (const l of this.listeners) l(event);
  }
}
