export interface EmittedFile {
  filename: string;
  content: string;
  language: 'typescript' | 'javascript' | 'csharp' | 'rust' | 'json' | string;
}

export interface CodegenMeta {
  projectName: string;
  version?: string;
  [key: string]: unknown;
}

export interface CodegenResult {
  files: EmittedFile[];
  errors: string[];
}

export interface Emitter<TInput = unknown> {
  id: string;
  emit(input: TInput, meta: CodegenMeta): CodegenResult;
}
