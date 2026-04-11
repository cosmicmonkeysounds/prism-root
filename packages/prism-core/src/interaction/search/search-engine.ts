/**
 * SearchEngine — cross-collection search orchestrator.
 *
 * Composes SearchIndex (full-text TF-IDF) with structured ObjectQuery-style
 * filters to provide unified search across all open collections in a vault.
 *
 * Supports:
 *   - Full-text search with relevance scoring
 *   - Structured filters (type, tags, status, date range, collection)
 *   - Faceted results (counts by type, collection, tag)
 *   - Pagination (limit/offset)
 *   - Sort by relevance, name, date, createdAt, updatedAt
 *   - Auto-indexing via CollectionStore change subscriptions
 *   - Live search subscriptions (re-run on index changes)
 */

import type { GraphObject, ObjectId } from "@prism/core/object-model";
import type { CollectionStore } from "@prism/core/persistence";
import {
  createSearchIndex,
  type SearchIndex,
  type SearchIndexOptions,
} from "./search-index.js";

// ── Types ─────────────────────────────────────────────────────────────────────

export interface SearchOptions {
  /** Full-text search query. Empty string returns all (filtered by structured params). */
  query?: string;
  /** Filter by object type(s). */
  types?: string[];
  /** Filter by tags — object must have ALL listed tags. */
  tags?: string[];
  /** Filter by status value(s). */
  statuses?: string[];
  /** Limit to specific collection IDs. */
  collectionIds?: string[];
  /** Only objects with date >= this value. */
  dateAfter?: string;
  /** Only objects with date <= this value. */
  dateBefore?: string;
  /** Sort field. Default: 'relevance' when query is present, 'name' otherwise. */
  sortBy?: "relevance" | "name" | "date" | "createdAt" | "updatedAt";
  /** Sort direction. Default: 'desc' for relevance, 'asc' for others. */
  sortDir?: "asc" | "desc";
  /** Max results to return. Default: 50. */
  limit?: number;
  /** Skip this many results. Default: 0. */
  offset?: number;
  /** Include soft-deleted objects. Default: false. */
  includeDeleted?: boolean;
}

export interface SearchHit {
  objectId: ObjectId;
  collectionId: string;
  score: number;
  object: GraphObject;
}

export interface SearchFacets {
  /** Count of results per object type. */
  types: Record<string, number>;
  /** Count of results per collection. */
  collections: Record<string, number>;
  /** Count of results per tag. */
  tags: Record<string, number>;
}

export interface SearchResult {
  /** Paginated hits. */
  hits: SearchHit[];
  /** Total matching results (before pagination). */
  total: number;
  /** Faceted counts (computed from full result set, not just the page). */
  facets: SearchFacets;
}

export type SearchSubscriber = (result: SearchResult) => void;

export interface SearchEngineOptions extends SearchIndexOptions {
  /** Default page size. Default: 50. */
  defaultLimit?: number;
}

// ── SearchEngine ─────────────────────────────────────────────────────────────

export interface SearchEngine {
  /**
   * Index all objects in a collection. Subscribes to future changes
   * for automatic re-indexing.
   */
  indexCollection(collectionId: string, store: CollectionStore): void;

  /** Remove a collection from the index and unsubscribe from changes. */
  removeCollection(collectionId: string): void;

  /** Re-index all objects in a collection (clear + re-add). */
  reindex(collectionId: string, store: CollectionStore): void;

  /** Execute a search query. */
  search(options?: SearchOptions): SearchResult;

  /**
   * Subscribe to search results. The subscriber is called immediately with
   * current results and again whenever the index changes.
   * Returns the current SearchOptions and an unsubscribe function.
   */
  subscribe(
    options: SearchOptions,
    handler: SearchSubscriber,
  ): () => void;

  /** IDs of currently indexed collections. */
  readonly indexedCollections: string[];

  /** Total number of indexed documents across all collections. */
  readonly totalDocuments: number;

  /** The underlying SearchIndex (for advanced use). */
  readonly index: SearchIndex;
}

export function createSearchEngine(
  engineOptions?: SearchEngineOptions,
): SearchEngine {
  const index = createSearchIndex(engineOptions);
  const defaultLimit = engineOptions?.defaultLimit ?? 50;

  /** collectionId → CollectionStore (for object lookup during search) */
  const stores = new Map<string, CollectionStore>();

  /** collectionId → unsubscribe function from CollectionStore.onChange */
  const unsubs = new Map<string, () => void>();

  /** Live subscriptions: options + handler */
  const subscriptions = new Map<
    number,
    { options: SearchOptions; handler: SearchSubscriber }
  >();
  let nextSubId = 0;

  function notifySubscribers(): void {
    for (const { options, handler } of subscriptions.values()) {
      handler(executeSearch(options));
    }
  }

  function indexCollection(
    collectionId: string,
    store: CollectionStore,
  ): void {
    // Clean up existing subscription if re-indexing
    const existingUnsub = unsubs.get(collectionId);
    if (existingUnsub) existingUnsub();
    index.removeCollection(collectionId);

    stores.set(collectionId, store);

    // Bulk index all existing objects
    for (const obj of store.allObjects()) {
      index.add(collectionId, obj);
    }

    // Subscribe to future changes
    const unsub = store.onChange((changes) => {
      for (const change of changes) {
        if (change.type === "object-put") {
          const obj = store.getObject(change.id as ObjectId);
          if (obj) {
            index.update(collectionId, obj);
          }
        } else if (change.type === "object-remove") {
          index.remove(collectionId, change.id as ObjectId);
        }
      }
      notifySubscribers();
    });

    unsubs.set(collectionId, unsub);
    notifySubscribers();
  }

  function removeCollection(collectionId: string): void {
    const unsub = unsubs.get(collectionId);
    if (unsub) unsub();
    unsubs.delete(collectionId);
    stores.delete(collectionId);
    index.removeCollection(collectionId);
    notifySubscribers();
  }

  function reindex(collectionId: string, store: CollectionStore): void {
    indexCollection(collectionId, store);
  }

  function executeSearch(options?: SearchOptions): SearchResult {
    const opts = options ?? {};
    const query = opts.query?.trim() ?? "";
    const hasQuery = query.length > 0;

    // Step 1: Get candidate hits from the inverted index (or all docs)
    let candidates: Array<{
      objectId: ObjectId;
      collectionId: string;
      score: number;
    }>;

    if (hasQuery) {
      candidates = index.search(query);
    } else {
      // No text query — start with all indexed documents
      candidates = [];
      for (const [collectionId, store] of stores) {
        for (const obj of store.allObjects()) {
          candidates.push({ objectId: obj.id, collectionId, score: 0 });
        }
      }
    }

    // Step 2: Apply structured filters
    const filtered: Array<{
      objectId: ObjectId;
      collectionId: string;
      score: number;
      object: GraphObject;
    }> = [];

    for (const hit of candidates) {
      // Collection filter
      if (
        opts.collectionIds &&
        opts.collectionIds.length > 0 &&
        !opts.collectionIds.includes(hit.collectionId)
      ) {
        continue;
      }

      // Resolve the object
      const store = stores.get(hit.collectionId);
      if (!store) continue;
      const obj = store.getObject(hit.objectId);
      if (!obj) continue;

      // Soft-delete filter
      if (!opts.includeDeleted && obj.deletedAt) continue;

      // Type filter
      if (opts.types && opts.types.length > 0 && !opts.types.includes(obj.type)) {
        continue;
      }

      // Tag filter (AND — must have all)
      if (opts.tags && opts.tags.length > 0) {
        if (!opts.tags.every((t) => obj.tags.includes(t))) continue;
      }

      // Status filter
      if (opts.statuses && opts.statuses.length > 0) {
        if (!obj.status || !opts.statuses.includes(obj.status)) continue;
      }

      // Date range filter
      if (opts.dateAfter && obj.date && obj.date < opts.dateAfter) continue;
      if (opts.dateBefore && obj.date && obj.date > opts.dateBefore) continue;

      filtered.push({ ...hit, object: obj });
    }

    // Step 3: Compute facets from the full filtered set (before pagination)
    const facets: SearchFacets = { types: {}, collections: {}, tags: {} };
    for (const item of filtered) {
      facets.types[item.object.type] =
        (facets.types[item.object.type] ?? 0) + 1;
      facets.collections[item.collectionId] =
        (facets.collections[item.collectionId] ?? 0) + 1;
      for (const tag of item.object.tags) {
        facets.tags[tag] = (facets.tags[tag] ?? 0) + 1;
      }
    }

    // Step 4: Sort
    const sortBy = opts.sortBy ?? (hasQuery ? "relevance" : "name");
    const sortDir = opts.sortDir ?? (sortBy === "relevance" ? "desc" : "asc");
    const dir = sortDir === "desc" ? -1 : 1;

    filtered.sort((a, b) => {
      if (sortBy === "relevance") {
        return dir * (a.score - b.score);
      }
      const av = String(
        (a.object as unknown as Record<string, unknown>)[sortBy] ?? "",
      );
      const bv = String(
        (b.object as unknown as Record<string, unknown>)[sortBy] ?? "",
      );
      if (av < bv) return -dir;
      if (av > bv) return dir;
      return 0;
    });

    // Step 5: Paginate
    const total = filtered.length;
    const offset = opts.offset ?? 0;
    const limit = opts.limit ?? defaultLimit;
    const page = filtered.slice(offset, offset + limit);

    const hits: SearchHit[] = page.map((item) => ({
      objectId: item.objectId,
      collectionId: item.collectionId,
      score: item.score,
      object: item.object,
    }));

    return { hits, total, facets };
  }

  return {
    indexCollection,
    removeCollection,
    reindex,
    search: executeSearch,
    subscribe(
      options: SearchOptions,
      handler: SearchSubscriber,
    ): () => void {
      const id = nextSubId++;
      subscriptions.set(id, { options, handler });
      // Immediate callback with current results
      handler(executeSearch(options));
      return () => {
        subscriptions.delete(id);
      };
    },
    get indexedCollections(): string[] {
      return [...stores.keys()];
    },
    get totalDocuments(): number {
      return index.size();
    },
    index,
  };
}
