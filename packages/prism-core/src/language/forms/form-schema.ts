import type { DocumentSchema } from "./document-schema.js";

export type ValidatorType =
  | "required"
  | "min"
  | "max"
  | "minLength"
  | "maxLength"
  | "pattern"
  | "custom";

export interface ValidationRule {
  type: ValidatorType;
  value?: number | string;
  message: string;
  validator?: (value: unknown, allValues: Record<string, unknown>) => boolean | Promise<boolean>;
}

export interface FieldValidation {
  fieldId: string;
  rules: ValidationRule[];
}

export type ConditionalOperator =
  | "eq"
  | "neq"
  | "gt"
  | "lt"
  | "gte"
  | "lte"
  | "includes"
  | "notEmpty"
  | "empty";

export interface FieldCondition {
  fieldId: string;
  operator: ConditionalOperator;
  value?: unknown;
}

export interface ConditionalRule {
  targetFieldId: string;
  showWhen: FieldCondition[];
}

export interface FormSchema extends DocumentSchema {
  validation?: FieldValidation[];
  conditional?: ConditionalRule[];
  submitLabel?: string;
  resetLabel?: string;
}
