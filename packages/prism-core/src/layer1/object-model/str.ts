/**
 * String-case helpers for codegen and route generation.
 * Exported from object-model so server and other packages can use without a new dep.
 */

/** Convert kebab-case or snake_case to PascalCase. */
export function pascal(s: string): string {
  return s.replace(/(^|[-_\s])([a-zA-Z])/g, (_, __, c: string) => c.toUpperCase());
}

/** Convert kebab-case or snake_case to camelCase. */
export function camel(s: string): string {
  const p = pascal(s);
  return p.charAt(0).toLowerCase() + p.slice(1);
}

/** Very light singularization for operationId generation. */
export function singular(s: string): string {
  if (s.endsWith("ies")) return s.slice(0, -3) + "y";
  if (s.endsWith("ses")) return s.slice(0, -2);
  if (s.endsWith("s") && !s.endsWith("ss")) return s.slice(0, -1);
  return s;
}
