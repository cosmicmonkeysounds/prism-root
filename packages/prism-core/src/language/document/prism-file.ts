/**
 * PrismFile — the single file/document abstraction that bridges
 * persistence, syntax, and rendering.
 *
 * Introduced by ADR-002 §A1. Prior to PrismFile a "file" was any of:
 * raw `string`, `LoroText`, `GraphObject`, `BinaryRef`, or `DocumentSchema`,
 * each living in its own subsystem with no shared contract. `PrismFile`
 * wraps those bodies in a discriminated union so that Surfaces, Syntax,
 * Codegen, and Persistence can all agree on "what a file is".
 *
 * Phase 1 of ADR-002 introduces this type additively — nothing consumes
 * it yet. Phase 4 unifies the language + document registries and wires
 * `DocumentSurface` through `PrismFile`.
 */

import type { LoroText } from "loro-crdt";
import type { GraphObject } from "@prism/core/object-model";
import type { BinaryRef } from "@prism/core/vfs";
import type { DocumentSchema } from "@prism/core/forms";

// ── FileBody ────────────────────────────────────────────────────────────────

/**
 * The payload of a `PrismFile`. Discriminated by `kind` so that surfaces
 * and syntax can narrow on the body shape without reaching into the
 * CRDT / VFS / graph layers directly.
 *
 * - `text`   — plain string or a `LoroText` for CRDT-synced editing.
 * - `graph`  — a `GraphObject` rooted in the object-model (records, boards,
 *              spatial canvases). Editors project the graph into whichever
 *              surface the user has open.
 * - `binary` — a `BinaryRef` pointing into the VFS (images, audio, CAD).
 */
export type FileBody =
  | { kind: "text"; ref: LoroText | string }
  | { kind: "graph"; ref: GraphObject }
  | { kind: "binary"; ref: BinaryRef };

// ── PrismFile ───────────────────────────────────────────────────────────────

/**
 * A unified file/document record.
 *
 * `languageId` resolves a `LanguageContribution` (parse / serialize /
 * syntax provider / surface renderers). `surfaceId` is an explicit
 * override for cases where a single language supports multiple surfaces
 * and the caller wants to pick one up front; otherwise the surface is
 * derived from `languageId`.
 *
 * `schema` is carried alongside the body rather than stuffed inside it so
 * form-driven files (YAML/JSON with a known schema, Flux records) can
 * share the same file abstraction as free-form documents.
 */
export interface PrismFile {
  /** NSID or VFS path. The primary identity of the file. */
  path: string;
  /** Language contribution id. Resolves parse/serialize/surface. */
  languageId?: string;
  /** Explicit surface override; defaults to the language's `defaultMode`. */
  surfaceId?: string;
  /** The actual content, discriminated by `kind`. */
  body: FileBody;
  /** Optional form/field schema for structured editing. */
  schema?: DocumentSchema;
  /** Free-form metadata bag (owner, tags, custom keys). */
  metadata?: Record<string, unknown>;
}

// ── Narrowing helpers ───────────────────────────────────────────────────────

/** Type guard: narrow a `FileBody` to the `text` variant. */
export function isTextBody(
  body: FileBody,
): body is Extract<FileBody, { kind: "text" }> {
  return body.kind === "text";
}

/** Type guard: narrow a `FileBody` to the `graph` variant. */
export function isGraphBody(
  body: FileBody,
): body is Extract<FileBody, { kind: "graph" }> {
  return body.kind === "graph";
}

/** Type guard: narrow a `FileBody` to the `binary` variant. */
export function isBinaryBody(
  body: FileBody,
): body is Extract<FileBody, { kind: "binary" }> {
  return body.kind === "binary";
}

// ── Constructors ────────────────────────────────────────────────────────────

/** Build a `PrismFile` with a text body. */
export function createTextFile(params: {
  path: string;
  text: LoroText | string;
  languageId?: string;
  surfaceId?: string;
  schema?: DocumentSchema;
  metadata?: Record<string, unknown>;
}): PrismFile {
  const file: PrismFile = {
    path: params.path,
    body: { kind: "text", ref: params.text },
  };
  if (params.languageId !== undefined) file.languageId = params.languageId;
  if (params.surfaceId !== undefined) file.surfaceId = params.surfaceId;
  if (params.schema !== undefined) file.schema = params.schema;
  if (params.metadata !== undefined) file.metadata = params.metadata;
  return file;
}

/** Build a `PrismFile` with a graph body. */
export function createGraphFile(params: {
  path: string;
  object: GraphObject;
  languageId?: string;
  surfaceId?: string;
  schema?: DocumentSchema;
  metadata?: Record<string, unknown>;
}): PrismFile {
  const file: PrismFile = {
    path: params.path,
    body: { kind: "graph", ref: params.object },
  };
  if (params.languageId !== undefined) file.languageId = params.languageId;
  if (params.surfaceId !== undefined) file.surfaceId = params.surfaceId;
  if (params.schema !== undefined) file.schema = params.schema;
  if (params.metadata !== undefined) file.metadata = params.metadata;
  return file;
}

/** Build a `PrismFile` with a binary body. */
export function createBinaryFile(params: {
  path: string;
  ref: BinaryRef;
  languageId?: string;
  surfaceId?: string;
  metadata?: Record<string, unknown>;
}): PrismFile {
  const file: PrismFile = {
    path: params.path,
    body: { kind: "binary", ref: params.ref },
  };
  if (params.languageId !== undefined) file.languageId = params.languageId;
  if (params.surfaceId !== undefined) file.surfaceId = params.surfaceId;
  if (params.metadata !== undefined) file.metadata = params.metadata;
  return file;
}
