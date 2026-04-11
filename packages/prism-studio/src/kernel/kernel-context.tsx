/**
 * KernelProvider — React context exposing the Studio kernel to all components.
 *
 * Components use useKernel() to access the full kernel, or the focused hooks
 * (useSelection, useObjects, useNotifications) for specific slices.
 */

import { createContext, useContext, useSyncExternalStore, useCallback, useRef, useState } from "react";
import type { ReactNode } from "react";
import type { StudioKernel } from "./studio-kernel.js";
import type { GraphObject, ObjectId } from "@prism/core/object-model";
import type { Notification, NotificationKind } from "@prism/core/notification";
import type { RelayEntry } from "./relay-manager.js";
import type { AppProfile, BuildRun } from "./builder-manager.js";
import type { SettingDefinition } from "@prism/core/config";
import type { PresenceState } from "@prism/core/presence";
import type { Automation, AutomationRun } from "@prism/core/automation";
import type { PlanResult, SlipImpact } from "@prism/core/graph-analysis";
import type { ExprValue } from "@prism/core/expression";
import type { PrismPlugin } from "@prism/core/plugin";
import type { InputRouterEvent } from "@prism/core/input";
import type { RosterEntry } from "@prism/core/discovery";
import type { PrismIdentity, ExportedIdentity } from "@prism/core/identity";
import type { BinaryRef, BinaryLock } from "@prism/core/vfs";
import type { PeerReputation, SchemaValidationResult, ContentHash, SandboxPolicy, LuauSandbox, ShamirShare, ShamirConfig, EscrowDeposit } from "@prism/core/trust";
import type { SourceFormat, FacetDefinition, FacetLayout, ProseNode, SchemaModel, SequencerConditionState, SequencerScriptState, FacetStore, ValueListRegistry, ValueList } from "@prism/core/facet";
import type { FieldSchema } from "@prism/core/forms";
import type { SavedView, SavedViewRegistry } from "@prism/core/view";
import type { PrivilegeSet, PrivilegeEnforcer, RoleAssignment } from "@prism/core/manifest";

// ── Context ─────────────────────────────────────────────────────────────────

const KernelContext = createContext<StudioKernel | null>(null);

export function KernelProvider({
  kernel,
  children,
}: {
  kernel: StudioKernel;
  children: ReactNode;
}) {
  return (
    <KernelContext.Provider value={kernel}>{children}</KernelContext.Provider>
  );
}

/** Access the full Studio kernel. Throws if used outside KernelProvider. */
export function useKernel(): StudioKernel {
  const ctx = useContext(KernelContext);
  if (!ctx) throw new Error("useKernel must be used within KernelProvider");
  return ctx;
}

// ── Focused hooks ───────────────────────────────────────────────────────────

/** Reactive selected object ID from AtomStore. */
export function useSelection(): {
  selectedId: ObjectId | null;
  select: (id: ObjectId | null) => void;
} {
  const kernel = useKernel();
  const selectedId = useSyncExternalStore(
    (cb) => kernel.atoms.subscribe(cb),
    () => kernel.atoms.getState().selectedId,
  );
  return { selectedId, select: kernel.select };
}

/**
 * Reactive object list from ObjectAtomStore.
 * Uses a version counter to avoid returning new arrays from getSnapshot.
 */
export function useObjects(filter?: {
  types?: string[];
  parentId?: ObjectId | null;
}): GraphObject[] {
  const kernel = useKernel();
  const cacheRef = useRef<{ value: GraphObject[]; version: number }>({ value: [], version: -1 });

  const version = useSyncExternalStore(
    (cb) => kernel.objectAtoms.subscribe(cb),
    () => {
      // Return a simple counter that increments on change
      const keys = Object.keys(kernel.objectAtoms.getState().objects);
      return keys.length; // Stable primitive
    },
  );

  // Compute the filtered list only when version changes
  if (cacheRef.current.version !== version) {
    const all = Object.values(kernel.objectAtoms.getState().objects);
    cacheRef.current = {
      value: all.filter((obj) => {
        if (obj.deletedAt) return false;
        if (filter?.types && !filter.types.includes(obj.type)) return false;
        if (filter?.parentId !== undefined && obj.parentId !== filter.parentId)
          return false;
        return true;
      }),
      version,
    };
  }

  return cacheRef.current.value;
}

/** Reactive single object by ID. */
export function useObject(id: ObjectId | null): GraphObject | undefined {
  const kernel = useKernel();
  return useSyncExternalStore(
    (cb) => kernel.objectAtoms.subscribe(cb),
    () => (id ? kernel.objectAtoms.getState().objects[id] : undefined),
  );
}

/** Reactive undo/redo state. */
export function useUndo(): {
  canUndo: boolean;
  canRedo: boolean;
  undoLabel: string | null;
  redoLabel: string | null;
  undo: () => void;
  redo: () => void;
} {
  const kernel = useKernel();
  const cacheRef = useRef<{
    canUndo: boolean;
    canRedo: boolean;
    undoLabel: string | null;
    redoLabel: string | null;
  }>({ canUndo: false, canRedo: false, undoLabel: null, redoLabel: null });

  // Use a string key as the snapshot — stable primitive
  const stateKey = useSyncExternalStore(
    (cb) => kernel.undo.subscribe(cb),
    () => `${kernel.undo.canUndo}|${kernel.undo.canRedo}|${kernel.undo.undoLabel}|${kernel.undo.redoLabel}`,
  );

  // Update cache when key changes
  const parts = stateKey.split("|");
  cacheRef.current = {
    canUndo: parts[0] === "true",
    canRedo: parts[1] === "true",
    undoLabel: parts[2] === "null" ? null : (parts[2] ?? null),
    redoLabel: parts[3] === "null" ? null : (parts[3] ?? null),
  };

  return {
    ...cacheRef.current,
    undo: () => kernel.undo.undo(),
    redo: () => kernel.undo.redo(),
  };
}

/** Reactive notification list. */
export function useNotifications(kind?: NotificationKind): {
  items: Notification[];
  unreadCount: number;
  add: (title: string, kind: NotificationKind, body?: string) => void;
  dismiss: (id: string) => void;
} {
  const kernel = useKernel();
  const cacheRef = useRef<{ items: Notification[]; unreadCount: number; version: number }>({
    items: [],
    unreadCount: 0,
    version: -1,
  });

  const version = useSyncExternalStore(
    (cb) => kernel.notifications.subscribe(cb),
    () => kernel.notifications.getUnreadCount(),
  );

  if (cacheRef.current.version !== version) {
    cacheRef.current = {
      items: kernel.notifications.getAll(kind ? { kind: [kind] } : undefined),
      unreadCount: kernel.notifications.getUnreadCount(),
      version,
    };
  }

  const add = useCallback(
    (title: string, k: NotificationKind, body?: string) => {
      kernel.notifications.add({ title, kind: k, body });
    },
    [kernel],
  );

  const dismiss = useCallback(
    (id: string) => {
      kernel.notifications.dismiss(id);
    },
    [kernel],
  );

  return { ...cacheRef.current, add, dismiss };
}

/** Reactive relay connection list from RelayManager. */
export function useRelay(): {
  relays: RelayEntry[];
  manager: StudioKernel["relay"];
} {
  const kernel = useKernel();
  const cacheRef = useRef<{ relays: RelayEntry[]; version: string }>({
    relays: [],
    version: "",
  });

  const version = useSyncExternalStore(
    (cb) => kernel.relay.subscribe(cb),
    () => {
      // Use a simple length + status key as a stable primitive
      const entries = kernel.relay.listRelays();
      return entries.map((e) => `${e.id}:${e.status}`).join(",");
    },
  );

  if (cacheRef.current.version !== version) {
    cacheRef.current = {
      relays: kernel.relay.listRelays(),
      version,
    };
  }

  return { relays: cacheRef.current.relays, manager: kernel.relay };
}

/** Reactive builder state (profiles, active profile, build runs). */
export function useBuilder(): {
  profiles: AppProfile[];
  activeProfile: AppProfile | null;
  runs: BuildRun[];
  manager: StudioKernel["builder"];
} {
  const kernel = useKernel();
  const cacheRef = useRef<{
    profiles: AppProfile[];
    activeProfile: AppProfile | null;
    runs: BuildRun[];
    version: string;
  }>({ profiles: [], activeProfile: null, runs: [], version: "" });

  const version = useSyncExternalStore(
    (cb) => kernel.builder.subscribe(cb),
    () => {
      const profiles = kernel.builder.listProfiles();
      const active = kernel.builder.getActiveProfile();
      const runs = kernel.builder.listRuns();
      return `${profiles.length}:${active?.id ?? "_"}:${runs.length}:${runs[0]?.status ?? "_"}`;
    },
  );

  if (cacheRef.current.version !== version) {
    cacheRef.current = {
      profiles: kernel.builder.listProfiles(),
      activeProfile: kernel.builder.getActiveProfile(),
      runs: kernel.builder.listRuns(),
      version,
    };
  }

  return {
    profiles: cacheRef.current.profiles,
    activeProfile: cacheRef.current.activeProfile,
    runs: cacheRef.current.runs,
    manager: kernel.builder,
  };
}

/** Reactive config value by key. */
export function useConfig<T>(key: string): {
  value: T;
  set: (value: T) => void;
  definition: SettingDefinition | undefined;
} {
  const kernel = useKernel();

  const value = useSyncExternalStore(
    (cb) => kernel.config.on("change", cb),
    () => kernel.config.get<T>(key) as T,
  );

  const set = useCallback(
    (v: T) => kernel.config.set(key, v, "user"),
    [kernel, key],
  );

  const definition = kernel.configRegistry.get(key);

  return { value, set, definition };
}

/** Reactive config settings list grouped by tag. */
export function useConfigSettings(tag?: string): SettingDefinition[] {
  const kernel = useKernel();
  if (tag) return kernel.configRegistry.byTag(tag);
  return kernel.configRegistry.all();
}

/** Reactive presence peer list. */
export function usePresence(): {
  peers: PresenceState[];
  localPeer: PresenceState;
  peerCount: number;
} {
  const kernel = useKernel();
  const cacheRef = useRef<{ peers: PresenceState[]; version: number }>({
    peers: [],
    version: -1,
  });

  const version = useSyncExternalStore(
    (cb) => kernel.presence.subscribe(cb),
    () => kernel.presence.peerCount,
  );

  if (cacheRef.current.version !== version) {
    cacheRef.current = {
      peers: kernel.presence.getAll(),
      version,
    };
  }

  return {
    peers: cacheRef.current.peers,
    localPeer: kernel.presence.local,
    peerCount: version,
  };
}

/** Reactive automation list and runs. */
export function useAutomation(): {
  automations: Automation[];
  runs: AutomationRun[];
  save: (automation: Automation) => void;
  remove: (id: string) => void;
  run: (id: string) => Promise<AutomationRun>;
} {
  const kernel = useKernel();
  const cacheRef = useRef<{ automations: Automation[]; runs: AutomationRun[]; version: string }>({
    automations: [],
    runs: [],
    version: "",
  });

  const version = useSyncExternalStore(
    (cb) => kernel.onAutomationChange(cb),
    () => {
      const automations = kernel.listAutomations();
      const runs = kernel.listAutomationRuns();
      return automations.map((a) => `${a.id}:${a.enabled}:${a.runCount}`).join(",") + `|${runs.length}`;
    },
  );

  if (cacheRef.current.version !== version) {
    cacheRef.current = {
      automations: kernel.listAutomations(),
      runs: kernel.listAutomationRuns(),
      version,
    };
  }

  const save = useCallback(
    (automation: Automation) => kernel.saveAutomation(automation),
    [kernel],
  );

  const remove = useCallback(
    (id: string) => kernel.deleteAutomation(id),
    [kernel],
  );

  const run = useCallback(
    (id: string) => kernel.runAutomation(id),
    [kernel],
  );

  return { ...cacheRef.current, save, remove, run };
}

/** Graph analysis utilities. */
export function useGraphAnalysis(): {
  topologicalSort: () => string[];
  detectCycles: () => string[][];
  blockingChain: (id: string) => string[];
  impact: (id: string) => string[];
  slipImpact: (id: string, days: number) => SlipImpact[];
  plan: () => PlanResult;
} {
  const kernel = useKernel();

  return {
    topologicalSort: kernel.analyzeTopologicalSort,
    detectCycles: kernel.analyzeCycles,
    blockingChain: kernel.analyzeBlockingChain,
    impact: kernel.analyzeImpact,
    slipImpact: kernel.analyzeSlipImpact,
    plan: kernel.analyzePlan,
  };
}

/** Expression formula evaluator. */
export function useExpression(): {
  evaluate: (formula: string, objectId?: ObjectId) => { result: ExprValue; errors: string[] };
} {
  const kernel = useKernel();

  const evaluate = useCallback(
    (formula: string, objectId?: ObjectId) => kernel.evaluateFormula(formula, objectId),
    [kernel],
  );

  return { evaluate };
}

/** Reactive plugin registry. */
export function usePlugins(): {
  plugins: PrismPlugin[];
  register: (plugin: PrismPlugin) => () => void;
  unregister: (id: string) => boolean;
} {
  const kernel = useKernel();
  const cacheRef = useRef<{ plugins: PrismPlugin[]; version: number }>({
    plugins: [],
    version: -1,
  });

  const version = useSyncExternalStore(
    (cb) => kernel.onPluginChange(cb),
    () => kernel.plugins.size,
  );

  if (cacheRef.current.version !== version) {
    cacheRef.current = {
      plugins: kernel.listPlugins(),
      version,
    };
  }

  return {
    plugins: cacheRef.current.plugins,
    register: kernel.registerPlugin,
    unregister: kernel.unregisterPlugin,
  };
}

/** Reactive keyboard bindings from InputRouter. */
export function useInputRouter(): {
  bindings: Array<{ shortcut: string; action: string }>;
  bind: (shortcut: string, action: string) => void;
  unbind: (shortcut: string) => void;
  recentEvents: InputRouterEvent[];
} {
  const kernel = useKernel();
  const [events, setEvents] = useState<InputRouterEvent[]>([]);
  const [bindingVersion, setBindingVersion] = useState(0);

  // Subscribe to router events
  const eventsRef = useRef(events);
  eventsRef.current = events;

  useSyncExternalStore(
    (cb) => {
      return kernel.onInputEvent((event) => {
        const next = [...eventsRef.current.slice(-49), event];
        setEvents(next);
        cb();
      });
    },
    () => events.length,
  );

  const bindings = kernel.listBindings();

  const bind = useCallback(
    (shortcut: string, action: string) => {
      kernel.bindShortcut(shortcut, action);
      setBindingVersion((v) => v + 1);
    },
    [kernel],
  );

  const unbind = useCallback(
    (shortcut: string) => {
      kernel.unbindShortcut(shortcut);
      setBindingVersion((v) => v + 1);
    },
    [kernel],
  );

  // Force re-read on binding version change
  void bindingVersion;

  return {
    bindings,
    bind,
    unbind,
    recentEvents: events,
  };
}

/** Reactive vault roster. */
export function useVaultRoster(): {
  vaults: RosterEntry[];
  addVault: (entry: Omit<RosterEntry, "addedAt"> & { addedAt?: string }) => RosterEntry;
  removeVault: (id: string) => boolean;
  pinVault: (id: string, pinned: boolean) => RosterEntry | undefined;
  touchVault: (id: string) => RosterEntry | undefined;
} {
  const kernel = useKernel();
  const versionRef = useRef(0);

  const version = useSyncExternalStore(
    (cb) => kernel.onVaultChange(() => {
      versionRef.current++;
      cb();
    }),
    () => versionRef.current,
  );

  const vaults = kernel.listVaults();

  // Force re-read when version changes
  void version;

  return {
    vaults,
    addVault: kernel.addVault,
    removeVault: kernel.removeVault,
    pinVault: kernel.pinVault,
    touchVault: kernel.touchVault,
  };
}

/** Reactive identity state. */
export function useIdentity(): {
  identity: PrismIdentity | null;
  generate: () => Promise<PrismIdentity>;
  exportId: () => Promise<ExportedIdentity | null>;
  importId: (exported: ExportedIdentity) => Promise<PrismIdentity>;
  sign: (data: Uint8Array) => Promise<Uint8Array | null>;
  verify: (data: Uint8Array, signature: Uint8Array) => Promise<boolean>;
} {
  const kernel = useKernel();
  const versionRef = useRef(0);

  const version = useSyncExternalStore(
    (cb) => kernel.onIdentityChange(() => {
      versionRef.current++;
      cb();
    }),
    () => versionRef.current,
  );

  // Force re-read on version change
  void version;

  return {
    identity: kernel.identity,
    generate: kernel.generateIdentity,
    exportId: kernel.exportIdentity,
    importId: kernel.importIdentity,
    sign: kernel.signData,
    verify: kernel.verifyData,
  };
}

/** Reactive virtual file system state. */
export function useVfs(): {
  files: BinaryRef[];
  locks: BinaryLock[];
  importFile: (data: Uint8Array, filename: string, mimeType: string) => Promise<BinaryRef>;
  exportFile: (ref: BinaryRef) => Promise<Uint8Array | null>;
  removeFile: (hash: string) => Promise<boolean>;
  acquireLock: (hash: string, reason?: string) => BinaryLock;
  releaseLock: (hash: string) => void;
} {
  const kernel = useKernel();
  const versionRef = useRef(0);

  const version = useSyncExternalStore(
    (cb) => kernel.onVfsChange(() => {
      versionRef.current++;
      cb();
    }),
    () => versionRef.current,
  );

  // Force re-read on version change
  void version;

  return {
    files: kernel.listFiles(),
    locks: kernel.listLocks(),
    importFile: kernel.importFile,
    exportFile: kernel.exportFile,
    removeFile: kernel.removeFile,
    acquireLock: kernel.acquireLock,
    releaseLock: kernel.releaseLock,
  };
}

/** Reactive trust & safety state. */
export function useTrust(): {
  peers: PeerReputation[];
  flaggedContent: ReadonlyArray<ContentHash>;
  trustPeer: (peerId: string) => void;
  distrustPeer: (peerId: string) => void;
  banPeer: (peerId: string, reason: string) => void;
  unbanPeer: (peerId: string) => void;
  validateImport: (data: unknown) => SchemaValidationResult;
  createSandbox: (policy: SandboxPolicy) => LuauSandbox;
  flagContent: (hash: string, category: string) => void;
  splitSecret: (secret: Uint8Array, config: ShamirConfig) => ShamirShare[];
  combineShares: (shares: ShamirShare[], config: ShamirConfig) => Uint8Array;
  depositEscrow: (encryptedPayload: string, expiresAt?: string) => EscrowDeposit | null;
  listEscrowDeposits: () => EscrowDeposit[];
} {
  const kernel = useKernel();
  const versionRef = useRef(0);

  const version = useSyncExternalStore(
    (cb) => kernel.onTrustChange(() => {
      versionRef.current++;
      cb();
    }),
    () => versionRef.current,
  );

  // Force re-read on version change
  void version;

  return {
    peers: kernel.listPeers(),
    flaggedContent: kernel.listFlaggedContent(),
    trustPeer: kernel.trustPeer,
    distrustPeer: kernel.distrustPeer,
    banPeer: kernel.banPeer,
    unbanPeer: kernel.unbanPeer,
    validateImport: kernel.validateImport,
    createSandbox: kernel.createSandbox,
    flagContent: kernel.flagContent,
    splitSecret: kernel.splitSecret,
    combineShares: kernel.combineShares,
    depositEscrow: kernel.depositEscrow,
    listEscrowDeposits: kernel.listEscrowDeposits,
  };
}

/** Facet parser utilities. */
export function useFacetParser(): {
  detectFormat: (source: string) => SourceFormat;
  parseValues: (source: string, format: SourceFormat) => Record<string, unknown>;
  serializeValues: (values: Record<string, unknown>, format: SourceFormat, originalSource?: string) => string;
  inferFields: (values: Record<string, unknown>) => FieldSchema[];
} {
  const kernel = useKernel();

  return {
    detectFormat: kernel.detectFormat,
    parseValues: kernel.parseValues,
    serializeValues: kernel.serializeValues,
    inferFields: kernel.inferFields,
  };
}

/** Spell checking utilities. */
export function useSpellCheck(): {
  check: (text: string) => Array<{ word: string; from: number; to: number; suggestions: string[] }>;
  suggest: (word: string) => string[];
} {
  const kernel = useKernel();

  return {
    check: kernel.spellCheck,
    suggest: kernel.spellSuggest,
  };
}

/** Prose codec (Markdown ↔ ProseNode). */
export function useProseCodec(): {
  markdownToNodes: (md: string) => ProseNode;
  nodesToMarkdown: (node: ProseNode) => string;
} {
  const kernel = useKernel();

  return {
    markdownToNodes: kernel.markdownToNodes,
    nodesToMarkdown: kernel.nodesToMarkdown,
  };
}

/** Sequencer (visual condition/script → Luau). */
export function useSequencer(): {
  emitConditionLuau: (state: SequencerConditionState) => string;
  emitScriptLuau: (state: SequencerScriptState) => string;
} {
  const kernel = useKernel();

  return {
    emitConditionLuau: kernel.emitConditionLuau,
    emitScriptLuau: kernel.emitScriptLuau,
  };
}

/** Code emitters (SchemaModel → multi-language). */
export function useEmitters(): {
  emit: (model: SchemaModel, language: "typescript" | "javascript" | "csharp" | "luau" | "json" | "yaml" | "toml") => string;
} {
  const kernel = useKernel();

  return {
    emit: kernel.emitCode,
  };
}

/** Reactive facet definition registry. */
export function useFacetDefinitions(): {
  definitions: FacetDefinition[];
  register: (definition: FacetDefinition) => void;
  remove: (id: string) => boolean;
  get: (id: string) => FacetDefinition | undefined;
  buildDefinition: (id: string, entityType: string, layout: FacetLayout) => ReturnType<StudioKernel["buildFacetDefinition"]>;
} {
  const kernel = useKernel();
  const versionRef = useRef(0);

  const version = useSyncExternalStore(
    (cb) => kernel.onFacetChange(() => {
      versionRef.current++;
      cb();
    }),
    () => versionRef.current,
  );

  // Force re-read on version change
  void version;

  return {
    definitions: kernel.listFacetDefinitions(),
    register: kernel.registerFacetDefinition,
    remove: kernel.removeFacetDefinition,
    get: kernel.getFacetDefinition,
    buildDefinition: kernel.buildFacetDefinition,
  };
}

/** Reactive FacetStore access (persistent facets/scripts/value-lists). */
export function useFacetStore(): FacetStore {
  const kernel = useKernel();
  return kernel.facetStore;
}

/** Reactive saved views state. */
export function useSavedViews(): {
  views: SavedView[];
  registry: SavedViewRegistry;
} {
  const kernel = useKernel();
  const versionRef = useRef(0);

  const version = useSyncExternalStore(
    (cb) => kernel.onSavedViewChange(() => {
      versionRef.current++;
      cb();
    }),
    () => versionRef.current,
  );

  void version;

  return {
    views: kernel.savedViews.all(),
    registry: kernel.savedViews,
  };
}

/** Reactive value lists state. */
export function useValueLists(): {
  lists: ValueList[];
  registry: ValueListRegistry;
} {
  const kernel = useKernel();
  const versionRef = useRef(0);

  const version = useSyncExternalStore(
    (cb) => kernel.onValueListChange(() => {
      versionRef.current++;
      cb();
    }),
    () => versionRef.current,
  );

  void version;

  return {
    lists: kernel.valueLists.all(),
    registry: kernel.valueLists,
  };
}

/** Reactive privilege set state. */
export function usePrivilegeSets(): {
  sets: PrivilegeSet[];
  roles: RoleAssignment[];
  save: (ps: PrivilegeSet) => void;
  remove: (id: string) => boolean;
  getEnforcer: (id: string) => PrivilegeEnforcer | undefined;
  assignRole: (did: string, privilegeSetId: string) => void;
  removeRole: (did: string) => void;
} {
  const kernel = useKernel();
  const versionRef = useRef(0);

  const version = useSyncExternalStore(
    (cb) => kernel.onPrivilegeSetChange(() => {
      versionRef.current++;
      cb();
    }),
    () => versionRef.current,
  );

  void version;

  return {
    sets: kernel.listPrivilegeSets(),
    roles: kernel.listRoleAssignments(),
    save: kernel.savePrivilegeSet,
    remove: kernel.removePrivilegeSet,
    getEnforcer: kernel.getEnforcer,
    assignRole: kernel.assignRole,
    removeRole: kernel.removeRoleAssignment,
  };
}
