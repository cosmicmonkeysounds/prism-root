/**
 * @prism/core — OpenDAW React Hooks (Layer 2)
 *
 * React hooks for integrating OpenDAW audio with Prism timeline.
 * Wraps the bridge in React lifecycle management.
 */

import { useState, useEffect, useRef, useCallback } from "react";
import { AnimationFrame } from "@opendaw/lib-dom";
import { PPQN } from "@opendaw/lib-dsp";
import type { TimelineEngine } from "@prism/core/timeline";
import type { OpenDawBridge, OpenDawBridgeOptions, OpenDawEffectType } from "./opendaw-types.js";
import { createOpenDawBridge } from "./opendaw-bridge.js";

// ── useOpenDawBridge ──────────────────────────────────────────────────────

export interface UseOpenDawBridgeResult {
  /** The bridge instance (null while initializing). */
  bridge: OpenDawBridge | null;
  /** Whether the bridge is currently initializing. */
  loading: boolean;
  /** Error if initialization failed. */
  error: Error | null;
  /** Status message during initialization. */
  status: string;
}

/**
 * Hook to create and manage an OpenDAW bridge connected to a Prism timeline.
 *
 * Handles initialization, sync start, and cleanup on unmount.
 * Workers must be installed before this hook is used.
 *
 * @example
 * ```tsx
 * function AudioTimeline() {
 *   const timeline = useMemo(() => createTimelineEngine(), []);
 *   const { bridge, loading, error } = useOpenDawBridge(timeline, { bpm: 120 });
 *
 *   if (loading) return <p>Loading audio engine...</p>;
 *   if (error) return <p>Error: {error.message}</p>;
 *
 *   return <TransportControls timeline={timeline} />;
 * }
 * ```
 */
export function useOpenDawBridge(
  timeline: TimelineEngine | null,
  options?: OpenDawBridgeOptions,
): UseOpenDawBridgeResult {
  const [bridge, setBridge] = useState<OpenDawBridge | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<Error | null>(null);
  const [status, setStatus] = useState("Initializing...");

  useEffect(() => {
    if (!timeline) return;

    let disposed = false;
    let bridgeInstance: OpenDawBridge | null = null;
    const tl = timeline;

    async function init() {
      try {
        bridgeInstance = await createOpenDawBridge(tl, {
          ...options,
          onStatusUpdate: (msg) => {
            if (!disposed) setStatus(msg);
            options?.onStatusUpdate?.(msg);
          },
        });

        if (!disposed) {
          bridgeInstance.startSync();
          setBridge(bridgeInstance);
          setLoading(false);
        } else {
          bridgeInstance.dispose();
        }
      } catch (e) {
        if (!disposed) {
          setError(e instanceof Error ? e : new Error(String(e)));
          setLoading(false);
        }
      }
    }

    init();

    return () => {
      disposed = true;
      if (bridgeInstance) {
        bridgeInstance.dispose();
      }
      setBridge(null);
    };
  }, [timeline]);

  return { bridge, loading, error, status };
}

// ── usePlaybackPosition ───────────────────────────────────────────────────

export interface PlaybackPositionResult {
  /** Current position in seconds. */
  position: number;
  /** Whether the engine is currently playing. */
  isPlaying: boolean;
  /** Current position in musical time (bar.beat). */
  musicalPosition: string;
}

/**
 * Hook for tracking playback position from an OpenDAW bridge.
 * Updates every animation frame while playing.
 *
 * @example
 * ```tsx
 * const { position, isPlaying, musicalPosition } = usePlaybackPosition(bridge);
 * return <span>{musicalPosition} ({position.toFixed(1)}s)</span>;
 * ```
 */
export function usePlaybackPosition(bridge: OpenDawBridge | null): PlaybackPositionResult {
  const [position, setPosition] = useState(0);
  const [isPlaying, setIsPlaying] = useState(false);
  const [musicalPosition, setMusicalPosition] = useState("1.1.000");

  useEffect(() => {
    if (!bridge) return;

    const playingSub = bridge.project.engine.isPlaying.catchupAndSubscribe((obs: { getValue(): boolean }) => {
      setIsPlaying(obs.getValue());
    });

    const frameSub = AnimationFrame.add(() => {
      const ppqnPos = bridge.project.engine.position.getValue();
      const bpm = bridge.project.timelineBox.bpm.getValue();
      const seconds = PPQN.pulsesToSeconds(ppqnPos, bpm);
      setPosition(seconds);

      // Convert to musical position via Prism tempo map
      const mp = bridge.timeline.getTempoMap().toMusical(seconds);
      setMusicalPosition(`${mp.bar}.${mp.beat}.${String(mp.tick).padStart(3, "0")}`);
    });

    return () => {
      playingSub.terminate();
      frameSub.terminate();
    };
  }, [bridge]);

  return { position, isPlaying, musicalPosition };
}

// ── useTransportControls ──────────────────────────────────────────────────

export interface TransportControlsResult {
  /** Start playback. */
  handlePlay: () => void;
  /** Pause playback (retains position). */
  handlePause: () => void;
  /** Stop playback (resets to start). */
  handleStop: () => void;
  /** Seek to a specific time in seconds. */
  handleSeek: (time: number) => void;
}

/**
 * Hook for transport control handlers connected to a Prism timeline + OpenDAW bridge.
 * Controls go through the Prism timeline; the bridge syncs to OpenDAW.
 *
 * @example
 * ```tsx
 * const { handlePlay, handlePause, handleStop } = useTransportControls(bridge);
 * return (
 *   <>
 *     <button onClick={handlePlay}>Play</button>
 *     <button onClick={handlePause}>Pause</button>
 *     <button onClick={handleStop}>Stop</button>
 *   </>
 * );
 * ```
 */
export function useTransportControls(bridge: OpenDawBridge | null): TransportControlsResult {
  const handlePlay = useCallback(async () => {
    if (!bridge) return;
    // Resume AudioContext if suspended (browser autoplay policy)
    if (bridge.audioContext.state === "suspended") {
      await bridge.audioContext.resume();
    }
    bridge.timeline.play();
  }, [bridge]);

  const handlePause = useCallback(() => {
    if (!bridge) return;
    bridge.timeline.pause();
  }, [bridge]);

  const handleStop = useCallback(() => {
    if (!bridge) return;
    bridge.timeline.stop();
  }, [bridge]);

  const handleSeek = useCallback((time: number) => {
    if (!bridge) return;
    bridge.timeline.seek(time);
  }, [bridge]);

  return { handlePlay, handlePause, handleStop, handleSeek };
}

// ── useTrackEffects ───────────────────────────────────────────────────────

export interface TrackEffectsResult {
  /** Insert an effect on the track. */
  addEffect: (effectType: OpenDawEffectType) => void;
  /** Set track volume in dB. */
  setVolume: (db: number) => void;
  /** Set track pan (-1..1). */
  setPan: (pan: number) => void;
  /** Toggle mute. */
  toggleMute: () => void;
  /** Toggle solo. */
  toggleSolo: () => void;
}

/**
 * Hook for per-track audio controls (effects, volume, pan, mute, solo).
 *
 * @example
 * ```tsx
 * const { addEffect, setVolume, toggleMute } = useTrackEffects(bridge, trackId);
 * return (
 *   <>
 *     <input type="range" onChange={e => setVolume(Number(e.target.value))} />
 *     <button onClick={() => addEffect("Reverb")}>Add Reverb</button>
 *     <button onClick={toggleMute}>Mute</button>
 *   </>
 * );
 * ```
 */
export function useTrackEffects(bridge: OpenDawBridge | null, trackId: string): TrackEffectsResult {
  const mutedRef = useRef(false);
  const soloRef = useRef(false);

  const addEffect = useCallback((effectType: OpenDawEffectType) => {
    if (!bridge) return;
    bridge.insertEffect(trackId, effectType);
  }, [bridge, trackId]);

  const setVolume = useCallback((db: number) => {
    if (!bridge) return;
    bridge.setVolume(trackId, db);
  }, [bridge, trackId]);

  const setPan = useCallback((pan: number) => {
    if (!bridge) return;
    bridge.setPan(trackId, pan);
  }, [bridge, trackId]);

  const toggleMute = useCallback(() => {
    if (!bridge) return;
    mutedRef.current = !mutedRef.current;
    bridge.setMute(trackId, mutedRef.current);
  }, [bridge, trackId]);

  const toggleSolo = useCallback(() => {
    if (!bridge) return;
    soloRef.current = !soloRef.current;
    bridge.setSolo(trackId, soloRef.current);
  }, [bridge, trackId]);

  return { addEffect, setVolume, setPan, toggleMute, toggleSolo };
}
