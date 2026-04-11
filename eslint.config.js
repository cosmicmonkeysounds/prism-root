import eslint from "@eslint/js";
import tseslint from "typescript-eslint";

export default tseslint.config(
  eslint.configs.recommended,
  ...tseslint.configs.strict,
  {
    rules: {
      "@typescript-eslint/no-explicit-any": "error",
      "@typescript-eslint/no-unused-vars": [
        "error",
        { argsIgnorePattern: "^_" },
      ],
    },
  },
  {
    ignores: [
      "**/dist/",
      "node_modules/",
      ".turbo/",
      "coverage/",
      "packages/prism-daemon/",
      "packages/prism-studio/src-tauri/",
      // wasm-pack generated glue + type declarations for the full-moon Luau
      // parser — not hand-authored, and references browser globals like
      // `WebAssembly`/`TextDecoder` that aren't in the base lint env.
      "packages/prism-core/src/language/luau/pkg/",
      "$legacy-inspiration-only/",
      "tmp-*",
    ],
  },
);
