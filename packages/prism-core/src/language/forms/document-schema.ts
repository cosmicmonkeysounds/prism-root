import type { FieldSchema } from "./field-schema.js";

export interface TextSection {
  id: string;
  label?: string;
  kind: "text";
  dynamic?: boolean;
}

export interface FieldGroupSection {
  id: string;
  label?: string;
  kind: "field-group";
  fieldIds: string[];
  columns?: 1 | 2 | 3;
}

export type SectionDef = TextSection | FieldGroupSection;

export function isTextSection(s: SectionDef): s is TextSection {
  return s.kind === "text";
}

export function isFieldGroupSection(s: SectionDef): s is FieldGroupSection {
  return s.kind === "field-group";
}

export interface DocumentSchema {
  id: string;
  name: string;
  fields: FieldSchema[];
  sections: SectionDef[];
}

export function getField(schema: DocumentSchema, fieldId: string): FieldSchema | undefined {
  return schema.fields.find((f) => f.id === fieldId);
}

export function orderedFieldIds(schema: DocumentSchema): string[] {
  const ids: string[] = [];
  for (const section of schema.sections) {
    if (section.kind === "field-group") {
      ids.push(...section.fieldIds);
    }
  }
  return ids;
}

export function orderedFields(schema: DocumentSchema): FieldSchema[] {
  return orderedFieldIds(schema)
    .map((id) => getField(schema, id))
    .filter((f): f is FieldSchema => f !== undefined);
}

export const NOTES_TEXT_SECTION: SectionDef = {
  id: "notes",
  label: "Notes",
  kind: "text",
  dynamic: true,
};

export const DESCRIPTION_TEXT_SECTION: SectionDef = {
  id: "description",
  label: "Description",
  kind: "text",
  dynamic: false,
};
