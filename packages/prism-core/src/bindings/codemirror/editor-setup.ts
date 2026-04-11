/**
 * Standard CodeMirror 6 editor setup for Prism.
 * Provides a baseline configuration that all Prism editors share.
 * Individual contexts can add language-specific extensions on top.
 */

import { keymap, highlightSpecialChars, drawSelection } from "@codemirror/view";
import { EditorState, type Extension } from "@codemirror/state";
import { defaultKeymap, history, historyKeymap } from "@codemirror/commands";
import {
  syntaxHighlighting,
  defaultHighlightStyle,
  bracketMatching,
  foldGutter,
  foldKeymap,
} from "@codemirror/language";
import {
  autocompletion,
  completionKeymap,
  closeBrackets,
  closeBracketsKeymap,
} from "@codemirror/autocomplete";
import { lintKeymap } from "@codemirror/lint";
import { searchKeymap, highlightSelectionMatches } from "@codemirror/search";
import { javascript } from "@codemirror/lang-javascript";
import { json } from "@codemirror/lang-json";

/**
 * Minimal shared extensions for all Prism CodeMirror instances.
 * Does NOT include Loro sync — that's added separately via loroSync().
 */
export function prismEditorSetup(): Extension {
  return [
    highlightSpecialChars(),
    history(),
    drawSelection(),
    EditorState.allowMultipleSelections.of(true),
    syntaxHighlighting(defaultHighlightStyle, { fallback: true }),
    bracketMatching(),
    closeBrackets(),
    autocompletion(),
    highlightSelectionMatches(),
    foldGutter(),
    keymap.of([
      ...closeBracketsKeymap,
      ...defaultKeymap,
      ...searchKeymap,
      ...historyKeymap,
      ...foldKeymap,
      ...completionKeymap,
      ...lintKeymap,
    ]),
  ];
}

/** Language extension for JavaScript/TypeScript files. */
export function prismJSLang(): Extension {
  return javascript({ typescript: true, jsx: true });
}

/** Language extension for JSON files. */
export function prismJSONLang(): Extension {
  return json();
}
