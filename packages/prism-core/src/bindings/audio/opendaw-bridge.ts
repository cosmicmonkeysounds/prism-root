/**
 * @prism/core — OpenDAW Audio Bridge (Layer 2)
 *
 * Bridges Prism's Layer 1 Timeline to OpenDAW's audio engine.
 * OpenDAW handles all DSP, mixing, effects, and playback.
 * Prism timeline provides the state model and transport abstraction.
 */

import { UUID, Progress, Option } from "@opendaw/lib-std";
import { PPQN } from "@opendaw/lib-dsp";
import {
  Project,
  Workers,
  AudioWorklets,
  GlobalSampleLoaderManager,
  GlobalSoundfontLoaderManager,
  SampleService,
  OfflineEngineRenderer,
  AudioOfflineRenderer,
} from "@opendaw/studio-core";
import type { SampleProvider, SoundfontProvider, SoundfontService } from "@opendaw/studio-core";
import { AnimationFrame } from "@opendaw/lib-dom";
import { AudioData, WavFile } from "@opendaw/lib-dsp";
import { AudioFileBox, AudioRegionBox, ValueEventCollectionBox, AudioUnitBox, TrackBox } from "@opendaw/studio-boxes";
import type { EffectFactory } from "@opendaw/studio-core";
import type { InstrumentFactory, SampleMetaData, SoundfontMetaData, ExportStemsConfiguration } from "@opendaw/studio-adapters";

import type { TimelineEngine, TrackKind } from "@prism/core/timeline";
import type {
  OpenDawBridge,
  OpenDawBridgeOptions,
  OpenDawEffectType,
  TrackBinding,
  ClipBinding,
  AudioExportOptions,
  StemConfig,
} from "./opendaw-types.js";

// ── AudioBuffer → AudioData conversion ────────────────────────────────────

function audioBufferToAudioData(audioBuffer: AudioBuffer): AudioData {
  const { numberOfChannels, length: numberOfFrames, sampleRate } = audioBuffer;
  const audioData = AudioData.create(sampleRate, numberOfFrames, numberOfChannels);
  for (let channel = 0; channel < numberOfChannels; channel++) {
    const frame = audioData.frames[channel];
    if (frame) frame.set(audioBuffer.getChannelData(channel));
  }
  return audioData;
}

// ── Factory Types ─────────────────────────────────────────────────────────
// Effect and instrument factories are injected via OpenDawBridgeOptions
// because the concrete factory instances live in downstream app code,
// not in the core library packages.

export type { EffectFactory } from "@opendaw/studio-core";
export type { InstrumentFactory } from "@opendaw/studio-adapters";

// ── Worker URLs (Vite-style imports) ──────────────────────────────────────
// These are resolved at build time by Vite. For non-Vite builds,
// the consumer must call installWorkers() before creating the bridge.

let workersInstalled = false;

/**
 * Install OpenDAW workers and worklets from URL strings.
 * Must be called once before creating an OpenDAW bridge.
 * In a Vite project, use:
 *   import WorkersUrl from "@opendaw/studio-core/workers-main.js?worker&url";
 *   import WorkletsUrl from "@opendaw/studio-core/processors.js?url";
 *   import OfflineUrl from "@opendaw/studio-core/offline-engine.js?worker&url";
 */
export async function installOpenDawWorkers(
  workersUrl: string,
  workletsUrl: string,
  offlineEngineUrl: string,
): Promise<void> {
  await Workers.install(workersUrl);
  AudioWorklets.install(workletsUrl);
  OfflineEngineRenderer.install(offlineEngineUrl);
  workersInstalled = true;
}

// ── Bridge Factory ────────────────────────────────────────────────────────

/**
 * Create an OpenDAW audio bridge connected to a Prism TimelineEngine.
 *
 * Prerequisites:
 *   - Call `installOpenDawWorkers()` once before creating the bridge
 *   - Browser must support SharedArrayBuffer (cross-origin isolation required)
 *   - AnimationFrame must be started (`AnimationFrame.start(window)`)
 *
 * @example
 * ```typescript
 * import WorkersUrl from "@opendaw/studio-core/workers-main.js?worker&url";
 * import WorkletsUrl from "@opendaw/studio-core/processors.js?url";
 * import OfflineUrl from "@opendaw/studio-core/offline-engine.js?worker&url";
 *
 * // One-time setup
 * AnimationFrame.start(window);
 * await installOpenDawWorkers(WorkersUrl, WorkletsUrl, OfflineUrl);
 *
 * // Create bridge
 * const timeline = createTimelineEngine();
 * const bridge = await createOpenDawBridge(timeline, { bpm: 120 });
 *
 * // Load audio
 * const binding = await bridge.loadTrack("Vocals", audioBuffer);
 *
 * // Play (syncs Prism transport → OpenDAW engine)
 * bridge.startSync();
 * timeline.play();
 * ```
 */
export async function createOpenDawBridge(
  timeline: TimelineEngine,
  options: OpenDawBridgeOptions = {},
): Promise<OpenDawBridge> {
  if (!workersInstalled) {
    throw new Error(
      "OpenDAW workers not installed. Call installOpenDawWorkers() before creating a bridge."
    );
  }

  const {
    bpm = timeline.getTempoMap().tempoAt(0),
    onStatusUpdate,
    localAudioBuffers = new Map<string, AudioBuffer>(),
    instrumentFactory,
    effectFactories = {},
  } = options;

  onStatusUpdate?.("Creating AudioContext...");

  // Create AudioContext
  const audioContext = new AudioContext({ latencyHint: 0 });

  // Install audio worklets
  onStatusUpdate?.("Installing audio worklets...");
  await AudioWorklets.createFor(audioContext);

  // Sample provider backed by localAudioBuffers map
  const sampleProvider: SampleProvider = {
    fetch: async (uuid: UUID.Bytes, _progress: Progress.Handler): Promise<[AudioData, SampleMetaData]> => {
      const uuidString = UUID.toString(uuid);
      const audioBuffer = localAudioBuffers.get(uuidString);
      if (!audioBuffer) {
        throw new Error(`Sample not found: ${uuidString}`);
      }
      const audioData = audioBufferToAudioData(audioBuffer);
      const metadata: SampleMetaData = {
        name: uuidString,
        bpm,
        duration: audioBuffer.duration,
        sample_rate: audioBuffer.sampleRate,
        origin: "import",
      };
      return [audioData, metadata];
    },
  };

  const sampleManager = new GlobalSampleLoaderManager(sampleProvider);

  // Soundfont manager (stub — not used in Prism demos)
  const soundfontProvider: SoundfontProvider = {
    fetch: async (_uuid: UUID.Bytes, _progress: Progress.Handler): Promise<[ArrayBuffer, SoundfontMetaData]> => {
      throw new Error("Soundfonts not available in Prism bridge");
    },
  };
  const soundfontManager = new GlobalSoundfontLoaderManager(soundfontProvider);

  // Soundfont service proxy (not used)
  const soundfontService = new Proxy({} as SoundfontService, {
    get(_target, prop) {
      throw new Error(`SoundfontService.${String(prop)} not available in Prism bridge`);
    },
  });

  onStatusUpdate?.("Creating project...");

  const sampleService = new SampleService(audioContext);
  const audioWorklets = AudioWorklets.get(audioContext);

  const project = Project.new({
    audioContext,
    sampleManager,
    soundfontManager,
    audioWorklets,
    sampleService,
    soundfontService,
  });

  // Set BPM
  if (bpm !== 120) {
    project.editing.modify(() => {
      project.timelineBox.bpm.setValue(bpm);
    });
  }

  // Start audio worklet and wait for engine
  project.startAudioWorklet();
  await project.engine.isReady();

  onStatusUpdate?.("Engine ready.");

  // ── State ─────────────────────────────────────────────

  const trackBindings: TrackBinding[] = [];
  const clipBindings: ClipBinding[] = [];
  let syncTerminator: (() => void) | null = null;
  let transportUnsubscribe: (() => void) | null = null;

  // ── Transport Sync ────────────────────────────────────

  function syncTransportToOpenDaw(): void {
    const transport = timeline.getTransport();
    const currentBpm = project.timelineBox.bpm.getValue();

    switch (transport.status) {
      case "playing":
        if (!project.engine.isPlaying.getValue()) {
          const ppqnPos = PPQN.secondsToPulses(transport.position, currentBpm);
          project.engine.setPosition(ppqnPos);
          project.engine.play();
        }
        break;
      case "paused":
      case "stopped":
        if (project.engine.isPlaying.getValue()) {
          project.engine.stop(transport.status === "stopped");
        }
        if (transport.status === "stopped") {
          project.engine.setPosition(0);
        }
        break;
    }

    // Sync loop region
    if (transport.loop.enabled) {
      project.editing.modify(() => {
        project.timelineBox.loopArea.enabled.setValue(true);
        project.timelineBox.loopArea.from.setValue(
          PPQN.secondsToPulses(transport.loop.start, currentBpm)
        );
        project.timelineBox.loopArea.to.setValue(
          PPQN.secondsToPulses(transport.loop.end, currentBpm)
        );
      });
    }
  }

  function syncOpenDawToTimeline(): void {
    if (!project.engine.isPlaying.getValue()) return;
    const ppqnPos = project.engine.position.getValue();
    const currentBpm = project.timelineBox.bpm.getValue();
    const seconds = PPQN.pulsesToSeconds(ppqnPos, currentBpm);
    timeline.scrub(seconds);
  }

  // ── Bridge Implementation ─────────────────────────────

  const bridge: OpenDawBridge = {
    get project() { return project; },
    get audioContext() { return audioContext; },
    get timeline() { return timeline; },

    async loadTrack(name: string, audioBuffer: AudioBuffer, kind: TrackKind = "audio"): Promise<TrackBinding> {
      const fileUUID = UUID.generate();
      const uuidString = UUID.toString(fileUUID);

      // Store buffer for sample provider
      localAudioBuffers.set(uuidString, audioBuffer);

      const currentBpm = project.timelineBox.bpm.getValue();

      // Create Prism timeline track
      const prismTrack = timeline.addTrack(kind, name);

      // Create clip spanning full duration
      const prismClip = timeline.addClip(prismTrack.id, {
        name,
        startTime: 0,
        duration: audioBuffer.duration,
        sourceOffset: 0,
        sourceRef: uuidString,
        muted: false,
        locked: false,
        gain: 1.0,
      });

      // Create OpenDAW track
      let trackBox: TrackBox | undefined;
      let audioUnitBox: AudioUnitBox | undefined;
      let regionBox: AudioRegionBox | undefined;

      if (!instrumentFactory) {
        throw new Error("instrumentFactory must be provided in OpenDawBridgeOptions to load tracks");
      }

      project.editing.modify(() => {
        const result = project.api.createInstrument(instrumentFactory as InstrumentFactory<unknown, never>);
        const tb = result.trackBox;
        const aub = result.audioUnitBox;
        trackBox = tb;
        audioUnitBox = aub;

        const audioFileBox = AudioFileBox.create(project.boxGraph, fileUUID, box => {
          box.fileName.setValue(name);
          box.endInSeconds.setValue(audioBuffer.duration);
        });

        const clipDurationPPQN = PPQN.secondsToPulses(audioBuffer.duration, currentBpm);
        const eventsBox = ValueEventCollectionBox.create(project.boxGraph, UUID.generate());

        regionBox = AudioRegionBox.create(project.boxGraph, UUID.generate(), box => {
          box.regions.refer(tb.regions);
          box.file.refer(audioFileBox);
          box.events.refer(eventsBox.owners);
          box.position.setValue(0);
          box.duration.setValue(clipDurationPPQN);
          box.loopOffset.setValue(0);
          box.loopDuration.setValue(clipDurationPPQN);
          box.label.setValue(name);
          box.mute.setValue(false);
        });
      });

      if (!trackBox || !audioUnitBox || !regionBox) {
        throw new Error("Failed to create OpenDAW track");
      }

      const binding: TrackBinding = {
        trackId: prismTrack.id,
        trackBox,
        audioUnitBox,
        uuid: fileUUID,
        name,
      };

      trackBindings.push(binding);

      clipBindings.push({
        clipId: prismClip.id,
        trackId: prismTrack.id,
        regionBox,
        fileUuid: fileUUID,
      });

      // Wait for sample to load
      await project.engine.queryLoadingComplete();

      return binding;
    },

    getTrackBindings(): TrackBinding[] {
      return [...trackBindings];
    },

    getTrackBinding(trackId: string): TrackBinding | undefined {
      return trackBindings.find(b => b.trackId === trackId);
    },

    startSync(): void {
      if (syncTerminator) return;

      // Subscribe to Prism transport events → push to OpenDAW
      transportUnsubscribe = timeline.subscribe(event => {
        if (event.kind.startsWith("transport:")) {
          syncTransportToOpenDaw();
        }
      });

      // AnimationFrame loop: OpenDAW position → Prism timeline
      const sub = AnimationFrame.add(() => {
        syncOpenDawToTimeline();
      });

      syncTerminator = () => {
        sub.terminate();
        if (transportUnsubscribe) {
          transportUnsubscribe();
          transportUnsubscribe = null;
        }
      };
    },

    stopSync(): void {
      if (syncTerminator) {
        syncTerminator();
        syncTerminator = null;
      }
    },

    insertEffect(trackId: string, effectType: OpenDawEffectType): void {
      const binding = trackBindings.find(b => b.trackId === trackId);
      if (!binding) throw new Error(`Track binding not found: ${trackId}`);

      const factory = effectFactories[effectType];
      if (!factory) throw new Error(`Unknown or unconfigured effect type: ${effectType}. Provide it in OpenDawBridgeOptions.effectFactories.`);

      project.editing.modify(() => {
        project.api.insertEffect(binding.audioUnitBox.audioEffects, factory as EffectFactory);
      });
    },

    async exportMix(options: AudioExportOptions = {}): Promise<ArrayBuffer> {
      const { sampleRate = 48000, onProgress, onStatus, abortSignal } = options;

      onStatus?.("Rendering mix...");
      const progressHandler: Progress.Handler = (value: number) => {
        onProgress?.(value);
      };

      const audioBuffer = await AudioOfflineRenderer.start(
        project,
        Option.None,
        progressHandler,
        abortSignal,
        sampleRate,
      );

      onStatus?.("Encoding WAV...");
      return WavFile.encodeFloats(audioBuffer);
    },

    async exportStems(
      stemsConfig: Record<string, StemConfig>,
      options: AudioExportOptions = {},
    ): Promise<Map<string, ArrayBuffer>> {
      const { sampleRate = 48000, onProgress, onStatus, abortSignal } = options;

      // Map our StemConfig to OpenDAW's ExportStemsConfiguration (add useInstrumentOutput)
      const exportConfig: ExportStemsConfiguration = {};
      for (const [key, stem] of Object.entries(stemsConfig)) {
        exportConfig[key] = {
          includeAudioEffects: stem.includeAudioEffects,
          includeSends: stem.includeSends,
          useInstrumentOutput: false,
          fileName: stem.fileName,
        };
      }

      onStatus?.("Rendering stems...");
      const progressHandler: Progress.Handler = (value: number) => {
        onProgress?.(value);
      };

      const audioBuffer = await AudioOfflineRenderer.start(
        project,
        Option.wrap(exportConfig),
        progressHandler,
        abortSignal,
        sampleRate,
      );

      const result = new Map<string, ArrayBuffer>();
      const stems = Object.values(stemsConfig);

      for (let i = 0; i < stems.length; i++) {
        const stem = stems[i];
        if (!stem) continue;
        const leftIdx = i * 2;
        const rightIdx = i * 2 + 1;

        if (rightIdx >= audioBuffer.numberOfChannels) break;

        const stemBuffer = new AudioBuffer({
          length: audioBuffer.length,
          numberOfChannels: 2,
          sampleRate: audioBuffer.sampleRate,
        });
        stemBuffer.copyToChannel(audioBuffer.getChannelData(leftIdx), 0);
        stemBuffer.copyToChannel(audioBuffer.getChannelData(rightIdx), 1);

        onStatus?.(`Encoding ${stem.fileName}.wav...`);
        result.set(stem.fileName, WavFile.encodeFloats(stemBuffer));
      }

      return result;
    },

    setVolume(trackId: string, volumeDb: number): void {
      const binding = trackBindings.find(b => b.trackId === trackId);
      if (!binding) throw new Error(`Track binding not found: ${trackId}`);
      project.editing.modify(() => {
        binding.audioUnitBox.volume.setValue(volumeDb);
      });
    },

    setPan(trackId: string, pan: number): void {
      const binding = trackBindings.find(b => b.trackId === trackId);
      if (!binding) throw new Error(`Track binding not found: ${trackId}`);
      project.editing.modify(() => {
        binding.audioUnitBox.panning.setValue(pan);
      });
    },

    setMute(trackId: string, muted: boolean): void {
      const binding = trackBindings.find(b => b.trackId === trackId);
      if (!binding) throw new Error(`Track binding not found: ${trackId}`);
      project.editing.modify(() => {
        binding.audioUnitBox.mute.setValue(muted);
      });
      timeline.updateTrack(trackId, { muted });
    },

    setSolo(trackId: string, solo: boolean): void {
      const binding = trackBindings.find(b => b.trackId === trackId);
      if (!binding) throw new Error(`Track binding not found: ${trackId}`);
      project.editing.modify(() => {
        binding.audioUnitBox.solo.setValue(solo);
      });
      timeline.updateTrack(trackId, { solo });
    },

    dispose(): void {
      bridge.stopSync();
      trackBindings.length = 0;
      clipBindings.length = 0;
      audioContext.close();
      timeline.dispose();
    },
  };

  return bridge;
}
