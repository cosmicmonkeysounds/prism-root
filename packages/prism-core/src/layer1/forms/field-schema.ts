export type FieldType =
  | "text"
  | "textarea"
  | "rich-text"
  | "number"
  | "currency"
  | "duration"
  | "rating"
  | "slider"
  | "boolean"
  | "date"
  | "datetime"
  | "url"
  | "email"
  | "phone"
  | "color"
  | "select"
  | "multi-select"
  | "tags"
  | "formula";

export interface SelectOption {
  value: string;
  label: string;
  color?: string;
}

export interface FieldSchema {
  id: string;
  label: string;
  type: FieldType;
  description?: string;
  placeholder?: string;
  required?: boolean;
  options?: SelectOption[];
  min?: number;
  max?: number;
  step?: number;
  unit?: string;
  maxLength?: number;
  pattern?: string;
  hidden?: boolean;
  readOnly?: boolean;
  section?: string;
  /** For type="formula" — the expression to evaluate at render time. */
  expression?: string;
}
