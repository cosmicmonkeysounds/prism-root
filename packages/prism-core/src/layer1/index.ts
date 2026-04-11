export { createLoroBridge } from "./loro-bridge.js";
export type { LoroBridge, LoroChangeHandler } from "./loro-bridge.js";
export { createCrdtStore } from "./stores/use-crdt-store.js";
export type { CrdtStore, CrdtStoreState, CrdtStoreActions } from "./stores/use-crdt-store.js";
export { executeLuau, createLuauEngine } from "./luau/luau-runtime.js";
export type { LuauEngine } from "./luau/luau-runtime.js";
export { createLuauDebugger } from "./luau/luau-debugger.js";
export type { LuauDebugger, TraceFrame, DebugRunResult } from "./luau/luau-debugger.js";

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
} from "./object-model/index.js";

// ── Lens System & Shell ───────────────────────────────────────────────────────
export {
  lensId,
  tabId,
  createLensRegistry,
  createShellStore,
} from "./lens/index.js";

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
} from "./lens/index.js";

// ── Input System ──────────────────────────────────────────────────────────────
export { parseShortcut, normaliseKeyEvent, keyToShortcut, KeyboardModel } from "./input/index.js";
export { InputScope } from "./input/index.js";
export { InputRouter } from "./input/index.js";

export type {
  NormalisedKey,
  KeyEventLike,
  UndoHook,
  InputRouterEvent,
  InputRouterListener,
} from "./input/index.js";

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
} from "./forms/index.js";

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
} from "./forms/index.js";

// ── Layout System ─────────────────────────────────────────────────────────────
export { SelectionModel } from "./layout/index.js";
export { PageModel } from "./layout/index.js";
export { PageRegistry } from "./layout/index.js";
export { LensSlot } from "./layout/index.js";
export { LensManager } from "./layout/index.js";

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
} from "./layout/index.js";

// ── Expression Engine ─────────────────────────────────────────────────────────
export { tokenize, isDigit, isIdentStart, isIdentChar } from "./expression/index.js";
export { parse } from "./expression/index.js";
export { evaluate, evaluateExpression } from "./expression/index.js";

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
} from "./expression/index.js";

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
} from "./object-model/index.js";

// ── Plugin System ────────────────────────────────────────────────────────────
export {
  ContributionRegistry,
  PluginRegistry,
  pluginId,
} from "./plugin/index.js";

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
} from "./plugin/index.js";

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
} from "./atom/index.js";

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
} from "./atom/index.js";

// ── State Machines ───────────────────────────────────────────────────────────
export { Machine, createMachine } from "./automaton/index.js";

export type {
  StateNode,
  Transition,
  MachineDefinition,
  MachineListener,
} from "./automaton/index.js";

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
} from "./graph-analysis/index.js";

export type {
  DependencyGraph,
  SlipImpact,
  PlanNode,
  PlanResult,
} from "./graph-analysis/index.js";

// ── Automation Engine ───────────────────────────────────────────────────────
export {
  evaluateCondition,
  compare,
  getPath,
  interpolate,
  matchesObjectTrigger,
  AutomationEngine,
} from "./automation/index.js";

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
} from "./automation/index.js";

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
} from "./config/index.js";

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
} from "./config/index.js";

// ── Undo/Redo ──────────────────────────────────────────────────────────────
export { UndoRedoManager, createUndoBridge } from "./undo/index.js";

export type {
  ObjectSnapshot,
  UndoEntry,
  UndoApplier,
  UndoListener,
  UndoBridge,
} from "./undo/index.js";

// ── Server Factory ──────────────────────────────────────────────────────────
export {
  generateRouteSpecs,
  registerRoutes,
  groupByType,
  printRouteTable,
  buildOpenApiDocument,
  generateOpenApiJson,
} from "./server/index.js";

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
} from "./server/index.js";

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
} from "./manifest/index.js";

export {
  createPrivilegeSet,
  getCollectionPermission,
  getFieldPermission,
  getLayoutPermission,
  getScriptPermission,
  canWrite,
  canRead,
  createPrivilegeEnforcer,
} from "./manifest/index.js";

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
} from "./manifest/index.js";

// ── CRDT Persistence ──────────────────────────────────────────────────────
export {
  createCollectionStore,
  createMemoryAdapter,
  createVaultManager,
} from "./persistence/index.js";

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
} from "./persistence/index.js";

// ── Search Engine ──────────────────────────────────────────────────────────
export {
  createSearchIndex,
  tokenize as searchTokenize,
  createSearchEngine,
} from "./search/index.js";

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
} from "./search/index.js";

// ── Vault Discovery ────────────────────────────────────────────────────────
export {
  createMemoryRosterStore,
  createVaultRoster,
  createMemoryDiscoveryAdapter,
  createVaultDiscovery,
} from "./discovery/index.js";

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
} from "./discovery/index.js";

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
} from "./view/index.js";

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
} from "./view/index.js";

// ── Notification System ────────────────────────────────────────────────────
export {
  createNotificationStore,
  createNotificationQueue,
} from "./notification/index.js";

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
} from "./notification/index.js";

// ── Activity Log ──────────────────────────────────────────────────────────
export {
  createActivityStore,
  createActivityTracker,
  formatActivity,
  formatFieldName,
  formatFieldValue,
  groupActivityByDate,
} from "./activity/index.js";

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
} from "./activity/index.js";

// ── Batch Operations ──────────────────────────────────────────────────────
export { createBatchTransaction } from "./batch/index.js";

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
} from "./batch/index.js";

// ── Clipboard ─────────────────────────────────────────────────────────────
export { createTreeClipboard } from "./clipboard/index.js";

export type {
  TreeClipboard,
  TreeClipboardOptions,
  SerializedSubtree,
  ClipboardEntry,
  ClipboardMode,
  PasteOptions,
  PasteResult,
} from "./clipboard/index.js";

// ── Template System ───────────────────────────────────────────────────────
export { createTemplateRegistry } from "./template/index.js";

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
} from "./template/index.js";

// ── Ephemeral Presence ───────────────────────────────────────────────────
export { createPresenceManager } from "./presence/index.js";

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
} from "./presence/index.js";

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
} from "./identity/index.js";

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
} from "./identity/index.js";

// ── Encryption ──────────────────────────────────────────────────────────────
export {
  createMemoryKeyStore,
  createVaultKeyManager,
  encryptSnapshot,
  decryptSnapshot,
} from "./encryption/index.js";

export type {
  VaultKeyInfo,
  EncryptedSnapshot,
  KeyStore,
  VaultKeyManager,
  VaultKeyManagerOptions,
} from "./encryption/index.js";

// ── Virtual File System ─────────────────────────────────────────────────────
export {
  createMemoryVfsAdapter,
  createVfsManager,
  computeBinaryHash,
} from "./vfs/index.js";

export type {
  BinaryRef,
  FileStat,
  BinaryLock,
  VfsAdapter,
  VfsManager,
  VfsManagerOptions,
} from "./vfs/index.js";

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
} from "./relay/index.js";

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
} from "./relay/index.js";

// ── Actor System ───────────────────────────────────────────────────────────
export {
  DEFAULT_CAPABILITY_SCOPE,
  createProcessQueue,
  createLuauActorRuntime,
  createSidecarRuntime,
  createTestRuntime,
} from "./actor/index.js";

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
} from "./actor/index.js";

// ── Intelligence Layer ─────────────────────────────────────────────────────
export {
  createAiProviderRegistry,
  createOllamaProvider,
  createExternalProvider,
  createContextBuilder,
  createTestAiProvider,
} from "./actor/index.js";

// ── Syntax Engine ──────────────────────────────────────────────────────────
export {
  FIELD_TYPE_MAP,
  BUILTIN_FUNCTIONS,
  inferNodeType,
  createExpressionProvider,
  generateLuauTypeDef,
  createSyntaxEngine,
} from "./syntax/index.js";

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
} from "./syntax/index.js";

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
} from "./actor/index.js";

// ── Communication Fabric ───────────────────────────────────────────────────
export {
  createTranscriptTimeline,
  createPlaybackController,
  createTestTransport,
  createTestTranscriptionProvider,
  createSessionManager,
} from "./session/index.js";

export type {
  TestTranscriptionProvider,
} from "./session/index.js";

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
} from "./session/index.js";

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
} from "./trust/index.js";

// ── NLE / Timeline ────────────────────────────────────────────────────────
export {
  createTimelineEngine,
  createManualClock,
  createTempoMap,
  resetIdCounter,
} from "./timeline/index.js";

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
} from "./timeline/index.js";

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
} from "./trust/index.js";

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
} from "./facet/index.js";

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
} from "./facet/index.js";

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
} from "./flux/index.js";

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
} from "./flux/index.js";

// ── Plugin Bundles ───────────────────────────────────────────────────────
export * from "./plugins/index.js";

// ── Self-Replicating Builder ─────────────────────────────────────────────
export * from "./builder/index.js";
