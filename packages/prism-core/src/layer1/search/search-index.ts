/**
 * SearchIndex — in-memory inverted index with TF-IDF scoring.
 *
 * Tokenizes GraphObject fields (name, description, tags, type, status, data values)
 * and builds an inverted index mapping tokens to document references. Supports:
 *
 *   - Add/remove/update documents
 *   - Full-text search with TF-IDF relevance scoring
 *   - Field-weighted scoring (name matches rank higher than data matches)
 *   - Per-collection scoping (each document carries its collectionId)
 *
 * This is a building block — SearchEngine composes it with structured filters.
 */

import type { GraphObject, ObjectId } from "../object-model/index.js";

// ── Types ─────────────────────────────────────────────────────────────────────

/** A reference to an indexed document. */
export interface DocRef {
  objectId: ObjectId;
  collectionId: string;
}

/** A single search hit with relevance score. */
export interface IndexHit {
  objectId: ObjectId;
  collectionId: string;
  score: number;
}

/** Field weight multipliers for scoring. */
export interface FieldWeights {
  name: number;
  description: number;
  type: number;
  tags: number;
  status: number;
  data: number;
}

export interface SearchIndexOptions {
  /** Custom field weights. Defaults: name=3, type=2, tags=2, status=1, description=1, data=0.5 */
  weights?: Partial<FieldWeights>;
  /** Minimum token length to index. Default: 2 */
  minTokenLength?: number;
}

// ── Tokenizer ─────────────────────────────────────────────────────────────────

const SPLIT_RE = /[\s\-_.,;:!?()[\]{}"'`/\\|@#$%^&*+=<>~]+/;

/**
 * Tokenize a string into lowercase terms.
 * Splits on whitespace and punctuation, filters by minimum length.
 */
export function tokenize(text: string, minLength = 2): string[] {
  return text
    .toLowerCase()
    .split(SPLIT_RE)
    .filter((t) => t.length >= minLength);
}

/**
 * Extract all indexable text from a GraphObject, keyed by field category.
 */
function extractFields(obj: GraphObject): Record<keyof FieldWeights, string[]> {
  const result: Record<keyof FieldWeights, string[]> = {
    name: [obj.name],
    description: [obj.description],
    type: [obj.type],
    tags: [...obj.tags],
    status: obj.status ? [obj.status] : [],
    data: [],
  };

  // Extract string values from data payload
  for (const val of Object.values(obj.data)) {
    if (typeof val === "string") {
      result.data.push(val);
    } else if (typeof val === "number" || typeof val === "boolean") {
      result.data.push(String(val));
    }
  }

  return result;
}

// ── Index internals ──────────────────────────────────────────────────────────

/** Per-document token frequency map, keyed by field. */
interface DocEntry {
  ref: DocRef;
  /** field → token → count */
  fieldTokens: Map<string, Map<string, number>>;
  /** Total token count across all fields (for TF normalization). */
  totalTokens: number;
}

/** Composite key for the document map. */
function docKey(collectionId: string, objectId: ObjectId): string {
  return `${collectionId}:${objectId}`;
}

// ── SearchIndex ──────────────────────────────────────────────────────────────

export interface SearchIndex {
  /** Add a document to the index. */
  add(collectionId: string, obj: GraphObject): void;
  /** Remove a document from the index. Returns true if it existed. */
  remove(collectionId: string, objectId: ObjectId): boolean;
  /** Update a document (remove + re-add). */
  update(collectionId: string, obj: GraphObject): void;
  /** Search the index. Returns hits sorted by descending score. */
  search(query: string): IndexHit[];
  /** Remove all documents for a collection. */
  removeCollection(collectionId: string): void;
  /** Clear the entire index. */
  clear(): void;
  /** Number of indexed documents. */
  size(): number;
}

export function createSearchIndex(options?: SearchIndexOptions): SearchIndex {
  const weights: FieldWeights = {
    name: 3,
    type: 2,
    tags: 2,
    status: 1,
    description: 1,
    data: 0.5,
    ...options?.weights,
  };
  const minTokenLength = options?.minTokenLength ?? 2;

  /** token → Set<docKey> */
  const invertedIndex = new Map<string, Set<string>>();

  /** docKey → DocEntry */
  const documents = new Map<string, DocEntry>();

  /** collectionId → Set<docKey> */
  const collectionDocs = new Map<string, Set<string>>();

  function indexDocument(collectionId: string, obj: GraphObject): void {
    const key = docKey(collectionId, obj.id);
    const fields = extractFields(obj);
    const fieldTokens = new Map<string, Map<string, number>>();
    let totalTokens = 0;

    for (const [field, texts] of Object.entries(fields)) {
      const freqMap = new Map<string, number>();
      for (const text of texts) {
        const tokens = tokenize(text, minTokenLength);
        for (const token of tokens) {
          freqMap.set(token, (freqMap.get(token) ?? 0) + 1);
          totalTokens++;

          // Add to inverted index
          let postings = invertedIndex.get(token);
          if (!postings) {
            postings = new Set();
            invertedIndex.set(token, postings);
          }
          postings.add(key);
        }
      }
      if (freqMap.size > 0) {
        fieldTokens.set(field, freqMap);
      }
    }

    const entry: DocEntry = {
      ref: { objectId: obj.id, collectionId },
      fieldTokens,
      totalTokens: Math.max(totalTokens, 1),
    };

    documents.set(key, entry);

    // Track per-collection
    let colSet = collectionDocs.get(collectionId);
    if (!colSet) {
      colSet = new Set();
      collectionDocs.set(collectionId, colSet);
    }
    colSet.add(key);
  }

  function removeDocument(collectionId: string, objectId: ObjectId): boolean {
    const key = docKey(collectionId, objectId);
    const entry = documents.get(key);
    if (!entry) return false;

    // Remove from inverted index
    for (const freqMap of entry.fieldTokens.values()) {
      for (const token of freqMap.keys()) {
        const postings = invertedIndex.get(token);
        if (postings) {
          postings.delete(key);
          if (postings.size === 0) {
            invertedIndex.delete(token);
          }
        }
      }
    }

    documents.delete(key);

    // Remove from collection tracking
    const colSet = collectionDocs.get(collectionId);
    if (colSet) {
      colSet.delete(key);
      if (colSet.size === 0) {
        collectionDocs.delete(collectionId);
      }
    }

    return true;
  }

  function search(query: string): IndexHit[] {
    const queryTokens = tokenize(query, minTokenLength);
    if (queryTokens.length === 0) return [];

    const N = documents.size;
    if (N === 0) return [];

    // Gather candidate documents from all query tokens
    const scores = new Map<string, number>();

    for (const qToken of queryTokens) {
      const postings = invertedIndex.get(qToken);
      if (!postings) continue;

      // IDF with smoothing: log(1 + N / df) — avoids zero when N === df
      const idf = Math.log(1 + N / postings.size);

      for (const key of postings) {
        const entry = documents.get(key);
        if (!entry) continue;

        // Compute weighted TF across all fields
        let weightedTf = 0;
        for (const [field, freqMap] of entry.fieldTokens) {
          const freq = freqMap.get(qToken);
          if (freq !== undefined) {
            const tf = freq / entry.totalTokens;
            const w = weights[field as keyof FieldWeights] ?? 1;
            weightedTf += tf * w;
          }
        }

        const score = weightedTf * idf;
        scores.set(key, (scores.get(key) ?? 0) + score);
      }
    }

    // Build sorted results
    const hits: IndexHit[] = [];
    for (const [key, score] of scores) {
      const entry = documents.get(key);
      if (entry) {
        hits.push({
          objectId: entry.ref.objectId,
          collectionId: entry.ref.collectionId,
          score,
        });
      }
    }

    hits.sort((a, b) => b.score - a.score);
    return hits;
  }

  return {
    add: indexDocument,
    remove: removeDocument,
    update(collectionId: string, obj: GraphObject): void {
      removeDocument(collectionId, obj.id);
      indexDocument(collectionId, obj);
    },
    search,
    removeCollection(collectionId: string): void {
      const colSet = collectionDocs.get(collectionId);
      if (!colSet) return;
      // Copy keys since we mutate during iteration
      const keys = [...colSet];
      for (const key of keys) {
        const entry = documents.get(key);
        if (entry) {
          removeDocument(collectionId, entry.ref.objectId);
        }
      }
    },
    clear(): void {
      invertedIndex.clear();
      documents.clear();
      collectionDocs.clear();
    },
    size(): number {
      return documents.size;
    },
  };
}
