# expression/

Expression Engine — scanner, parser, and evaluator for the formula language used by computed fields, conditional formats, and sequencer conditions. Also hosts the field resolvers that walk `GraphObject` edges to evaluate formula/lookup/rollup/computed fields against live data.

```ts
import { evaluateExpression, parse, tokenize } from '@prism/core/expression';
```

## Key exports

- `tokenize` / `parse` / `evaluate` / `evaluateExpression` — the full pipeline. `evaluateExpression(formula, context)` is the one-shot convenience that returns `{ result, errors }`.
- `ExprType` / `ExprValue` / `ExprError` / `ValueStore` — core types used by the pipeline.
- `AnyExprNode`, `LiteralNode`, `OperandNode`, `BinaryNode`, `UnaryNode`, `CallNode`, `BinaryOp`, `ParseResult` — AST node types produced by `parse`.
- `Token` / `TokenKind` / `OperandData` — scanner token types.
- `isDigit` / `isIdentStart` / `isIdentChar` — character-class predicates reused by other scanners.
- `readObjectField`, `buildFormulaContext`, `resolveFormulaField`, `resolveLookupField`, `resolveRollupField`, `resolveComputedField`, `aggregate` — field resolvers operating on `EdgeLookup` / `ObjectLookup` / `FieldResolverStores` abstractions, so the engine stays decoupled from any specific collection layer.

## Usage

```ts
import { evaluateExpression } from '@prism/core/expression';

const { result, errors } = evaluateExpression(
  'round(price * (1 + taxRate), 2)',
  { price: 19.995, taxRate: 0.08 },
);
// result === 21.59
```
