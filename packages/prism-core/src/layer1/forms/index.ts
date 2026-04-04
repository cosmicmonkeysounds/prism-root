export type { FieldType, SelectOption, FieldSchema } from "./field-schema.js";

export type { TextSection, FieldGroupSection, SectionDef, DocumentSchema } from "./document-schema.js";
export {
  isTextSection,
  isFieldGroupSection,
  getField,
  orderedFieldIds,
  orderedFields,
  NOTES_TEXT_SECTION,
  DESCRIPTION_TEXT_SECTION,
} from "./document-schema.js";

export type {
  ValidatorType,
  ValidationRule,
  FieldValidation,
  ConditionalOperator,
  FieldCondition,
  ConditionalRule,
  FormSchema,
} from "./form-schema.js";

export type { FormState } from "./form-state.js";
export {
  createFormState,
  setFieldValue,
  setFieldErrors,
  touchField,
  setAllErrors,
  setSubmitting,
  resetFormState,
  isDirty,
  isTouchedValid,
  fieldErrors,
  fieldHasVisibleError,
} from "./form-state.js";

export type { WikiToken } from "./wiki-link.js";
export {
  parseWikiLinks,
  extractLinkedIds,
  renderWikiLinks,
  buildWikiLink,
  detectInlineLink,
} from "./wiki-link.js";

export type { BlockToken, InlineToken } from "./markdown.js";
export { parseMarkdown, parseInline, inlineToPlainText, extractWikiIds } from "./markdown.js";
