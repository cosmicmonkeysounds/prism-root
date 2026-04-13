# forms/

Form and document schemas, form state, wiki-link parsing, and the canonical markdown tokenizer. This is the single source of markdown parsing in the codebase — `@prism/core/markdown` and everything else re-uses `parseMarkdown` from here.

```ts
import { createFormState, parseMarkdown, parseWikiLinks } from '@prism/core/forms';
```

## Key exports

- `FieldType` / `FieldSchema` / `SelectOption` — field descriptors driving form rendering and validation.
- `DocumentSchema` / `SectionDef` / `TextSection` / `FieldGroupSection` — document shape combining text sections and field groups. Helpers: `isTextSection`, `isFieldGroupSection`, `getField`, `orderedFieldIds`, `orderedFields`. Canonical sections: `NOTES_TEXT_SECTION`, `DESCRIPTION_TEXT_SECTION`.
- `FormSchema` / `ValidationRule` / `FieldValidation` / `ConditionalRule` / `FieldCondition` / `ValidatorType` / `ConditionalOperator` — declarative validation and conditional visibility.
- `FormState` + `createFormState`, `setFieldValue`, `setFieldErrors`, `touchField`, `setAllErrors`, `setSubmitting`, `resetFormState`, `isDirty`, `isTouchedValid`, `fieldErrors`, `fieldHasVisibleError` — immutable form-state record and transforms.
- `parseWikiLinks`, `extractLinkedIds`, `renderWikiLinks`, `buildWikiLink`, `detectInlineLink`, `WikiToken` — `[[wiki-link]]` parser used by markdown, prose codec, and search.
- `parseMarkdown`, `parseInline`, `inlineToPlainText`, `extractWikiIds`, `BlockToken`, `InlineToken` — the canonical block + inline markdown tokenizer.

## Usage

```ts
import { createFormState, setFieldValue, parseMarkdown } from '@prism/core/forms';

let state = createFormState({ title: '' });
state = setFieldValue(state, 'title', 'Hello', '');

const blocks = parseMarkdown('# Heading\n\nhi');
// blocks[0] => { kind: 'h1', text: 'Heading' }
```
