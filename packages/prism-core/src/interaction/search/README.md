# search

Cross-collection search built from two layers. `SearchIndex` is a tokenized inverted index with TF-IDF scoring and field-weighted ranking (name > type/tags > description > data). `SearchEngine` composes the index with structured filters (types, tags, statuses, date range, collection scope), faceted counts, sort, pagination, and auto-reindexing via `CollectionStore` change subscriptions. Live subscriptions re-run the current query on any index change.

## Import

```ts
import {
  createSearchIndex,
  createSearchEngine,
  tokenize,
} from "@prism/core/search";
```

## Key exports

- `createSearchIndex({ weights?, minTokenLength? })` — returns `SearchIndex` with `add`/`remove`/`update`/`search`/`clear`/`size`.
- `createSearchEngine({ weights?, minTokenLength?, defaultLimit? })` — returns `SearchEngine` with `indexCollection`/`removeCollection`/`reindex`/`search`/`subscribe`/`dispose`.
- `tokenize(text, minLength?)` — lowercase tokenizer splitting on whitespace and punctuation.
- Types: `DocRef`, `IndexHit`, `FieldWeights`, `SearchIndexOptions`, `SearchIndex`, `SearchOptions`, `SearchHit`, `SearchFacets`, `SearchResult`, `SearchSubscriber`, `SearchEngineOptions`, `SearchEngine`.

## Usage

```ts
import { createSearchEngine } from "@prism/core/search";

const engine = createSearchEngine({ defaultLimit: 50 });
engine.indexCollection("tasks", tasksStore);
engine.indexCollection("contacts", contactsStore);

const result = engine.search({
  query: "invoice",
  types: ["task"],
  statuses: ["open"],
  sortBy: "relevance",
  limit: 20,
});

// result.hits, result.total, result.facets.types
```
