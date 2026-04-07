/**
 * Studio Kernel — central nervous system wiring all Layer 1 systems.
 *
 * Creates and connects:
 *   - ObjectRegistry (entity/edge type definitions)
 *   - CollectionStore (Loro CRDT-backed object/edge storage)
 *   - PrismBus (typed event bus for cross-panel communication)
 *   - AtomStore (UI state atoms: selection, active panel, search)
 *   - ObjectAtomStore (in-memory object/edge cache with selectors)
 *   - UndoRedoManager (snapshot-based undo/redo)
 *   - NotificationStore (toast/alert queue)
 *   - SearchEngine (full-text TF-IDF search)
 *   - ActivityStore + ActivityTracker (audit trail)
 *   - LiveView (materialized filtered/sorted/grouped projection)
 *
 * The kernel is instantiated once at app startup and provided via React
 * context to all components.
 */

import { ObjectRegistry } from "@prism/core/object-model";
import type { GraphObject, ObjectEdge, ObjectId, EdgeId } from "@prism/core/object-model";
import { objectId, edgeId } from "@prism/core/object-model";
import { createCollectionStore } from "@prism/core/persistence";
import type { CollectionStore } from "@prism/core/persistence";
import {
  createPrismBus,
  PrismEvents,
  createAtomStore,
  createObjectAtomStore,
  connectBusToAtoms,
  connectBusToObjectAtoms,
} from "@prism/core/atom";
import type { PrismBus, AtomStore, ObjectAtomStore } from "@prism/core/atom";
import { UndoRedoManager } from "@prism/core/undo";
import type { ObjectSnapshot } from "@prism/core/undo";
import { createNotificationStore } from "@prism/core/notification";
import type { NotificationStore } from "@prism/core/notification";
import { createSearchEngine } from "@prism/core/search";
import type { SearchEngine } from "@prism/core/search";
import { createActivityStore } from "@prism/core/activity";
import type { ActivityStore } from "@prism/core/activity";
import { createActivityTracker } from "@prism/core/activity";
import type { ActivityTracker } from "@prism/core/activity";
import { createLiveView } from "@prism/core/view";
import type { LiveView, LiveViewOptions } from "@prism/core/view";
import type { ObjectTemplate, TemplateNode, InstantiateResult } from "@prism/core/template";
import { createPageBuilderRegistry } from "./entities.js";
import { createRelayManager } from "./relay-manager.js";
import type { RelayManager } from "./relay-manager.js";
import { ConfigRegistry, ConfigModel } from "@prism/core/config";
import { createPresenceManager } from "@prism/core/presence";
import type { PresenceManager } from "@prism/core/presence";
import { createViewRegistry } from "@prism/core/view";
import type { ViewRegistry, ViewMode } from "@prism/core/view";
import { AutomationEngine } from "@prism/core/automation";
import type {
  AutomationStore,
} from "@prism/core/automation";
import type {
  Automation,
  AutomationRun,
  ActionHandlerMap,
} from "@prism/core/automation";
import {
  topologicalSort,
  detectCycles,
  findBlockingChain,
  findImpactedObjects,
  computeSlipImpact,
  computePlan,
} from "@prism/core/graph-analysis";
import type { SlipImpact, PlanResult } from "@prism/core/graph-analysis";
import { evaluateExpression } from "@prism/core/expression";
import type { ExprValue } from "@prism/core/expression";
import { PluginRegistry } from "@prism/core/plugin";
import type { PrismPlugin } from "@prism/core/plugin";
import { InputRouter, InputScope } from "@prism/core/input";
import type { InputRouterEvent } from "@prism/core/input";
import { createVaultRoster, createMemoryRosterStore } from "@prism/core/discovery";
import type { VaultRoster, RosterEntry, RosterListOptions } from "@prism/core/discovery";
import { createFormState, setFieldValue, setFieldErrors, fieldHasVisibleError, isDirty } from "@prism/core/forms";
import type { FormState } from "@prism/core/forms";
import { createIdentity, signPayload, verifySignature, exportIdentity, importIdentity } from "@prism/core/identity";
import type { PrismIdentity, ExportedIdentity } from "@prism/core/identity";
import { createVfsManager, createMemoryVfsAdapter } from "@prism/core/vfs";
import type { VfsManager, BinaryRef, BinaryLock } from "@prism/core/vfs";
import {
  createPeerTrustGraph,
  createSchemaValidator,
  createLuaSandbox,
  createShamirSplitter,
  createEscrowManager,
} from "@prism/core/trust";
import type {
  PeerTrustGraph,
  SchemaValidator,
  LuaSandbox,
  SandboxPolicy,
  ShamirSplitter,
  ShamirShare,
  ShamirConfig,
  EscrowManager,
  EscrowDeposit,
  PeerReputation,
  SchemaValidationResult,
  ContentHash,
} from "@prism/core/trust";

// ── Clipboard Types ────────────────────────────────────────────────────────

export interface ClipboardEntry {
  mode: "copy" | "cut";
  objects: GraphObject[];
  edges: ObjectEdge[];
}

export interface PasteResult {
  created: GraphObject[];
  idMap: Map<string, string>;
}

// ── Kernel Interface ────────────────────────────────────────────────────────

export interface StudioKernel {
  readonly registry: ObjectRegistry<string>;
  readonly store: CollectionStore;
  readonly bus: PrismBus;
  readonly atoms: AtomStore;
  readonly objectAtoms: ObjectAtomStore;
  readonly undo: UndoRedoManager;
  readonly notifications: NotificationStore;
  readonly search: SearchEngine;
  readonly activity: ActivityStore;
  readonly activityTracker: ActivityTracker;
  readonly relay: RelayManager;
  readonly config: ConfigModel;
  readonly configRegistry: ConfigRegistry;
  readonly presence: PresenceManager;
  readonly viewRegistry: ViewRegistry;

  // ── Automation ─────────────────────────────────────────────────────────────

  readonly automationEngine: AutomationEngine;

  /** List all automations. */
  listAutomations(): Automation[];

  /** Get a single automation by ID. */
  getAutomation(id: string): Automation | undefined;

  /** Save (create or update) an automation. */
  saveAutomation(automation: Automation): void;

  /** Delete an automation by ID. */
  deleteAutomation(id: string): void;

  /** Manually trigger an automation. */
  runAutomation(id: string): Promise<AutomationRun>;

  /** List recent automation runs. */
  listAutomationRuns(): AutomationRun[];

  /** Subscribe to automation changes. */
  onAutomationChange(listener: () => void): () => void;

  // ── Graph Analysis ────────────────────────────────────────────────────────

  /** Compute topological sort of all objects. */
  analyzeTopologicalSort(): string[];

  /** Detect dependency cycles. */
  analyzeCycles(): string[][];

  /** Find blocking chain for an object. */
  analyzeBlockingChain(objectId: string): string[];

  /** Find impacted downstream objects. */
  analyzeImpact(objectId: string): string[];

  /** Compute slip impact for an object. */
  analyzeSlipImpact(objectId: string, slipDays: number): SlipImpact[];

  /** Compute critical path plan. */
  analyzePlan(): PlanResult;

  // ── Expression ────────────────────────────────────────────────────────────

  /** Evaluate an expression formula against the current object context. */
  evaluateFormula(formula: string, objectId?: ObjectId): { result: ExprValue; errors: string[] };

  // ── Plugin System ──────────────────────────────────────────────────────────

  readonly plugins: PluginRegistry;

  /** Register a plugin. Returns unregister function. */
  registerPlugin(plugin: PrismPlugin): () => void;

  /** List all registered plugins. */
  listPlugins(): PrismPlugin[];

  /** Unregister a plugin by ID. */
  unregisterPlugin(id: string): boolean;

  /** Subscribe to plugin registry changes. */
  onPluginChange(listener: () => void): () => void;

  // ── Input System ──────────────────────────────────────────────────────────

  readonly inputRouter: InputRouter;

  /** Get all keyboard bindings from the global scope. */
  listBindings(): Array<{ shortcut: string; action: string }>;

  /** Bind a shortcut in the global scope. */
  bindShortcut(shortcut: string, action: string): void;

  /** Unbind a shortcut from the global scope. */
  unbindShortcut(shortcut: string): void;

  /** Subscribe to input router events. */
  onInputEvent(listener: (event: InputRouterEvent) => void): () => void;

  // ── Vault Discovery ───────────────────────────────────────────────────────

  readonly vaultRoster: VaultRoster;

  /** List vaults with optional filtering/sorting. */
  listVaults(options?: RosterListOptions): RosterEntry[];

  /** Add a vault to the roster. */
  addVault(entry: Omit<RosterEntry, "addedAt"> & { addedAt?: string }): RosterEntry;

  /** Remove a vault from the roster. */
  removeVault(id: string): boolean;

  /** Pin/unpin a vault. */
  pinVault(id: string, pinned: boolean): RosterEntry | undefined;

  /** Touch (update lastOpenedAt) a vault. */
  touchVault(id: string): RosterEntry | undefined;

  /** Subscribe to vault roster changes. */
  onVaultChange(listener: () => void): () => void;

  // ── Forms & Validation ────────────────────────────────────────────────────

  /** Create a form state from defaults. */
  createFormState(defaults?: Record<string, unknown>): FormState;

  /** Update a field value in form state (returns new state). */
  updateFormField(state: FormState, fieldId: string, value: unknown, original: unknown): FormState;

  /** Set field errors in form state (returns new state). */
  setFormErrors(state: FormState, fieldId: string, errors: string[]): FormState;

  /** Check if a form field has a visible error. */
  hasFieldError(state: FormState, fieldId: string): boolean;

  /** Check if form state is dirty. */
  isFormDirty(state: FormState): boolean;

  // ── Identity ──────────────────────────────────────────────────────────────

  /** The active vault identity (null before generation). */
  readonly identity: PrismIdentity | null;

  /** Generate a new Ed25519 DID identity. */
  generateIdentity(): Promise<PrismIdentity>;

  /** Export the current identity for persistence. */
  exportIdentity(): Promise<ExportedIdentity | null>;

  /** Import a previously exported identity. */
  importIdentity(exported: ExportedIdentity): Promise<PrismIdentity>;

  /** Sign arbitrary data with the current identity. */
  signData(data: Uint8Array): Promise<Uint8Array | null>;

  /** Verify a signature against the current identity. */
  verifyData(data: Uint8Array, signature: Uint8Array): Promise<boolean>;

  /** Subscribe to identity changes. */
  onIdentityChange(listener: () => void): () => void;

  // ── Virtual File System ─────────────────────────────────────────────────

  readonly vfs: VfsManager;

  /** Import a binary file into content-addressed storage. */
  importFile(data: Uint8Array, filename: string, mimeType: string): Promise<BinaryRef>;

  /** Export a file from storage. */
  exportFile(ref: BinaryRef): Promise<Uint8Array | null>;

  /** Remove a file from storage. */
  removeFile(hash: string): Promise<boolean>;

  /** List all active binary locks. */
  listLocks(): BinaryLock[];

  /** Acquire a lock on a binary blob. */
  acquireLock(hash: string, reason?: string): BinaryLock;

  /** Release a lock on a binary blob. */
  releaseLock(hash: string): void;

  /** Subscribe to VFS changes. */
  onVfsChange(listener: () => void): () => void;

  // ── Trust & Safety ──────────────────────────────────────────────────────

  readonly trustGraph: PeerTrustGraph;
  readonly schemaValidator: SchemaValidator;
  readonly escrow: EscrowManager;
  readonly shamir: ShamirSplitter;

  /** Record a positive interaction for a peer. */
  trustPeer(peerId: string): void;

  /** Record a negative interaction for a peer. */
  distrustPeer(peerId: string): void;

  /** Ban a peer. */
  banPeer(peerId: string, reason: string): void;

  /** Unban a peer. */
  unbanPeer(peerId: string): void;

  /** Get all known peers with reputation. */
  listPeers(): PeerReputation[];

  /** Validate data before import. */
  validateImport(data: unknown): SchemaValidationResult;

  /** Create a sandbox for a plugin. */
  createSandbox(policy: SandboxPolicy): LuaSandbox;

  /** Flag content as toxic. */
  flagContent(hash: string, category: string): void;

  /** Get all flagged content. */
  listFlaggedContent(): ReadonlyArray<ContentHash>;

  /** Split a secret into Shamir shares. */
  splitSecret(secret: Uint8Array, config: ShamirConfig): ShamirShare[];

  /** Combine Shamir shares to recover a secret. */
  combineShares(shares: ShamirShare[], config: ShamirConfig): Uint8Array;

  /** Deposit encrypted key material for recovery. */
  depositEscrow(encryptedPayload: string, expiresAt?: string): EscrowDeposit | null;

  /** List escrow deposits for the current identity. */
  listEscrowDeposits(): EscrowDeposit[];

  /** Subscribe to trust graph changes. */
  onTrustChange(listener: () => void): () => void;

  /** Get the active view mode. */
  readonly viewMode: ViewMode;
  /** Set the active view mode. */
  setViewMode(mode: ViewMode): void;
  /** Subscribe to view mode changes. */
  onViewModeChange(listener: () => void): () => void;

  /** Create a new object in the collection, emit bus event, push undo. */
  createObject(obj: Omit<GraphObject, "id" | "createdAt" | "updatedAt">): GraphObject;

  /** Update an existing object, emit bus event, push undo. */
  updateObject(id: ObjectId, patch: Partial<GraphObject>): GraphObject | undefined;

  /** Delete an object (soft-delete), emit bus event, push undo. */
  deleteObject(id: ObjectId): boolean;

  /** Create an edge, emit bus event. */
  createEdge(edge: Omit<ObjectEdge, "id" | "createdAt">): ObjectEdge;

  /** Delete an edge, emit bus event. */
  deleteEdge(id: EdgeId): boolean;

  /** Select an object (updates atoms + emits bus event). */
  select(id: ObjectId | null): void;

  // ── Clipboard ────────────────────────────────────────────────────────────

  /** Copy object subtrees to clipboard. */
  clipboardCopy(ids: ObjectId[]): void;

  /** Cut object subtrees to clipboard (deleted on paste). */
  clipboardCut(ids: ObjectId[]): void;

  /** Paste clipboard contents under a target parent. */
  clipboardPaste(parentId: ObjectId | null, position?: number): PasteResult | null;

  /** Whether the clipboard has content. */
  readonly clipboardHasContent: boolean;

  // ── Batch Operations ─────────────────────────────────────────────────────

  /** Execute multiple operations atomically with a single undo entry. */
  batch(
    label: string,
    operations: Array<
      | { kind: "create"; draft: Omit<GraphObject, "id" | "createdAt" | "updatedAt"> }
      | { kind: "update"; id: ObjectId; patch: Partial<GraphObject> }
      | { kind: "delete"; id: ObjectId }
    >,
  ): GraphObject[];

  // ── Templates ────────────────────────────────────────────────────────────

  /** Register a reusable template. */
  registerTemplate(template: ObjectTemplate): void;

  /** List all registered templates, optionally filtered by category. */
  listTemplates(category?: string): ObjectTemplate[];

  /** Instantiate a template under a parent. */
  instantiateTemplate(
    templateId: string,
    options?: { parentId?: ObjectId | null; position?: number; variables?: Record<string, string> },
  ): InstantiateResult | null;

  // ── LiveView ─────────────────────────────────────────────────────────────

  /** Create a live materialized view of the collection. */
  createLiveView(options?: LiveViewOptions): LiveView;

  /** Dispose all subscriptions. */
  dispose(): void;
}

// ── Helpers ─────────────────────────────────────────────────────────────────

let counter = 0;
function genId(): string {
  return `obj_${Date.now().toString(36)}_${(counter++).toString(36)}`;
}

const VARIABLE_RE = /\{\{(\w+)\}\}/g;

function interpolate(str: string, vars: Record<string, string>): string {
  return str.replace(VARIABLE_RE, (_match, name: string) => vars[name] ?? `{{${name}}}`);
}

// ── TrackableStore adapter for CollectionStore ─────────────────────────────

function createTrackableAdapter(store: CollectionStore) {
  return {
    get(id: string): unknown {
      return store.getObject(objectId(id));
    },
    subscribeObject(id: string, cb: (obj: unknown) => void): () => void {
      return store.onChange((changes) => {
        for (const change of changes) {
          if (change.id === id && (change.type === "object-put" || change.type === "object-remove")) {
            cb(store.getObject(objectId(id)));
          }
        }
      });
    },
  };
}

// ── Factory ─────────────────────────────────────────────────────────────────

export function createStudioKernel(): StudioKernel {
  const registry = createPageBuilderRegistry();
  const store = createCollectionStore();
  const bus = createPrismBus();
  const atoms = createAtomStore();
  const objectAtoms = createObjectAtomStore();
  const notifications = createNotificationStore({ maxItems: 100 });
  const search = createSearchEngine();
  const activityStore = createActivityStore();
  const tracker = createActivityTracker({ activityStore });
  const trackableAdapter = createTrackableAdapter(store);

  const relay = createRelayManager();
  const configRegistry = new ConfigRegistry();
  const config = new ConfigModel(configRegistry);
  const viewReg = createViewRegistry();

  // ── Plugin System ──────────────────────────────────────────────────────────
  const pluginRegistry = new PluginRegistry();
  const pluginListeners = new Set<() => void>();
  pluginRegistry.subscribe(() => {
    for (const fn of pluginListeners) fn();
  });

  // ── Input System ───────────────────────────────────────────────────────────
  const inputRouter = new InputRouter();
  const globalScope = new InputScope("global", "Global");

  // Register default studio shortcuts in global scope
  globalScope.keyboard.bindAll({
    "cmd+z": "undo",
    "cmd+shift+z": "redo",
    "cmd+k": "command-palette",
    "cmd+s": "save",
    "cmd+n": "new-object",
  });

  inputRouter.push(globalScope);

  // ── Vault Discovery ────────────────────────────────────────────────────────
  const rosterStore = createMemoryRosterStore();
  const vaultRoster = createVaultRoster(rosterStore);
  const vaultListeners = new Set<() => void>();
  vaultRoster.onChange(() => {
    for (const fn of vaultListeners) fn();
  });

  // ── Identity ────────────────────────────────────────────────────────────
  let currentIdentity: PrismIdentity | null = null;
  const identityListeners = new Set<() => void>();

  function notifyIdentityListeners() {
    for (const fn of identityListeners) fn();
  }

  // ── Virtual File System ────────────────────────────────────────────────
  const vfsAdapter = createMemoryVfsAdapter();
  const vfs = createVfsManager({ adapter: vfsAdapter });
  const vfsListeners = new Set<() => void>();
  function notifyVfsListeners() {
    for (const fn of vfsListeners) fn();
  }

  // ── Trust & Safety ─────────────────────────────────────────────────────
  const trustGraph = createPeerTrustGraph();
  const schemaValidator = createSchemaValidator();
  const shamirSplitter = createShamirSplitter();
  const escrowManager = createEscrowManager();
  const sandboxes = new Map<string, LuaSandbox>();
  const trustListeners = new Set<() => void>();

  function notifyTrustListeners() {
    for (const fn of trustListeners) fn();
  }

  const disconnectTrustGraph = trustGraph.onChange(() => {
    notifyTrustListeners();
  });

  const presence = createPresenceManager({
    localIdentity: {
      peerId: `local_${Date.now().toString(36)}`,
      displayName: "You",
      color: "#4a9eff",
    },
  });

  let currentViewMode: ViewMode = "list";
  const viewModeListeners = new Set<() => void>();

  // ── Automation ──────────────────────────────────────────────────────────────

  const automationRuns: AutomationRun[] = [];
  const automationMap = new Map<string, Automation>();
  const automationListeners = new Set<() => void>();

  function notifyAutomationListeners() {
    for (const fn of automationListeners) fn();
  }

  const automationStore: AutomationStore = {
    list(filter?) {
      let all = [...automationMap.values()];
      if (filter?.enabled !== undefined) all = all.filter((a) => a.enabled === filter.enabled);
      if (filter?.triggerType) all = all.filter((a) => a.trigger.type === filter.triggerType);
      return all;
    },
    get(id) { return automationMap.get(id); },
    save(automation) {
      automationMap.set(automation.id, automation);
      notifyAutomationListeners();
    },
    saveRun(run) {
      automationRuns.push(run);
      if (automationRuns.length > 100) automationRuns.shift();
      notifyAutomationListeners();
    },
  };

  const actionHandlers: ActionHandlerMap = {
    "object:create": async (action) => {
      const a = action as { type: "object:create"; objectType: string; template: Record<string, unknown> };
      createObject({
        type: a.objectType,
        name: (a.template.name as string) ?? `New ${a.objectType}`,
        parentId: null,
        position: 0,
        status: (a.template.status as string) ?? null,
        tags: (a.template.tags as string[]) ?? [],
        date: null,
        endDate: null,
        description: (a.template.description as string) ?? "",
        color: null,
        image: null,
        pinned: false,
        data: a.template,
      });
    },
    "object:update": async (action, ctx) => {
      const a = action as { type: "object:update"; target: string; patch: Record<string, unknown> };
      const targetId = a.target === "trigger" && ctx.object
        ? objectId(ctx.object.id as string)
        : objectId(a.target);
      updateObject(targetId, a.patch as Partial<GraphObject>);
    },
    "object:delete": async (action, ctx) => {
      const a = action as { type: "object:delete"; target: string };
      const targetId = a.target === "trigger" && ctx.object
        ? objectId(ctx.object.id as string)
        : objectId(a.target);
      deleteObject(targetId);
    },
    "notification:send": async (action) => {
      const a = action as { type: "notification:send"; title: string; body: string };
      notifications.add({ title: a.title, kind: "info", body: a.body });
    },
    "automation:run": async (action) => {
      const a = action as { type: "automation:run"; automationId: string };
      await automationEngine.run(a.automationId);
    },
  };

  const automationEngine = new AutomationEngine(automationStore, actionHandlers, {
    onRunComplete: (run) => {
      if (run.status === "failed") {
        notifications.add({
          title: `Automation failed: ${automationMap.get(run.automationId)?.name ?? run.automationId}`,
          kind: "error",
          body: run.actionResults.find((r) => r.error)?.error,
        });
      }
    },
  });

  // Wire bus events → automation engine
  const disconnectAutomationCreated = bus.on(PrismEvents.ObjectCreated, (payload: unknown) => {
    const p = payload as { object: GraphObject };
    void automationEngine.handleObjectEvent({
      type: "object:created",
      object: p.object as unknown as Record<string, unknown>,
    });
  });
  const disconnectAutomationUpdated = bus.on(PrismEvents.ObjectUpdated, (payload: unknown) => {
    const p = payload as { object: GraphObject };
    void automationEngine.handleObjectEvent({
      type: "object:updated",
      object: p.object as unknown as Record<string, unknown>,
    });
  });
  const disconnectAutomationDeleted = bus.on(PrismEvents.ObjectDeleted, (payload: unknown) => {
    const p = payload as { id: string };
    void automationEngine.handleObjectEvent({
      type: "object:deleted",
      object: { id: p.id } as Record<string, unknown>,
    });
  });

  automationEngine.start();

  // Index the default collection so search covers all kernel objects
  search.indexCollection("default", store);

  // Wire bus → atom stores (selection, navigation, object cache)
  const disconnectAtoms = connectBusToAtoms(bus, atoms);
  const disconnectObjectAtoms = connectBusToObjectAtoms(bus, objectAtoms);

  // Undo applier — restores snapshots to CollectionStore
  const undoApplier = (snapshots: ObjectSnapshot[], direction: "undo" | "redo") => {
    for (const snap of snapshots) {
      if (snap.kind === "object") {
        const target = direction === "undo" ? snap.before : snap.after;
        if (target) {
          store.putObject(target);
          objectAtoms.getState().setObject(target);
          bus.emit(PrismEvents.ObjectUpdated, { object: target });
        } else {
          const source = direction === "undo" ? snap.after : snap.before;
          if (source) {
            store.removeObject(source.id);
            objectAtoms.getState().removeObject(source.id);
            bus.emit(PrismEvents.ObjectDeleted, { id: source.id });
          }
        }
      } else {
        const target = direction === "undo" ? snap.before : snap.after;
        if (target) {
          store.putEdge(target);
          objectAtoms.getState().setEdge(target);
          bus.emit(PrismEvents.EdgeCreated, { edge: target });
        } else {
          const source = direction === "undo" ? snap.after : snap.before;
          if (source) {
            store.removeEdge(source.id);
            objectAtoms.getState().removeEdge(source.id);
            bus.emit(PrismEvents.EdgeDeleted, { id: source.id });
          }
        }
      }
    }
  };

  const undo = new UndoRedoManager(undoApplier, { maxHistory: 200 });

  // Sync CollectionStore changes → ObjectAtomStore (keeps cache in sync)
  const disconnectStoreSync = store.onChange((changes) => {
    for (const change of changes) {
      if (change.type === "object-put") {
        const obj = store.getObject(objectId(change.id));
        if (obj) objectAtoms.getState().setObject(obj);
      } else if (change.type === "object-remove") {
        objectAtoms.getState().removeObject(change.id);
      } else if (change.type === "edge-put") {
        const edge = store.getEdge(edgeId(change.id));
        if (edge) objectAtoms.getState().setEdge(edge);
      } else if (change.type === "edge-remove") {
        objectAtoms.getState().removeEdge(change.id);
      }
    }
  });

  // Template storage
  const templates = new Map<string, ObjectTemplate>();

  // Clipboard storage
  let clipboardEntry: ClipboardEntry | null = null;

  // ── Internal helpers ─────────────────────────────────────────────────────

  function getDescendants(id: ObjectId): GraphObject[] {
    const result: GraphObject[] = [];
    const queue = store.listObjects({ parentId: id }).filter((o) => !o.deletedAt);
    while (queue.length > 0) {
      const obj = queue.shift() as GraphObject;
      result.push(obj);
      queue.push(...store.listObjects({ parentId: obj.id }).filter((o) => !o.deletedAt));
    }
    return result;
  }

  function collectSubtree(id: ObjectId): { objects: GraphObject[]; edges: ObjectEdge[] } {
    const root = store.getObject(id);
    if (!root) return { objects: [], edges: [] };

    const descendants = getDescendants(id);
    const all = [root, ...descendants];
    const allIds = new Set(all.map((o) => o.id as string));

    // Collect internal edges (both endpoints in subtree)
    const internalEdges: ObjectEdge[] = [];
    for (const edge of store.allEdges()) {
      if (allIds.has(edge.sourceId as string) && allIds.has(edge.targetId as string)) {
        internalEdges.push(edge);
      }
    }

    return { objects: all, edges: internalEdges };
  }

  // ── CRUD with undo + bus + activity tracking ───────────────────────────

  function createObject(
    partial: Omit<GraphObject, "id" | "createdAt" | "updatedAt">,
  ): GraphObject {
    const now = new Date().toISOString();
    const obj: GraphObject = {
      ...partial,
      id: objectId(genId()),
      createdAt: now,
      updatedAt: now,
    };

    store.putObject(obj);
    bus.emit(PrismEvents.ObjectCreated, { object: obj });

    undo.push(`Create ${obj.type} "${obj.name}"`, [
      { kind: "object", before: null, after: structuredClone(obj) },
    ]);

    // Track activity
    activityStore.record({
      objectId: obj.id,
      verb: "created",
      meta: { objectType: obj.type, objectName: obj.name },
    });

    // Start tracking future changes
    tracker.track(obj.id, trackableAdapter);

    return obj;
  }

  function updateObject(
    id: ObjectId,
    patch: Partial<GraphObject>,
  ): GraphObject | undefined {
    const before = store.getObject(id);
    if (!before) return undefined;

    const after: GraphObject = {
      ...before,
      ...patch,
      id: before.id, // never overwrite identity
      updatedAt: new Date().toISOString(),
    };

    store.putObject(after);
    bus.emit(PrismEvents.ObjectUpdated, { object: after });

    undo.push(`Update ${after.type} "${after.name}"`, [
      { kind: "object", before: structuredClone(before), after: structuredClone(after) },
    ]);

    return after;
  }

  function deleteObject(id: ObjectId): boolean {
    const before = store.getObject(id);
    if (!before) return false;

    const after: GraphObject = {
      ...before,
      deletedAt: new Date().toISOString(),
      updatedAt: new Date().toISOString(),
    };

    store.putObject(after);
    bus.emit(PrismEvents.ObjectUpdated, { object: after });

    undo.push(`Delete ${before.type} "${before.name}"`, [
      { kind: "object", before: structuredClone(before), after: structuredClone(after) },
    ]);

    activityStore.record({
      objectId: id,
      verb: "deleted",
      meta: { objectType: before.type, objectName: before.name },
    });

    return true;
  }

  function createEdge(
    partial: Omit<ObjectEdge, "id" | "createdAt">,
  ): ObjectEdge {
    const edge: ObjectEdge = {
      ...partial,
      id: edgeId(genId()),
      createdAt: new Date().toISOString(),
    };

    store.putEdge(edge);
    bus.emit(PrismEvents.EdgeCreated, { edge });

    undo.push(`Create edge "${edge.relation}"`, [
      { kind: "edge", before: null, after: structuredClone(edge) },
    ]);

    return edge;
  }

  function deleteEdge(id: EdgeId): boolean {
    const before = store.getEdge(id);
    if (!before) return false;

    store.removeEdge(id);
    bus.emit(PrismEvents.EdgeDeleted, { id });

    undo.push(`Delete edge "${before.relation}"`, [
      { kind: "edge", before: structuredClone(before), after: null },
    ]);

    return true;
  }

  function select(id: ObjectId | null): void {
    atoms.getState().setSelectedId(id);
    bus.emit(PrismEvents.SelectionChanged, { ids: id ? [id] : [] });
  }

  // ── Clipboard ────────────────────────────────────────────────────────────

  function clipboardCopy(ids: ObjectId[]): void {
    const objects: GraphObject[] = [];
    const edges: ObjectEdge[] = [];
    const allIds = new Set<string>();

    for (const id of ids) {
      const { objects: sub, edges: subEdges } = collectSubtree(id);
      for (const o of sub) {
        if (!allIds.has(o.id as string)) {
          allIds.add(o.id as string);
          objects.push(structuredClone(o));
        }
      }
      edges.push(...subEdges.map((e) => structuredClone(e)));
    }

    clipboardEntry = { mode: "copy", objects, edges };
  }

  function clipboardCut(ids: ObjectId[]): void {
    clipboardCopy(ids);
    if (clipboardEntry) {
      clipboardEntry.mode = "cut";
    }
  }

  function clipboardPaste(parentId: ObjectId | null, position?: number): PasteResult | null {
    if (!clipboardEntry) return null;

    const { mode, objects, edges: clipEdges } = clipboardEntry;
    const idMap = new Map<string, string>();
    const created: GraphObject[] = [];
    const snapshots: ObjectSnapshot[] = [];

    // Generate new IDs for all objects
    for (const obj of objects) {
      idMap.set(obj.id as string, genId());
    }

    // Sort by depth (roots first) — objects whose parentId is NOT in the set are roots
    const rootIds = new Set(
      objects
        .filter((o) => !idMap.has(o.parentId as string))
        .map((o) => o.id as string),
    );

    const now = new Date().toISOString();
    let posCounter = position ?? 0;

    for (const obj of objects) {
      const newId = objectId(idMap.get(obj.id as string) as string);
      const isRoot = rootIds.has(obj.id as string);

      const newObj: GraphObject = {
        ...obj,
        id: newId,
        parentId: isRoot ? parentId : objectId(idMap.get(obj.parentId as string) as string),
        position: isRoot ? posCounter++ : obj.position,
        createdAt: now,
        updatedAt: now,
        deletedAt: null,
      };

      store.putObject(newObj);
      bus.emit(PrismEvents.ObjectCreated, { object: newObj });
      created.push(newObj);
      snapshots.push({ kind: "object", before: null, after: structuredClone(newObj) });

      tracker.track(newObj.id, trackableAdapter);
    }

    // Re-create internal edges with remapped IDs
    for (const edge of clipEdges) {
      const newSourceId = idMap.get(edge.sourceId as string);
      const newTargetId = idMap.get(edge.targetId as string);
      if (newSourceId && newTargetId) {
        const newEdge: ObjectEdge = {
          ...edge,
          id: edgeId(genId()),
          sourceId: objectId(newSourceId),
          targetId: objectId(newTargetId),
          createdAt: now,
        };
        store.putEdge(newEdge);
        bus.emit(PrismEvents.EdgeCreated, { edge: newEdge });
        snapshots.push({ kind: "edge", before: null, after: structuredClone(newEdge) });
      }
    }

    undo.push(`Paste ${created.length} object(s)`, snapshots);

    // If cut, delete the originals
    if (mode === "cut") {
      const cutSnapshots: ObjectSnapshot[] = [];
      for (const obj of objects) {
        const original = store.getObject(obj.id);
        if (original) {
          const deleted: GraphObject = {
            ...original,
            deletedAt: now,
            updatedAt: now,
          };
          store.putObject(deleted);
          bus.emit(PrismEvents.ObjectUpdated, { object: deleted });
          cutSnapshots.push({
            kind: "object",
            before: structuredClone(original),
            after: structuredClone(deleted),
          });
        }
      }
      if (cutSnapshots.length > 0) {
        undo.push(`Cut ${cutSnapshots.length} object(s)`, cutSnapshots);
      }
      clipboardEntry = null; // cut is one-time
    }

    return { created, idMap };
  }

  // ── Batch Operations ─────────────────────────────────────────────────────

  function batchOps(
    label: string,
    operations: Array<
      | { kind: "create"; draft: Omit<GraphObject, "id" | "createdAt" | "updatedAt"> }
      | { kind: "update"; id: ObjectId; patch: Partial<GraphObject> }
      | { kind: "delete"; id: ObjectId }
    >,
  ): GraphObject[] {
    const snapshots: ObjectSnapshot[] = [];
    const results: GraphObject[] = [];
    const now = new Date().toISOString();

    for (const op of operations) {
      switch (op.kind) {
        case "create": {
          const obj: GraphObject = {
            ...op.draft,
            id: objectId(genId()),
            createdAt: now,
            updatedAt: now,
          };
          store.putObject(obj);
          bus.emit(PrismEvents.ObjectCreated, { object: obj });
          snapshots.push({ kind: "object", before: null, after: structuredClone(obj) });
          results.push(obj);
          tracker.track(obj.id, trackableAdapter);
          break;
        }
        case "update": {
          const before = store.getObject(op.id);
          if (before) {
            const after: GraphObject = {
              ...before,
              ...op.patch,
              id: before.id,
              updatedAt: now,
            };
            store.putObject(after);
            bus.emit(PrismEvents.ObjectUpdated, { object: after });
            snapshots.push({
              kind: "object",
              before: structuredClone(before),
              after: structuredClone(after),
            });
            results.push(after);
          }
          break;
        }
        case "delete": {
          const before = store.getObject(op.id);
          if (before) {
            const after: GraphObject = {
              ...before,
              deletedAt: now,
              updatedAt: now,
            };
            store.putObject(after);
            bus.emit(PrismEvents.ObjectUpdated, { object: after });
            snapshots.push({
              kind: "object",
              before: structuredClone(before),
              after: structuredClone(after),
            });
            results.push(after);
          }
          break;
        }
      }
    }

    if (snapshots.length > 0) {
      undo.push(label, snapshots);
    }

    return results;
  }

  // ── Templates ────────────────────────────────────────────────────────────

  function registerTemplate(template: ObjectTemplate): void {
    templates.set(template.id, template);
  }

  function listTemplates(category?: string): ObjectTemplate[] {
    const all = [...templates.values()];
    if (!category) return all;
    return all.filter((t) => t.category === category);
  }

  function instantiateTemplateNode(
    node: TemplateNode,
    parentId: ObjectId | null,
    position: number,
    vars: Record<string, string>,
    idMap: Map<string, string>,
    created: GraphObject[],
    snapshots: ObjectSnapshot[],
  ): void {
    const now = new Date().toISOString();
    const newId = genId();
    idMap.set(node.placeholderId, newId);

    // Interpolate string fields
    const data: Record<string, unknown> = {};
    if (node.data) {
      for (const [k, v] of Object.entries(node.data)) {
        data[k] = typeof v === "string" ? interpolate(v, vars) : v;
      }
    }

    const obj: GraphObject = {
      type: node.type,
      name: interpolate(node.name, vars),
      parentId,
      position,
      status: node.status ? interpolate(node.status, vars) : null,
      tags: node.tags ?? [],
      date: null,
      endDate: null,
      description: node.description ? interpolate(node.description, vars) : "",
      color: node.color ?? null,
      image: null,
      pinned: node.pinned ?? false,
      data,
      id: objectId(newId),
      createdAt: now,
      updatedAt: now,
    };

    store.putObject(obj);
    bus.emit(PrismEvents.ObjectCreated, { object: obj });
    created.push(obj);
    snapshots.push({ kind: "object", before: null, after: structuredClone(obj) });
    tracker.track(obj.id, trackableAdapter);

    // Recurse into children
    if (node.children) {
      for (let i = 0; i < node.children.length; i++) {
        instantiateTemplateNode(
          node.children[i] as TemplateNode,
          obj.id,
          i,
          vars,
          idMap,
          created,
          snapshots,
        );
      }
    }
  }

  function instantiateTemplate(
    templateId: string,
    options?: { parentId?: ObjectId | null; position?: number; variables?: Record<string, string> },
  ): InstantiateResult | null {
    const template = templates.get(templateId);
    if (!template) return null;

    const vars = options?.variables ?? {};
    const parentId = options?.parentId ?? null;
    const position = options?.position ?? 0;
    const idMap = new Map<string, string>();
    const created: GraphObject[] = [];
    const createdEdges: ObjectEdge[] = [];
    const snapshots: ObjectSnapshot[] = [];

    instantiateTemplateNode(template.root, parentId, position, vars, idMap, created, snapshots);

    // Create template edges with remapped IDs
    if (template.edges) {
      const now = new Date().toISOString();
      for (const te of template.edges) {
        const sourceId = idMap.get(te.sourcePlaceholderId);
        const targetId = idMap.get(te.targetPlaceholderId);
        if (sourceId && targetId) {
          const edge: ObjectEdge = {
            id: edgeId(genId()),
            sourceId: objectId(sourceId),
            targetId: objectId(targetId),
            relation: te.relation,
            data: te.data ?? {},
            createdAt: now,
          };
          store.putEdge(edge);
          bus.emit(PrismEvents.EdgeCreated, { edge });
          createdEdges.push(edge);
          snapshots.push({ kind: "edge", before: null, after: structuredClone(edge) });
        }
      }
    }

    if (snapshots.length > 0) {
      undo.push(`Instantiate template "${template.name}"`, snapshots);
    }

    return { created, createdEdges, idMap };
  }

  // ── LiveView factory ─────────────────────────────────────────────────────

  function makeLiveView(options?: LiveViewOptions): LiveView {
    return createLiveView(store, options);
  }

  // ── Dispose ──────────────────────────────────────────────────────────────

  function setViewMode(mode: ViewMode): void {
    if (mode === currentViewMode) return;
    currentViewMode = mode;
    for (const fn of viewModeListeners) fn();
  }

  // ── Graph Analysis helpers ─────────────────────────────────────────────────

  function getAllObjects(): GraphObject[] {
    return store.listObjects().filter((o) => !o.deletedAt);
  }

  function analyzeTopologicalSort(): string[] {
    return topologicalSort(getAllObjects());
  }

  function analyzeCycles(): string[][] {
    return detectCycles(getAllObjects());
  }

  function analyzeBlockingChain(id: string): string[] {
    return findBlockingChain(id, getAllObjects());
  }

  function analyzeImpact(id: string): string[] {
    return findImpactedObjects(id, getAllObjects());
  }

  function analyzeSlipImpact(id: string, slipDays: number): SlipImpact[] {
    return computeSlipImpact(id, slipDays, getAllObjects());
  }

  function analyzePlan(): PlanResult {
    return computePlan(getAllObjects());
  }

  // ── Expression helpers ─────────────────────────────────────────────────────

  function evaluateFormula(
    formula: string,
    targetId?: ObjectId,
  ): { result: ExprValue; errors: string[] } {
    const ctx: Record<string, ExprValue> = {};

    if (targetId) {
      const obj = store.getObject(targetId);
      if (obj) {
        ctx.name = obj.name;
        ctx.type = obj.type;
        ctx.status = obj.status ?? "";
        ctx.position = obj.position ?? 0;
        if (obj.data) {
          for (const [k, v] of Object.entries(obj.data)) {
            if (typeof v === "number" || typeof v === "string" || typeof v === "boolean") {
              ctx[k] = v;
            }
          }
        }
      }
    }

    return evaluateExpression(formula, ctx);
  }

  // ── Identity helpers ─────────────────────────────────────────────────────

  async function generateIdentityImpl(): Promise<PrismIdentity> {
    currentIdentity = await createIdentity();
    notifyIdentityListeners();
    notifications.add({ title: `Identity created: ${currentIdentity.did}`, kind: "success" });
    return currentIdentity;
  }

  async function exportIdentityImpl(): Promise<ExportedIdentity | null> {
    if (!currentIdentity) return null;
    return exportIdentity(currentIdentity);
  }

  async function importIdentityImpl(exported: ExportedIdentity): Promise<PrismIdentity> {
    currentIdentity = await importIdentity(exported);
    notifyIdentityListeners();
    notifications.add({ title: `Identity imported: ${currentIdentity.did}`, kind: "success" });
    return currentIdentity;
  }

  async function signDataImpl(data: Uint8Array): Promise<Uint8Array | null> {
    if (!currentIdentity) return null;
    return signPayload(currentIdentity, data);
  }

  async function verifyDataImpl(data: Uint8Array, signature: Uint8Array): Promise<boolean> {
    if (!currentIdentity) return false;
    return verifySignature(currentIdentity.did, data, signature);
  }

  // ── VFS helpers ─────────────────────────────────────────────────────────

  async function importFileImpl(data: Uint8Array, filename: string, mimeType: string): Promise<BinaryRef> {
    const ref = await vfs.importFile(data, filename, mimeType);
    notifyVfsListeners();
    return ref;
  }

  async function exportFileImpl(ref: BinaryRef): Promise<Uint8Array | null> {
    return vfs.exportFile(ref);
  }

  async function removeFileImpl(hash: string): Promise<boolean> {
    const result = await vfs.removeFile(hash);
    if (result) notifyVfsListeners();
    return result;
  }

  function acquireLockImpl(hash: string, reason?: string): BinaryLock {
    const peerId = currentIdentity?.did ?? `local_${Date.now().toString(36)}`;
    const lock = vfs.acquireLock(hash, peerId, reason);
    notifyVfsListeners();
    return lock;
  }

  function releaseLockImpl(hash: string): void {
    const peerId = currentIdentity?.did ?? `local_${Date.now().toString(36)}`;
    vfs.releaseLock(hash, peerId);
    notifyVfsListeners();
  }

  function dispose(): void {
    automationEngine.stop();
    disconnectAutomationCreated();
    disconnectAutomationUpdated();
    disconnectAutomationDeleted();
    automationListeners.clear();
    pluginListeners.clear();
    vaultListeners.clear();
    relay.dispose();
    presence.dispose();
    disconnectAtoms();
    disconnectObjectAtoms();
    disconnectStoreSync();
    tracker.untrackAll();
    viewModeListeners.clear();
    identityListeners.clear();
    vfsListeners.clear();
    vfs.dispose();
    disconnectTrustGraph();
    trustGraph.dispose();
    trustListeners.clear();
    sandboxes.clear();
  }

  return {
    registry,
    store,
    bus,
    atoms,
    objectAtoms,
    undo,
    notifications,
    search,
    activity: activityStore,
    activityTracker: tracker,
    relay,
    config,
    configRegistry,
    presence,
    viewRegistry: viewReg,
    automationEngine,
    listAutomations() { return automationStore.list(); },
    getAutomation(id: string) { return automationStore.get(id); },
    saveAutomation(automation: Automation) {
      automationStore.save(automation);
      automationEngine.refreshAutomation(automation.id);
    },
    deleteAutomation(id: string) {
      automationMap.delete(id);
      automationEngine.refreshAutomation(id);
      notifyAutomationListeners();
    },
    async runAutomation(id: string) { return automationEngine.run(id); },
    listAutomationRuns() { return [...automationRuns]; },
    onAutomationChange(listener: () => void) {
      automationListeners.add(listener);
      return () => automationListeners.delete(listener);
    },
    analyzeTopologicalSort,
    analyzeCycles,
    analyzeBlockingChain,
    analyzeImpact,
    analyzeSlipImpact,
    analyzePlan,
    evaluateFormula,
    get viewMode() { return currentViewMode; },
    setViewMode,
    onViewModeChange(listener: () => void) {
      viewModeListeners.add(listener);
      return () => viewModeListeners.delete(listener);
    },
    createObject,
    updateObject,
    deleteObject,
    createEdge,
    deleteEdge,
    select,
    clipboardCopy,
    clipboardCut,
    clipboardPaste,
    get clipboardHasContent() {
      return clipboardEntry !== null;
    },
    batch: batchOps,
    registerTemplate,
    listTemplates,
    instantiateTemplate,
    createLiveView: makeLiveView,

    // ── Plugin System ──────────────────────────────────────────────────────
    plugins: pluginRegistry,
    registerPlugin(plugin: PrismPlugin) { return pluginRegistry.register(plugin); },
    listPlugins() { return pluginRegistry.all(); },
    unregisterPlugin(id: string) { return pluginRegistry.unregister(id); },
    onPluginChange(listener: () => void) {
      pluginListeners.add(listener);
      return () => pluginListeners.delete(listener);
    },

    // ── Input System ──────────────────────────────────────────────────────
    inputRouter,
    listBindings() { return globalScope.keyboard.allBindings(); },
    bindShortcut(shortcut: string, action: string) { globalScope.keyboard.bind(shortcut, action); },
    unbindShortcut(shortcut: string) { globalScope.keyboard.unbind(shortcut); },
    onInputEvent(listener: (event: InputRouterEvent) => void) { return inputRouter.on(listener); },

    // ── Vault Discovery ────────────────────────────────────────────────────
    vaultRoster,
    listVaults(options?: RosterListOptions) { return vaultRoster.list(options); },
    addVault(entry: Omit<RosterEntry, "addedAt"> & { addedAt?: string }) { return vaultRoster.add(entry); },
    removeVault(id: string) { return vaultRoster.remove(id); },
    pinVault(id: string, pinned: boolean) { return vaultRoster.pin(id, pinned); },
    touchVault(id: string) { return vaultRoster.touch(id); },
    onVaultChange(listener: () => void) {
      vaultListeners.add(listener);
      return () => vaultListeners.delete(listener);
    },

    // ── Forms & Validation ─────────────────────────────────────────────────
    createFormState(defaults?: Record<string, unknown>) { return createFormState(defaults); },
    updateFormField(state: FormState, fieldId: string, value: unknown, original: unknown) {
      return setFieldValue(state, fieldId, value, original);
    },
    setFormErrors(state: FormState, fieldId: string, errors: string[]) {
      return setFieldErrors(state, fieldId, errors);
    },
    hasFieldError(state: FormState, fieldId: string) {
      return fieldHasVisibleError(state, fieldId);
    },
    isFormDirty(state: FormState) { return isDirty(state); },

    // ── Identity ──────────────────────────────────────────────────────────
    get identity() { return currentIdentity; },
    generateIdentity: generateIdentityImpl,
    exportIdentity: exportIdentityImpl,
    importIdentity: importIdentityImpl,
    signData: signDataImpl,
    verifyData: verifyDataImpl,
    onIdentityChange(listener: () => void) {
      identityListeners.add(listener);
      return () => identityListeners.delete(listener);
    },

    // ── Virtual File System ───────────────────────────────────────────────
    vfs,
    importFile: importFileImpl,
    exportFile: exportFileImpl,
    removeFile: removeFileImpl,
    listLocks() { return vfs.listLocks(); },
    acquireLock: acquireLockImpl,
    releaseLock: releaseLockImpl,
    onVfsChange(listener: () => void) {
      vfsListeners.add(listener);
      return () => vfsListeners.delete(listener);
    },

    // ── Trust & Safety ────────────────────────────────────────────────────
    trustGraph,
    schemaValidator,
    escrow: escrowManager,
    shamir: shamirSplitter,
    trustPeer(peerId: string) { trustGraph.recordPositive(peerId); },
    distrustPeer(peerId: string) { trustGraph.recordNegative(peerId); },
    banPeer(peerId: string, reason: string) {
      trustGraph.ban(peerId, reason);
      notifications.add({ title: `Peer banned: ${peerId}`, kind: "warning", body: reason });
    },
    unbanPeer(peerId: string) {
      trustGraph.unban(peerId);
      notifications.add({ title: `Peer unbanned: ${peerId}`, kind: "info" });
    },
    listPeers() { return trustGraph.allPeers(); },
    validateImport(data: unknown) { return schemaValidator.validate(data); },
    createSandbox(policy: SandboxPolicy) {
      const sandbox = createLuaSandbox(policy);
      sandboxes.set(policy.pluginId, sandbox);
      return sandbox;
    },
    flagContent(hash: string, category: string) {
      const reportedBy = currentIdentity?.did ?? "local";
      trustGraph.flagContent(hash, category, reportedBy);
    },
    listFlaggedContent() { return trustGraph.flaggedContent(); },
    splitSecret(secret: Uint8Array, config: ShamirConfig) {
      return shamirSplitter.split(secret, config);
    },
    combineShares(shares: ShamirShare[], config: ShamirConfig) {
      return shamirSplitter.combine(shares, config);
    },
    depositEscrow(encryptedPayload: string, expiresAt?: string) {
      if (!currentIdentity) return null;
      return escrowManager.deposit(currentIdentity.did, encryptedPayload, expiresAt);
    },
    listEscrowDeposits() {
      if (!currentIdentity) return [];
      return escrowManager.listDeposits(currentIdentity.did);
    },
    onTrustChange(listener: () => void) {
      trustListeners.add(listener);
      return () => trustListeners.delete(listener);
    },

    dispose,
  };
}
