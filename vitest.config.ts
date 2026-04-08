import { defineConfig } from "vitest/config";
import { resolve } from "path";

const core = (sub: string) =>
  resolve(__dirname, `packages/prism-core/src/${sub}`);

export default defineConfig({
  resolve: {
    alias: {
      "@prism/shared": resolve(__dirname, "packages/shared/src"),
      // Map @prism/core subpath exports to their actual locations.
      // Order matters — more-specific first so they aren't shadowed.
      "@prism/core/object-model": core("layer1/object-model/index.ts"),
      "@prism/core/lens": core("layer1/lens/index.ts"),
      "@prism/core/persistence": core("layer1/persistence/index.ts"),
      "@prism/core/atom": core("layer1/atom/index.ts"),
      "@prism/core/undo": core("layer1/undo/index.ts"),
      "@prism/core/notification": core("layer1/notification/index.ts"),
      "@prism/core/stores": core("layer1/stores/index.ts"),
      "@prism/core/codemirror": core("layer2/codemirror/index.ts"),
      "@prism/core/puck": core("layer2/puck/index.ts"),
      "@prism/core/kbar": core("layer2/kbar/index.ts"),
      "@prism/core/graph": core("layer2/graph/index.ts"),
      "@prism/core/shell": core("layer2/shell/index.ts"),
      "@prism/core/input": core("layer1/input/index.ts"),
      "@prism/core/forms": core("layer1/forms/index.ts"),
      "@prism/core/layout": core("layer1/layout/index.ts"),
      "@prism/core/expression": core("layer1/expression/index.ts"),
      "@prism/core/plugin": core("layer1/plugin/index.ts"),
      "@prism/core/automaton": core("layer1/automaton/index.ts"),
      "@prism/core/graph-analysis": core("layer1/graph-analysis/index.ts"),
      "@prism/core/automation": core("layer1/automation/index.ts"),
      "@prism/core/manifest": core("layer1/manifest/index.ts"),
      "@prism/core/config": core("layer1/config/index.ts"),
      "@prism/core/server": core("layer1/server/index.ts"),
      "@prism/core/search": core("layer1/search/index.ts"),
      "@prism/core/discovery": core("layer1/discovery/index.ts"),
      "@prism/core/view": core("layer1/view/index.ts"),
      "@prism/core/activity": core("layer1/activity/index.ts"),
      "@prism/core/batch": core("layer1/batch/index.ts"),
      "@prism/core/clipboard": core("layer1/clipboard/index.ts"),
      "@prism/core/template": core("layer1/template/index.ts"),
      "@prism/core/presence": core("layer1/presence/index.ts"),
      "@prism/core/identity": core("layer1/identity/index.ts"),
      "@prism/core/encryption": core("layer1/encryption/index.ts"),
      "@prism/core/vfs": core("layer1/vfs/index.ts"),
      "@prism/core/relay": core("layer1/relay/index.ts"),
      "@prism/core/actor": core("layer1/actor/index.ts"),
      "@prism/core/syntax": core("layer1/syntax/index.ts"),
      "@prism/core/session": core("layer1/session/index.ts"),
      "@prism/core/trust": core("layer1/trust/index.ts"),
      "@prism/core/timeline": core("layer1/timeline/index.ts"),
      "@prism/core/audio": core("layer2/audio/index.ts"),
      "@prism/core/viewport3d": core("layer2/viewport3d/index.ts"),
      "@prism/core/facet": core("layer1/facet/index.ts"),
      "@prism/core/flux": core("layer1/flux/index.ts"),
      "@prism/core/builder": core("layer1/builder/index.ts"),
      "@prism/core/machines": core("layer1/machines/index.ts"),
      "@prism/core/lua": core("layer1/lua/index.ts"),
      // Catch-all for deep imports like @prism/core/layer2/codemirror/editor-setup
      "@prism/core": resolve(__dirname, "packages/prism-core/src"),
      // @prism/relay subpath exports
      "@prism/relay/server": resolve(__dirname, "packages/prism-relay/src/server/index.ts"),
      "@prism/relay/protocol": resolve(__dirname, "packages/prism-relay/src/protocol/index.ts"),
      "@prism/relay/config": resolve(__dirname, "packages/prism-relay/src/config/index.ts"),
    },
  },
  test: {
    globals: true,
    environment: "node",
    include: ["packages/*/src/**/*.test.ts"],
    coverage: {
      provider: "v8",
      include: ["packages/*/src/**/*.ts"],
      exclude: ["**/*.test.ts", "**/*.d.ts", "**/index.ts"],
    },
  },
});
