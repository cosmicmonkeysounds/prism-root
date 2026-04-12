import type {
  Emitter,
  CodegenMeta,
  CodegenResult,
  EmittedFile,
  CodegenInputs,
} from './codegen-types.js';

/**
 * CodegenPipeline — accepts heterogeneous emitters and dispatches each
 * one to the matching slot on the input bundle.
 *
 * The pipeline used to be monomorphic (a single `input: unknown` passed
 * to every emitter). ADR-002 §A3 replaces that with an `inputKind`
 * discriminator on `Emitter` so schema writers, symbol emitters, data
 * serializers, and plugin-custom emitters can share one registry
 * without the caller having to fan out by hand.
 *
 * Input kinds are open strings (see `EmitterInputKind`). Slots on
 * `CodegenInputs` are looked up by the same key, so adding a new kind
 * means nothing more than populating the matching slot at the call
 * site.
 */
export class CodegenPipeline {
  private _emitters: Emitter[] = [];

  register<T>(emitter: Emitter<T>): this {
    this._emitters.push(emitter as Emitter);
    return this;
  }

  /**
   * Run every registered emitter against its matching slot on `inputs`.
   * Emitters whose slot is undefined are skipped silently — callers can
   * register a single emitter set and opt in per run by only populating
   * the slots they want.
   */
  run(inputs: CodegenInputs, meta: CodegenMeta): CodegenResult {
    const allFiles: EmittedFile[] = [];
    const allErrors: string[] = [];
    for (const emitter of this._emitters) {
      const slot = inputs[emitter.inputKind];
      if (slot === undefined) continue;
      try {
        const result = emitter.emit(slot, meta);
        allFiles.push(...result.files);
        allErrors.push(...result.errors.map((e) => `[${emitter.id}] ${e}`));
      } catch (err) {
        allErrors.push(
          `[${emitter.id}] threw: ${err instanceof Error ? err.message : String(err)}`,
        );
      }
    }
    return { files: allFiles, errors: allErrors };
  }

  /** All registered emitters (read-only view). */
  emitters(): readonly Emitter[] {
    return this._emitters;
  }
}
