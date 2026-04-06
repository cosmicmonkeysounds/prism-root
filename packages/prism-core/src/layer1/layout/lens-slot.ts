import type { SerializedPage, LensSlotEvent, LensSlotListener } from "./layout-types.js";
import type { PageRegistry } from "./page-registry.js";
import { PageModel } from "./page-model.js";

export interface LensSlotOptions<TTarget extends { kind: string }> {
  id: string;
  label?: string;
  registry: PageRegistry<TTarget>;
  initialTarget: TTarget;
  cacheSize?: number;
}

export class LensSlot<TTarget extends { kind: string }> {
  readonly id: string;
  readonly label: string;

  private registry: PageRegistry<TTarget>;
  private pageCache = new Map<string, PageModel<TTarget>>();
  private cacheSize: number;
  private _current: TTarget;
  private _backStack: TTarget[] = [];
  private _forwardStack: TTarget[] = [];
  private _activePage: PageModel<TTarget>;
  private listeners = new Set<LensSlotListener<TTarget>>();

  constructor(options: LensSlotOptions<TTarget>) {
    this.id = options.id;
    this.label = options.label ?? options.id;
    this.registry = options.registry;
    this.cacheSize = options.cacheSize ?? 10;
    this._current = options.initialTarget;
    this._activePage = this.getOrCreatePage(options.initialTarget);
  }

  get current(): TTarget {
    return this._current;
  }

  get activePage(): PageModel<TTarget> {
    return this._activePage;
  }

  get canGoBack(): boolean {
    return this._backStack.length > 0;
  }

  get canGoForward(): boolean {
    return this._forwardStack.length > 0;
  }

  go(target: TTarget): void {
    this._backStack.push(this._current);
    this._forwardStack = [];
    this._current = target;
    this._activePage = this.getOrCreatePage(target);
    this.emit({ kind: "navigated", target, page: this._activePage });
  }

  back(): boolean {
    if (this._backStack.length === 0) return false;
    this._forwardStack.push(this._current);
    this._current = this._backStack.pop() as TTarget;
    this._activePage = this.getOrCreatePage(this._current);
    this.emit({ kind: "back", target: this._current, page: this._activePage });
    return true;
  }

  forward(): boolean {
    if (this._forwardStack.length === 0) return false;
    this._backStack.push(this._current);
    this._current = this._forwardStack.pop() as TTarget;
    this._activePage = this.getOrCreatePage(this._current);
    this.emit({ kind: "forward", target: this._current, page: this._activePage });
    return true;
  }

  persistPages(): SerializedPage[] {
    return [...this.pageCache.values()].map((p) => p.persist());
  }

  on(listener: LensSlotListener<TTarget>): () => void {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  }

  dispose(): void {
    for (const page of this.pageCache.values()) page.dispose();
    this.pageCache.clear();
    this._backStack = [];
    this._forwardStack = [];
    this.listeners.clear();
  }

  private cacheKey(target: TTarget): string {
    return JSON.stringify(target);
  }

  private getOrCreatePage(target: TTarget): PageModel<TTarget> {
    const key = this.cacheKey(target);
    let page = this.pageCache.get(key);
    if (!page) {
      this.evictIfNeeded();
      page = this.registry.createPage(target);
      this.pageCache.set(key, page);
    }
    return page;
  }

  private evictIfNeeded(): void {
    if (this.pageCache.size < this.cacheSize) return;
    const oldest = this.pageCache.keys().next().value;
    if (oldest !== undefined) {
      this.pageCache.get(oldest)?.dispose();
      this.pageCache.delete(oldest);
    }
  }

  private emit(event: LensSlotEvent<TTarget>): void {
    for (const l of this.listeners) l(event);
  }
}
