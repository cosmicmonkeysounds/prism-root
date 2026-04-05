export { createSearchIndex, tokenize } from "./search-index.js";

export type {
  DocRef,
  IndexHit,
  FieldWeights,
  SearchIndexOptions,
  SearchIndex,
} from "./search-index.js";

export { createSearchEngine } from "./search-engine.js";

export type {
  SearchOptions,
  SearchHit,
  SearchFacets,
  SearchResult,
  SearchSubscriber,
  SearchEngineOptions,
  SearchEngine,
} from "./search-engine.js";
