import type { Emitter, CodegenMeta, CodegenResult, EmittedFile } from './codegen-types.js';

export class CodegenPipeline {
  private _emitters: Emitter[] = [];

  register(emitter: Emitter): this {
    this._emitters.push(emitter);
    return this;
  }

  run(input: unknown, meta: CodegenMeta): CodegenResult {
    const allFiles: EmittedFile[] = [];
    const allErrors: string[] = [];
    for (const emitter of this._emitters) {
      try {
        const result = emitter.emit(input, meta);
        allFiles.push(...result.files);
        allErrors.push(...result.errors.map(e => `[${emitter.id}] ${e}`));
      } catch (err) {
        allErrors.push(`[${emitter.id}] threw: ${err instanceof Error ? err.message : String(err)}`);
      }
    }
    return { files: allFiles, errors: allErrors };
  }
}
