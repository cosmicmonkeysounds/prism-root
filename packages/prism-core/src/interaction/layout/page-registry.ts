import { PageModel } from "./page-model.js";

export interface PageTypeDef<TTarget> {
  defaultViewMode: string;
  defaultTab: string;
  getObjectId?: (target: TTarget) => string | null;
}

export class PageRegistry<TTarget extends { kind: string }> {
  private defs = new Map<string, PageTypeDef<TTarget>>();

  register(kind: string, def: PageTypeDef<TTarget>): this {
    this.defs.set(kind, def);
    return this;
  }

  get(kind: string): PageTypeDef<TTarget> {
    return this.defs.get(kind) ?? { defaultViewMode: "list", defaultTab: "overview" };
  }

  createPage(target: TTarget, pageId?: string): PageModel<TTarget> {
    const def = this.get(target.kind);
    const id =
      pageId ?? `${target.kind}-${Date.now()}-${Math.random().toString(36).slice(2, 7)}`;
    return new PageModel<TTarget>({
      id,
      target,
      objectId: def.getObjectId?.(target) ?? null,
      defaultViewMode: def.defaultViewMode,
      defaultTab: def.defaultTab,
    });
  }

  has(kind: string): boolean {
    return this.defs.has(kind);
  }

  registeredKinds(): string[] {
    return [...this.defs.keys()];
  }
}
