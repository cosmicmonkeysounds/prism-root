export { createLoroBridge } from "./foundation/loro-bridge.js";
export type { LoroBridge, LoroChangeHandler } from "./foundation/loro-bridge.js";
export { createCrdtStore } from "./foundation/crdt-stores/use-crdt-store.js";
export type { CrdtStore, CrdtStoreState, CrdtStoreActions } from "./foundation/crdt-stores/use-crdt-store.js";
export { executeLuau, createLuauEngine } from "./language/luau/luau-runtime.js";
export type { LuauEngine } from "./language/luau/luau-runtime.js";
export { createLuauDebugger } from "./language/luau/luau-debugger.js";
export type { LuauDebugger, TraceFrame, DebugRunResult } from "./language/luau/luau-debugger.js";

// ── Object Model ───────────────────────────────────────────────────────────────
export {
  objectId,
  edgeId,
  ObjectRegistry,
  TreeModel,
  TreeModelError,
  EdgeModel,
  WeakRefEngine,
  NSIDRegistry,
  isValidNSID,
  parseNSID,
  nsid,
  nsidAuthority,
  nsidName,
  isValidPrismAddress,
  prismAddress,
  parsePrismAddress,
  queryToParams,
  paramsToQuery,
  matchesQuery,
  sortObjects,
  ContextEngine,
  pascal,
  camel,
  singular,
} from "./foundation/object-model/index.js";

// ── Lens System & Shell ───────────────────────────────────────────────────────
export {
  lensId,
  tabId,
  createLensRegistry,
  createShellStore,
} from "./interaction/lens/index.js";

export type {
  LensId,
  TabId,
  LensCategory,
  LensCommand,
  LensKeybinding,
  LensView,
  LensManifest,
  LensRegistry,
  LensRegistryEvent,
  LensRegistryEventListener,
  TabEntry,
  PanelLayout,
  ShellState,
  ShellActions,
  ShellStore,
} from "./interaction/lens/index.js";

// ── Input System ──────────────────────────────────────────────────────────────
export { parseShortcut, normaliseKeyEvent, keyToShortcut, KeyboardModel } from "./interaction/input/index.js";
export { InputScope } from "./interaction/input/index.js";
export { InputRouter } from "./interaction/input/index.js";

export type {
  NormalisedKey,
  KeyEventLike,
  UndoHook,
  InputRouterEvent,
  InputRouterListener,
} from "./interaction/input/index.js";

// ── Forms & Validation ────────────────────────────────────────────────────────
export {
  isTextSection,
  isFieldGroupSection,
  getField,
  orderedFieldIds,
  orderedFields,
  NOTES_TEXT_SECTION,
  DESCRIPTION_TEXT_SECTION,
  createFormState,
  setFieldValue,
  setFieldErrors,
  touchField,
  setAllErrors,
  setSubmitting,
  resetFormState,
  isDirty,
  isTouchedValid,
  fieldErrors,
  fieldHasVisibleError,
  parseWikiLinks,
  extractLinkedIds,
  renderWikiLinks,
  buildWikiLink,
  detectInlineLink,
  parseMarkdown,
  parseInline,
  inlineToPlainText,
  extractWikiIds,
} from "./language/forms/index.js";

export type {
  FieldType,
  SelectOption,
  FieldSchema,
  TextSection,
  FieldGroupSection,
  SectionDef,
  DocumentSchema,
  ValidatorType,
  ValidationRule,
  FieldValidation,
  ConditionalOperator,
  FieldCondition,
  ConditionalRule,
  FormSchema,
  FormState,
  WikiToken,
  BlockToken,
  InlineToken,
} from "./language/forms/index.js";

// ── Layout System ─────────────────────────────────────────────────────────────
export { SelectionModel } from "./interaction/layout/index.js";
export { PageModel } from "./interaction/layout/index.js";
export { PageRegistry } from "./interaction/layout/index.js";
export { LensSlot } from "./interaction/layout/index.js";
export { LensManager } from "./interaction/layout/index.js";

export type {
  SerializedPage,
  PageModelEvent,
  PageModelListener,
  PageModelOptions,
  SelectionEvent,
  SelectionListener,
  LensSlotEvent,
  LensSlotListener,
  LensManagerEvent,
  LensManagerListener,
  PageTypeDef,
  LensSlotOptions,
} from "./interaction/layout/index.js";

// ── Expression Engine ─────────────────────────────────────────────────────────
export { tokenize, isDigit, isIdentStart, isIdentChar } from "./language/expression/index.js";
export { parse } from "./language/expression/index.js";
export { evaluate, evaluateExpression } from "./language/expression/index.js";

export type {
  ExprType,
  ExprValue,
  ExprError,
  LiteralNode,
  OperandNode,
  BinaryOp,
  BinaryNode,
  UnaryNode,
  CallNode,
  AnyExprNode,
  ParseResult,
  ValueStore,
  TokenKind,
  OperandData,
  Token,
} from "./language/expression/index.js";

export type {
  ObjectId,
  EdgeId,
  EntityFieldType,
  EntityFieldDef,
  GraphObject,
  ObjectEdge,
  ResolvedEdge,
  EdgeBehavior,
  EdgeTypeDef,
  EntityDef,
  CategoryRule,
  TabDefinition,
  SlotDef,
  SlotRegistration,
  TreeNode,
  WeakRefChildNode,
  TreeModelEvent,
  TreeModelEventListener,
  TreeModelHooks,
  TreeModelErrorCode,
  TreeModelOptions,
  AddOptions,
  DuplicateOptions,
  EdgeModelEvent,
  EdgeModelEventListener,
  EdgeModelHooks,
  EdgeModelOptions,
  WeakRefExtraction,
  WeakRefProvider,
  WeakRefChild,
  WeakRefEngineEvent,
  WeakRefEngineEventListener,
  WeakRefEngineOptions,
  NSID,
  PrismAddress,
  ObjectQuery,
  EdgeOption,
  ChildOption,
  ContextMenuAction,
  ContextMenuItem,
  ContextMenuSection,
  AutocompleteSuggestion,
  ApiOperation,
  ObjectTypeApiConfig,
} from "./foundation/object-model/index.js";

// ── Plugin System ────────────────────────────────────────────────────────────
export {
  ContributionRegistry,
  PluginRegistry,
  pluginId,
} from "./kernel/plugin/index.js";

export type {
  ContributionEntry,
  PluginRegistryEventType,
  PluginRegistryEvent,
  PluginRegistryListener,
  PluginId,
  ViewZone,
  ViewContributionDef,
  CommandContributionDef,
  ContextMenuContributionDef,
  KeybindingContributionDef,
  ActivityBarContributionDef,
  SettingsContributionDef,
  ToolbarContributionDef,
  StatusBarContributionDef,
  WeakRefProviderContributionDef,
  PluginContributions,
  PrismPlugin,
} from "./kernel/plugin/index.js";

// ── Reactive Atoms ───────────────────────────────────────────────────────────
export {
  createPrismBus,
  PrismEvents,
  createAtomStore,
  createObjectAtomStore,
  selectObject,
  selectQuery,
  selectChildren,
  selectEdgesFrom,
  selectEdgesTo,
  selectAllObjects,
  selectAllEdges,
  connectBusToAtoms,
  connectBusToObjectAtoms,
} from "./interaction/atom/index.js";

export type {
  PrismBus,
  EventHandler,
  NavigationTarget,
  AtomState,
  AtomActions,
  AtomStore,
  ObjectAtomState,
  ObjectAtomActions,
  ObjectAtomStore,
} from "./interaction/atom/index.js";

// ── State Machines ───────────────────────────────────────────────────────────
export { Machine, createMachine } from "./kernel/state-machine/index.js";

export type {
  StateNode,
  Transition,
  MachineDefinition,
  MachineListener,
} from "./kernel/state-machine/index.js";

// ── Graph Analysis & Planning ────────────────────────────────────────────────
export {
  buildDependencyGraph,
  buildPredecessorGraph,
  topologicalSort,
  detectCycles,
  findBlockingChain,
  findImpactedObjects,
  computeSlipImpact,
  computePlan,
} from "./domain/graph-analysis/index.js";

export type {
  DependencyGraph,
  SlipImpact,
  PlanNode,
  PlanResult,
} from "./domain/graph-analysis/index.js";

// ── Automation Engine ───────────────────────────────────────────────────────
export {
  evaluateCondition,
  compare,
  getPath,
  interpolate,
  matchesObjectTrigger,
  AutomationEngine,
} from "./kernel/automation/index.js";

export type {
  ObjectTrigger,
  CronTrigger,
  ManualTrigger,
  AutomationTrigger,
  FieldCondition as AutomationFieldCondition,
  TypeCondition as AutomationTypeCondition,
  TagCondition as AutomationTagCondition,
  AndCondition,
  OrCondition,
  NotCondition,
  AutomationCondition,
  CreateObjectAction,
  UpdateObjectAction,
  DeleteObjectAction,
  NotificationAction,
  DelayAction,
  RunAutomationAction,
  AutomationAction,
  Automation,
  AutomationContext,
  AutomationRunStatus,
  ActionResult,
  AutomationRun,
  ObjectEvent,
  ActionHandlerFn,
  ActionHandlerMap,
  AutomationStore,
  AutomationEngineOptions,
} from "./kernel/automation/index.js";

// ── Config System ──────────────────────────────────────────────────────────
export {
  SETTING_SCOPE_ORDER,
  ConfigRegistry,
  ConfigModel,
  validateConfig,
  coerceConfigValue,
  schemaToValidator,
  FeatureFlags,
  MemoryConfigStore,
} from "./kernel/config/index.js";

export type {
  SettingScope,
  SettingType,
  SettingDefinition,
  SettingChange,
  SettingWatcher,
  ChangeListener,
  ConfigStore,
  FeatureFlagContext,
  FeatureFlagCondition,
  FeatureFlagDefinition,
  ConfigSchema,
  StringSchema,
  NumberSchema,
  BooleanSchema,
  ArraySchema,
  ObjectSchema,
  ValidationError,
  ValidationResult,
} from "./kernel/config/index.js";

// ── Undo/Redo ──────────────────────────────────────────────────────────────
export { UndoRedoManager, createUndoBridge } from "./foundation/undo/index.js";

export type {
  ObjectSnapshot,
  UndoEntry,
  UndoApplier,
  UndoListener,
  UndoBridge,
} from "./foundation/undo/index.js";

// ── Server Factory ──────────────────────────────────────────────────────────
export {
  generateRouteSpecs,
  registerRoutes,
  groupByType,
  printRouteTable,
  buildOpenApiDocument,
  generateOpenApiJson,
} from "./network/server/index.js";

export type {
  HttpMethod,
  RouteOperation,
  RouteSpec,
  RouteGenOptions,
  RouteRequest,
  RouteResponse,
  RouteHandler,
  RouteAdapter,
  OpenApiOptions,
} from "./network/server/index.js";

// ── Manifest ─────────────────────────────────────────────────────────────────
export {
  MANIFEST_FILENAME,
  MANIFEST_VERSION,
  defaultManifest,
  parseManifest,
  serialiseManifest,
  validateManifest,
  addCollection,
  removeCollection,
  updateCollection,
  getCollection,
} from "./identity/manifest/index.js";

export {
  createPrivilegeSet,
  getCollectionPermission,
  getFieldPermission,
  getLayoutPermission,
  getScriptPermission,
  canWrite,
  canRead,
  createPrivilegeEnforcer,
} from "./identity/manifest/index.js";

export type {
  StorageBackend,
  LoroStorageConfig,
  MemoryStorageConfig,
  FsStorageConfig,
  StorageConfig,
  SchemaConfig,
  SyncMode,
  SyncConfig,
  CollectionRef,
  ManifestVisibility,
  PrismManifest,
  ManifestValidationError,
  CollectionPermission,
  FieldPermission,
  LayoutPermission,
  ScriptPermission,
  PrivilegeSet,
  PrivilegeSetOptions,
  RoleAssignment,
  PrivilegeEnforcer,
} from "./identity/manifest/index.js";

// ── CRDT Persistence ──────────────────────────────────────────────────────
export {
  createCollectionStore,
  createMemoryAdapter,
  createVaultManager,
} from "./foundation/persistence/index.js";

export type {
  CollectionStore,
  CollectionStoreOptions,
  CollectionChangeType,
  CollectionChange,
  CollectionChangeHandler,
  ObjectFilter,
  PersistenceAdapter,
  VaultManager,
  VaultManagerOptions,
} from "./foundation/persistence/index.js";

// ── Search Engine ──────────────────────────────────────────────────────────
export {
  createSearchIndex,
  tokenize as searchTokenize,
  createSearchEngine,
} from "./interaction/search/index.js";

export type {
  DocRef,
  IndexHit,
  FieldWeights,
  SearchIndexOptions,
  SearchIndex,
  SearchOptions,
  SearchHit,
  SearchFacets,
  SearchResult,
  SearchSubscriber,
  SearchEngineOptions,
  SearchEngine,
} from "./interaction/search/index.js";

// ── Vault Discovery ────────────────────────────────────────────────────────
export {
  createMemoryRosterStore,
  createVaultRoster,
  createMemoryDiscoveryAdapter,
  createVaultDiscovery,
} from "./network/discovery/index.js";

export type {
  RosterEntry,
  RosterSortField,
  RosterSortDir,
  RosterListOptions,
  RosterChangeType,
  RosterChange,
  RosterChangeHandler,
  RosterStore,
  VaultRoster,
  DiscoveryAdapter,
  MemoryDiscoveryAdapter,
  DiscoveredVault,
  DiscoveryEventType,
  DiscoveryEvent,
  DiscoveryEventHandler,
  DiscoveryScanOptions,
  VaultDiscovery,
} from "./network/discovery/index.js";

// ── Derived Views ──────────────────────────────────────────────────────────
export {
  createViewRegistry,
  getFieldValue,
  applyFilters,
  applySorts,
  applyGroups,
  applyViewConfig,
  createLiveView,
  createSavedView,
  createSavedViewRegistry,
} from "./interaction/view/index.js";

export type {
  ViewMode,
  ViewDef,
  ViewRegistry,
  FilterOp,
  FilterConfig,
  SortConfig,
  GroupConfig,
  GroupedResult,
  ViewConfig,
  LiveViewSnapshot,
  LiveViewListener,
  LiveViewOptions,
  LiveView,
  SavedView,
  SavedViewListener,
  SavedViewRegistry,
} from "./interaction/view/index.js";

// ── Notification System ────────────────────────────────────────────────────
export {
  createNotificationStore,
  createNotificationQueue,
} from "./interaction/notification/index.js";

export type {
  NotificationKind,
  Notification,
  NotificationFilter,
  NotificationInput,
  NotificationChangeType,
  NotificationChange,
  NotificationListener,
  NotificationStoreOptions,
  NotificationStore,
  NotificationQueueOptions,
  TimerProvider,
  NotificationQueue,
} from "./interaction/notification/index.js";

// ── Activity Log ──────────────────────────────────────────────────────────
export {
  createActivityStore,
  createActivityTracker,
  formatActivity,
  formatFieldName,
  formatFieldValue,
  groupActivityByDate,
} from "./interaction/activity/index.js";

export type {
  ActivityVerb,
  FieldChange,
  ActivityEvent,
  ActivityEventInput,
  ActivityDescription,
  ActivityGroup,
  ActivityStoreOptions,
  ActivityListener,
  ActivityStore,
  TrackableStore,
  ActivityTrackerOptions,
  ActivityTracker,
} from "./interaction/activity/index.js";

// ── Batch Operations ──────────────────────────────────────────────────────
export { createBatchTransaction } from "./foundation/batch/index.js";

export type {
  BatchTransaction,
  BatchTransactionOptions,
  BatchOp,
  CreateObjectOp,
  UpdateObjectOp,
  DeleteObjectOp,
  MoveObjectOp,
  CreateEdgeOp,
  UpdateEdgeOp,
  DeleteEdgeOp,
  BatchResult,
  BatchProgress,
  BatchProgressCallback,
  BatchValidationError,
  BatchValidationResult,
  BatchExecuteOptions,
} from "./foundation/batch/index.js";

// ── Clipboard ─────────────────────────────────────────────────────────────
export { createTreeClipboard } from "./foundation/clipboard/index.js";

export type {
  TreeClipboard,
  TreeClipboardOptions,
  SerializedSubtree,
  ClipboardEntry,
  ClipboardMode,
  PasteOptions,
  PasteResult,
} from "./foundation/clipboard/index.js";

// ── Template System ───────────────────────────────────────────────────────
export { createTemplateRegistry } from "./foundation/template/index.js";

export type {
  TemplateRegistry,
  TemplateRegistryOptions,
  TemplateVariable,
  TemplateNode,
  TemplateEdge,
  ObjectTemplate,
  TemplateFilter,
  InstantiateOptions,
  InstantiateResult,
} from "./foundation/template/index.js";

// ── Ephemeral Presence ───────────────────────────────────────────────────
export { createPresenceManager } from "./network/presence/index.js";

export type {
  PresenceManager,
  CursorPosition,
  SelectionRange,
  PeerIdentity,
  PresenceState,
  PresenceChangeType,
  PresenceChange,
  PresenceListener,
  PresenceManagerOptions,
  TimerProvider as PresenceTimerProvider,
} from "./network/presence/index.js";

// ── Identity ────────────────────────────────────────────────────────────────
export {
  createIdentity,
  resolveIdentity,
  signPayload,
  verifySignature,
  createMultiSigConfig,
  createPartialSignature,
  assembleMultiSignature,
  verifyMultiSignature,
  encodeBase58,
  decodeBase58,
  publicKeyToDidKey,
  didKeyToPublicKey,
  base64urlEncode,
} from "./identity/did/index.js";

export type {
  DIDMethod,
  DID,
  Ed25519KeyPair,
  KeyHandle,
  VerificationMethod,
  DIDDocument,
  PrismIdentity,
  ResolvedIdentity,
  PartialSignature,
  MultiSignature,
  MultiSigConfig,
  CreateIdentityOptions,
  ResolveIdentityOptions,
} from "./identity/did/index.js";

// ── Encryption ──────────────────────────────────────────────────────────────
export {
  createMemoryKeyStore,
  createVaultKeyManager,
  encryptSnapshot,
  decryptSnapshot,
} from "./identity/encryption/index.js";

export type {
  VaultKeyInfo,
  EncryptedSnapshot,
  KeyStore,
  VaultKeyManager,
  VaultKeyManagerOptions,
} from "./identity/encryption/index.js";

// ── Virtual File System ─────────────────────────────────────────────────────
export {
  createMemoryVfsAdapter,
  createVfsManager,
  computeBinaryHash,
} from "./foundation/vfs/index.js";

export type {
  BinaryRef,
  FileStat,
  BinaryLock,
  VfsAdapter,
  VfsManager,
  VfsManagerOptions,
} from "./foundation/vfs/index.js";

// ── Relay ───────────────────────────────────────────────────────────────────
export {
  RELAY_CAPABILITIES,
  createRelayBuilder,
  blindMailboxModule,
  relayRouterModule,
  relayTimestampModule,
  blindPingModule,
  capabilityTokenModule,
  webhookModule,
  sovereignPortalModule,
  createMemoryPingTransport,
  webrtcSignalingModule,
} from "./network/relay/index.js";

export type {
  WebhookHttpClient,
  RelayEnvelope,
  BlindMailbox,
  RelayRouter,
  RouteResult,
  RelayTimestamper,
  TimestampReceipt,
  BlindPinger,
  BlindPing,
  PingTransport,
  CapabilityToken,
  CapabilityTokenManager,
  WebhookConfig,
  WebhookPayload,
  WebhookDelivery,
  WebhookEmitter,
  PortalLevel,
  PortalManifest,
  PortalRegistry,
  RelayModule,
  RelayContext,
  RelayConfig,
  RelayInstance,
  RelayBuilder,
  RelayBuilderOptions,
  SignalType,
  SignalMessage,
  SignalingPeer,
  SignalingRoom,
  SignalDelivery,
  SignalingHub,
} from "./network/relay/index.js";

// ── Actor System ───────────────────────────────────────────────────────────
export {
  DEFAULT_CAPABILITY_SCOPE,
  createProcessQueue,
  createLuauActorRuntime,
  createSidecarRuntime,
  createTestRuntime,
} from "./kernel/actor/index.js";

export type {
  ExecutionTarget,
  CapabilityScope,
  TaskStatus,
  ProcessTask,
  RuntimeResult,
  ActorRuntime,
  QueueEventType,
  QueueEvent,
  QueueListener,
  ProcessQueue,
  ProcessQueueOptions,
  LuauPayload,
  SidecarPayload,
  SidecarExecutor,
} from "./kernel/actor/index.js";

// ── Intelligence Layer ─────────────────────────────────────────────────────
export {
  createAiProviderRegistry,
  createOllamaProvider,
  createExternalProvider,
  createContextBuilder,
  createTestAiProvider,
} from "./kernel/actor/index.js";

// ── Syntax Engine ──────────────────────────────────────────────────────────
export {
  FIELD_TYPE_MAP,
  BUILTIN_FUNCTIONS,
  inferNodeType,
  createExpressionProvider,
  generateLuauTypeDef,
  createSyntaxEngine,
} from "./language/syntax/index.js";

export type {
  DiagnosticSeverity,
  TextRange,
  Diagnostic,
  CompletionKind,
  CompletionItem,
  HoverInfo,
  FieldTypeMapping,
  TypeInfo,
  SchemaContext,
  LuauTypeDef,
  FunctionSignature,
  SyntaxProvider,
  SyntaxEngineOptions,
  SyntaxEngine,
} from "./language/syntax/index.js";

export type {
  AiRole,
  AiMessage,
  AiCompletionRequest,
  AiCompletion,
  InlineCompletionRequest,
  InlineCompletion,
  ObjectContext,
  AiProvider,
  AiProviderRegistry,
  OllamaProviderOptions,
  ExternalProviderOptions,
  ContextBuilderOptions,
  AiHttpClient,
} from "./kernel/actor/index.js";

// ── Communication Fabric ───────────────────────────────────────────────────
export {
  createTranscriptTimeline,
  createPlaybackController,
  createTestTransport,
  createTestTranscriptionProvider,
  createSessionManager,
} from "./network/session/index.js";

export type {
  TestTranscriptionProvider,
} from "./network/session/index.js";

export type {
  SessionStatus,
  ParticipantRole,
  MediaKind,
  SessionParticipant,
  TranscriptSegment,
  TranscriptTimeline,
  TrackState,
  MediaTrack,
  PlaybackController,
  PlaybackListener,
  TransportKind,
  SessionTransport,
  TransportEventType,
  TransportEvent,
  TransportEventListener,
  TranscriptionProvider,
  TranscriptionOptions,
  DelegationStatus,
  DelegationRequest,
  DelegationListener,
  SessionChangeType,
  SessionChangeListener,
  SessionManagerOptions,
  SessionConfig,
  SessionManager,
} from "./network/session/index.js";

// ── Trust & Safety ─────────────────────────────────────────────────────────
export {
  createLuauSandbox,
  createSchemaValidator,
  createHashcashMinter,
  createHashcashVerifier,
  createPeerTrustGraph,
  createShamirSplitter,
  createEscrowManager,
  createPasswordAuthManager,
} from "./identity/trust/index.js";

// ── NLE / Timeline ────────────────────────────────────────────────────────
export {
  createTimelineEngine,
  createManualClock,
  createTempoMap,
  resetIdCounter,
} from "./domain/timeline/index.js";

export type {
  TimeSeconds,
  TimeRange,
  PPQ,
  TempoMarker,
  TimeSignature,
  MusicalPosition,
  TempoMap,
  TrackKind,
  TimelineClip,
  InterpolationMode,
  AutomationPoint,
  AutomationLane,
  TimelineTrack,
  TransportStatus,
  LoopRegion,
  TransportState,
  TimelineClock,
  TimelineMarker,
  TimelineEventKind,
  TimelineEvent,
  TimelineListener,
  TimelineEngine,
  ManualClock,
  TimelineEngineOptions,
} from "./domain/timeline/index.js";

export type {
  SandboxCapability,
  SandboxPolicy,
  SandboxViolation,
  LuauSandbox,
  SchemaValidationSeverity,
  SchemaValidationIssue,
  SchemaValidationResult,
  SchemaValidationRule,
  SchemaValidator,
  SchemaValidatorOptions,
  HashcashChallenge,
  HashcashProof,
  HashcashMinter,
  HashcashVerifier,
  TrustLevel,
  PeerReputation,
  ContentHash,
  TrustGraphEvent,
  TrustGraphListener,
  PeerTrustGraph,
  TrustGraphOptions,
  ShamirShare,
  ShamirConfig,
  ShamirSplitter,
  EscrowDeposit,
  EscrowManager,
  PasswordAuthRecord,
  PasswordAuthResult,
  PasswordAuthManager,
  PasswordAuthManagerOptions,
} from "./identity/trust/index.js";

// ── Facet System ──────────────────────────────────────────────────────────
export {
  detectFormat,
  parseValues,
  serializeValues,
  inferFields,
  createFacetDefinition,
  FacetDefinitionBuilder,
  facetDefinitionBuilder,
  SpellCheckRegistry,
  SpellChecker,
  extractWords,
  PersonalDictionary,
  MemoryDictionaryStorage,
  SpellCheckerBuilder,
  spellCheckerBuilder,
  createUrlDictionaryProvider,
  createStaticDictionaryProvider,
  createLazyDictionaryProvider,
  URL_FILTER,
  EMAIL_FILTER,
  ALL_CAPS_FILTER,
  CAMEL_CASE_FILTER,
  ALPHANUMERIC_FILTER,
  FILE_PATH_FILTER,
  INLINE_CODE_FILTER,
  SYNTAX_CODE_FILTER,
  WIKI_LINK_FILTER,
  SINGLE_CHAR_FILTER,
  createDelimiterFilter,
  createSyntaxFilter,
  MockSpellCheckBackend,
  markdownToNodes,
  nodesToMarkdown,
  emitConditionLuau,
  emitScriptLuau,
  TypeScriptWriter,
  JavaScriptWriter,
  CSharpWriter,
  LuauWriter,
  JsonWriter,
  YamlWriter,
  TomlWriter,
  luauBrowserView,
  luauCollectionRule,
  luauStatsCommand,
  luauMenuItem,
  luauCommand,
  createStaticValueList,
  createDynamicValueList,
  resolveValueList,
  createValueListRegistry,
  createPrintConfig,
  evaluateConditionalFormats,
  computeFieldStyle,
  interpolateMergeFields,
  renderTextSlot,
  createCollectionValueListResolver,
  getValueListId,
  getBoundFields,
  STEP_KINDS,
  getStepMeta,
  createStep,
  createVisualScript,
  emitStepsLuau,
  emitStepsLuauWithMap,
  validateSteps,
  getStepCategories,
  createFacetStore,
  computePartBands,
  snapToGrid,
  alignSlots,
  distributeSlots,
  detectOverlaps,
  slotHitTest,
  partForY,
  clampToBand,
  sortByZIndex,
} from "./language/facet/index.js";

export type {
  SourceFormat,
  FacetLayout,
  FacetLayoutMode,
  LayoutPartKind,
  LayoutPart,
  SpatialRect,
  ConditionalFormat,
  FieldSlot,
  PortalSlot,
  TextSlot,
  DrawingShape,
  DrawingSlot,
  ContainerSlot,
  FacetSlot,
  SummaryField,
  PageOrientation,
  PageSize,
  PageMargins,
  PrintConfig,
  FacetDefinition,
  ComputedStyle,
  ValueListDataSource,
  ComputedBand,
  Alignment,
  FacetStoreListener,
  FacetStoreSnapshot,
  FacetStore,
  DictionaryData,
  DictionaryProvider,
  PersonalDictionaryStorage,
  ExtractedWord,
  SpellCheckBackend,
  SpellCheckerConfig,
  SpellCheckEvent,
  SpellCheckEventListener,
  MockSpellCheckConfig,
  ProseNode,
  ProseMark,
  SequencerSubjectKind,
  SequencerSubject,
  SequencerOperator,
  SequencerCombinator,
  SequencerConditionClause,
  SequencerConditionState,
  SequencerActionKind,
  SequencerScriptStep,
  SequencerScriptState,
  ScriptStepKind,
  StepKindMeta,
  ScriptStep,
  VisualScript,
  StepsLuauEmitResult,
  ValueListItem,
  StaticValueListSource,
  DynamicValueListSource,
  ValueListSource,
  ValueListDisplay,
  ValueList,
  ValueListResolver,
  ValueListListener,
  ValueListRegistry,
  SchemaField,
  SchemaInterface,
  SchemaEnum,
  SchemaDeclaration,
  SchemaModel,
  BrowserViewConfig,
  CollectionRuleConfig,
  StatsFieldConfig,
  StatsCommandConfig,
  MenuItemConfig,
  CommandConfig,
} from "./language/facet/index.js";

// ── Flux Domain ───────────────────────────────────────────────────────────
export {
  FLUX_CATEGORIES,
  FLUX_TYPES,
  FLUX_EDGES,
  TASK_STATUSES,
  PROJECT_STATUSES,
  GOAL_STATUSES,
  TRANSACTION_TYPES,
  CONTACT_TYPES,
  INVOICE_STATUSES,
  ITEM_STATUSES,
  createFluxRegistry,
} from "./domain/flux/index.js";

export type {
  FluxCategory,
  FluxEntityType,
  FluxEdgeType,
  FluxAutomationPreset,
  FluxTriggerKind,
  FluxAutomationAction,
  FluxExportFormat,
  FluxExportOptions,
  FluxImportResult,
  FluxRegistry,
} from "./domain/flux/index.js";

// ── Plugin Bundles ───────────────────────────────────────────────────────
export * from "./kernel/plugin-bundles/index.js";

// ── Self-Replicating Builder ─────────────────────────────────────────────
export * from "./kernel/builder/index.js";
