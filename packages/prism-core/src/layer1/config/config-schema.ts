/**
 * @prism/core — Config Schema Validation
 *
 * Lightweight, dependency-free JSON Schema subset for config value constraints.
 * Recursive for array.items and object.properties.
 *
 * Integration: use schemaToValidator() to convert a ConfigSchema into a
 * SettingDefinition.validate function for the ConfigRegistry.
 */

// ── ConfigSchema ────────────────────────────────────────────────────────────────

export type ConfigSchema =
  | StringSchema
  | NumberSchema
  | BooleanSchema
  | ArraySchema
  | ObjectSchema;

export interface StringSchema {
  type: "string";
  minLength?: number;
  maxLength?: number;
  /** RegExp source string. */
  pattern?: string;
  /** Exhaustive list of allowed values. */
  enum?: string[];
}

export interface NumberSchema {
  type: "number";
  min?: number;
  max?: number;
  /** When true, the value must be a safe integer. */
  integer?: boolean;
}

export interface BooleanSchema {
  type: "boolean";
}

export interface ArraySchema {
  type: "array";
  /** Schema applied to every element. */
  items?: ConfigSchema;
}

export interface ObjectSchema {
  type: "object";
  /** Schemas for named properties. */
  properties?: Record<string, ConfigSchema>;
  /** Property names that must be present. */
  required?: string[];
}

// ── ValidationResult ────────────────────────────────────────────────────────────

export interface ValidationError {
  /** Dot-notation path to the offending value. */
  path: string;
  message: string;
}

export interface ValidationResult {
  valid: boolean;
  errors: ValidationError[];
}

// ── validateConfig ──────────────────────────────────────────────────────────────

export function validateConfig(
  value: unknown,
  schema: ConfigSchema,
  path = "",
): ValidationResult {
  const errors: ValidationError[] = [];

  switch (schema.type) {
    case "string":
      validateString(value, schema, path, errors);
      break;
    case "number":
      validateNumber(value, schema, path, errors);
      break;
    case "boolean":
      validateBoolean(value, path, errors);
      break;
    case "array":
      validateArray(value, schema, path, errors);
      break;
    case "object":
      validateObject(value, schema, path, errors);
      break;
  }

  return { valid: errors.length === 0, errors };
}

// ── coerceConfigValue ───────────────────────────────────────────────────────────

/**
 * Coerce a raw string (e.g. from an environment variable) into the type
 * described by the schema.
 */
export function coerceConfigValue(
  value: string,
  schema: ConfigSchema,
): unknown {
  switch (schema.type) {
    case "string":
      return value;

    case "number": {
      const n = Number(value);
      if (Number.isNaN(n)) {
        throw new Error(`Cannot coerce '${value}' to number`);
      }
      return n;
    }

    case "boolean":
      return value === "true" || value === "1";

    case "array":
    case "object": {
      try {
        return JSON.parse(value);
      } catch {
        throw new Error(
          `Cannot coerce '${value}' to ${schema.type}: invalid JSON`,
        );
      }
    }
  }
}

// ── schemaToValidator ───────────────────────────────────────────────────────────

/**
 * Convert a ConfigSchema into a SettingDefinition.validate function.
 */
export function schemaToValidator<T = unknown>(
  schema: ConfigSchema,
): (value: T) => string | null {
  return (value: T): string | null => {
    const result = validateConfig(value, schema);
    if (result.valid) return null;
    return result.errors
      .map((e) => (e.path ? `[${e.path}] ${e.message}` : e.message))
      .join("; ");
  };
}

// ── Internal validators ─────────────────────────────────────────────────────────

function validateString(
  value: unknown,
  schema: StringSchema,
  path: string,
  errors: ValidationError[],
): void {
  if (typeof value !== "string") {
    errors.push({ path, message: `Expected string, got ${typeName(value)}` });
    return;
  }

  if (schema.minLength !== undefined && value.length < schema.minLength) {
    errors.push({
      path,
      message: `String too short (min ${schema.minLength}, got ${value.length})`,
    });
  }
  if (schema.maxLength !== undefined && value.length > schema.maxLength) {
    errors.push({
      path,
      message: `String too long (max ${schema.maxLength}, got ${value.length})`,
    });
  }
  if (schema.pattern !== undefined) {
    let re: RegExp;
    try {
      re = new RegExp(schema.pattern);
    } catch {
      errors.push({ path, message: `Invalid pattern: ${schema.pattern}` });
      return;
    }
    if (!re.test(value)) {
      errors.push({
        path,
        message: `Does not match pattern /${schema.pattern}/`,
      });
    }
  }
  if (schema.enum !== undefined && !schema.enum.includes(value)) {
    errors.push({
      path,
      message: `Must be one of: ${schema.enum.map((v) => JSON.stringify(v)).join(", ")}`,
    });
  }
}

function validateNumber(
  value: unknown,
  schema: NumberSchema,
  path: string,
  errors: ValidationError[],
): void {
  if (typeof value !== "number" || Number.isNaN(value)) {
    errors.push({ path, message: `Expected number, got ${typeName(value)}` });
    return;
  }

  if (schema.integer && !Number.isInteger(value)) {
    errors.push({ path, message: `Expected integer, got ${value}` });
  }
  if (schema.min !== undefined && value < schema.min) {
    errors.push({
      path,
      message: `Too small (min ${schema.min}, got ${value})`,
    });
  }
  if (schema.max !== undefined && value > schema.max) {
    errors.push({
      path,
      message: `Too large (max ${schema.max}, got ${value})`,
    });
  }
}

function validateBoolean(
  value: unknown,
  path: string,
  errors: ValidationError[],
): void {
  if (typeof value !== "boolean") {
    errors.push({ path, message: `Expected boolean, got ${typeName(value)}` });
  }
}

function validateArray(
  value: unknown,
  schema: ArraySchema,
  path: string,
  errors: ValidationError[],
): void {
  if (!Array.isArray(value)) {
    errors.push({ path, message: `Expected array, got ${typeName(value)}` });
    return;
  }

  if (schema.items) {
    for (let i = 0; i < value.length; i++) {
      const childPath = path ? `${path}[${i}]` : `[${i}]`;
      const child = validateConfig(value[i], schema.items, childPath);
      for (const e of child.errors) errors.push(e);
    }
  }
}

function validateObject(
  value: unknown,
  schema: ObjectSchema,
  path: string,
  errors: ValidationError[],
): void {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    errors.push({ path, message: `Expected object, got ${typeName(value)}` });
    return;
  }

  const obj = value as Record<string, unknown>;

  if (schema.required) {
    for (const key of schema.required) {
      if (!(key in obj)) {
        const childPath = path ? `${path}.${key}` : key;
        errors.push({ path: childPath, message: "Required property missing" });
      }
    }
  }

  if (schema.properties) {
    for (const [key, propSchema] of Object.entries(schema.properties)) {
      if (!(key in obj)) continue;
      const childPath = path ? `${path}.${key}` : key;
      const child = validateConfig(obj[key], propSchema, childPath);
      for (const e of child.errors) errors.push(e);
    }
  }
}

function typeName(value: unknown): string {
  if (value === null) return "null";
  if (Array.isArray(value)) return "array";
  return typeof value;
}
