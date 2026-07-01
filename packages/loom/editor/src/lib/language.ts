import type { Extension } from '@codemirror/state'
import { javascript } from '@codemirror/lang-javascript'
import { json } from '@codemirror/lang-json'
import { markdown } from '@codemirror/lang-markdown'
import { html } from '@codemirror/lang-html'
import { css } from '@codemirror/lang-css'
import { python } from '@codemirror/lang-python'
import { rust } from '@codemirror/lang-rust'
import { go } from '@codemirror/lang-go'
import { yaml } from '@codemirror/lang-yaml'
import { sql } from '@codemirror/lang-sql'
import { xml } from '@codemirror/lang-xml'
import { cpp } from '@codemirror/lang-cpp'
import { java } from '@codemirror/lang-java'
import { php } from '@codemirror/lang-php'
import { vue } from '@codemirror/lang-vue'
import { loomLanguage } from './loom-language'

export type LanguageId =
  | 'javascript'
  | 'typescript'
  | 'tsx'
  | 'jsx'
  | 'json'
  | 'markdown'
  | 'html'
  | 'css'
  | 'scss'
  | 'python'
  | 'rust'
  | 'go'
  | 'java'
  | 'kotlin'
  | 'cpp'
  | 'c'
  | 'csharp'
  | 'ruby'
  | 'php'
  | 'swift'
  | 'sql'
  | 'yaml'
  | 'toml'
  | 'xml'
  | 'shell'
  | 'dockerfile'
  | 'vue'
  | 'loom'
  | 'plaintext'

const EXT_TO_LANG: Record<string, LanguageId> = {
  js: 'javascript',
  mjs: 'javascript',
  cjs: 'javascript',
  jsx: 'jsx',
  ts: 'typescript',
  tsx: 'tsx',
  json: 'json',
  json5: 'json',
  jsonc: 'json',
  md: 'markdown',
  mdx: 'markdown',
  markdown: 'markdown',
  html: 'html',
  htm: 'html',
  xhtml: 'html',
  vue: 'vue',
  svelte: 'html',
  css: 'css',
  scss: 'scss',
  sass: 'scss',
  less: 'css',
  py: 'python',
  pyi: 'python',
  rs: 'rust',
  go: 'go',
  java: 'java',
  kt: 'kotlin',
  kts: 'kotlin',
  cpp: 'cpp',
  cc: 'cpp',
  cxx: 'cpp',
  hpp: 'cpp',
  hh: 'cpp',
  hxx: 'cpp',
  c: 'c',
  h: 'c',
  cs: 'csharp',
  rb: 'ruby',
  php: 'php',
  swift: 'swift',
  sql: 'sql',
  yaml: 'yaml',
  yml: 'yaml',
  toml: 'toml',
  xml: 'xml',
  svg: 'xml',
  sh: 'shell',
  bash: 'shell',
  zsh: 'shell',
  fish: 'shell',
  loom: 'loom',
}

const FILENAME_TO_LANG: Record<string, LanguageId> = {
  dockerfile: 'dockerfile',
  'docker-compose.yml': 'yaml',
  'docker-compose.yaml': 'yaml',
  makefile: 'shell',
  'cargo.toml': 'toml',
  '.gitignore': 'plaintext',
  '.env': 'shell',
}

export function languageForPath(path: string): LanguageId {
  const base = path.split('/').pop()?.toLowerCase() ?? ''
  if (FILENAME_TO_LANG[base]) return FILENAME_TO_LANG[base]
  for (const name of Object.keys(FILENAME_TO_LANG)) {
    if (base.startsWith(name)) return FILENAME_TO_LANG[name]
  }
  const ext = base.includes('.') ? (base.split('.').pop() ?? '') : ''
  return EXT_TO_LANG[ext] ?? 'plaintext'
}

const LANGUAGE_LABELS: Record<LanguageId, string> = {
  javascript: 'JavaScript',
  typescript: 'TypeScript',
  tsx: 'TSX',
  jsx: 'JSX',
  json: 'JSON',
  markdown: 'Markdown',
  html: 'HTML',
  css: 'CSS',
  scss: 'SCSS',
  python: 'Python',
  rust: 'Rust',
  go: 'Go',
  java: 'Java',
  kotlin: 'Kotlin',
  cpp: 'C++',
  c: 'C',
  csharp: 'C#',
  ruby: 'Ruby',
  php: 'PHP',
  swift: 'Swift',
  sql: 'SQL',
  yaml: 'YAML',
  toml: 'TOML',
  xml: 'XML',
  shell: 'Shell',
  dockerfile: 'Dockerfile',
  vue: 'Vue',
  loom: 'Loom',
  plaintext: 'Plain Text',
}

export function languageLabel(id: LanguageId): string {
  return LANGUAGE_LABELS[id]
}

export function extensionForPath(path: string): Extension[] {
  const lang = languageForPath(path)
  switch (lang) {
    case 'javascript':
      return [javascript()]
    case 'jsx':
      return [javascript({ jsx: true })]
    case 'typescript':
      return [javascript({ typescript: true })]
    case 'tsx':
      return [javascript({ jsx: true, typescript: true })]
    case 'json':
      return [json()]
    case 'markdown':
      return [markdown()]
    case 'html':
      return [html()]
    case 'css':
    case 'scss':
      return [css()]
    case 'python':
      return [python()]
    case 'rust':
      return [rust()]
    case 'go':
      return [go()]
    case 'yaml':
      return [yaml()]
    case 'sql':
      return [sql()]
    case 'xml':
      return [xml()]
    case 'cpp':
    case 'c':
      return [cpp()]
    case 'java':
    case 'kotlin':
      return [java()]
    case 'php':
      return [php()]
    case 'vue':
      return [vue()]
    case 'loom':
      // The Loom StreamLanguage only. Diagnostics + the LSP IDE extensions
      // (hover / completion / goto / references / occurrences) are composed
      // in `Editor.tsx`, where the editor settings + file path are available.
      return [loomLanguage()]
    default:
      return []
  }
}
