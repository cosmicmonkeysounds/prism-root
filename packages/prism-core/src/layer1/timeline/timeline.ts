/**
 * @prism/core — NLE / Timeline Engine (Layer 1)
 *
 * Pure data-model timeline with transport, tracks, clips, automation,
 * tempo map (PPQN), and markers. No audio/video APIs — those are Layer 2.
 */

import type {
  AutomationLane,
  AutomationPoint,
  LoopRegion,
  ManualClock,
  MusicalPosition,
  TempoMap,
  TempoMarker,
  TimelineClip,
  TimelineClock,
  TimelineEngine,
  TimelineEvent,
  TimelineEventKind,
  TimelineListener,
  TimelineMarker,
  TimelineTrack,
  TimeSeconds,
  TimeSignature,
  TrackKind,
  TransportState,
} from "./timeline-types.js";

// ── ID generation ─────────────────────────────────────────────────────────

let nextId = 1;
function genId(prefix: string): string {
  return `${prefix}_${nextId++}`;
}

/** Reset ID counter (for testing). */
export function resetIdCounter(): void {
  nextId = 1;
}

// ── Tempo Map ─────────────────────────────────────────────────────────────

interface TimeSigEntry {
  time: TimeSeconds;
  sig: TimeSignature;
}

export function createTempoMap(ppq: number = 480, initialBpm: number = 120): TempoMap {
  const tempoMarkers: TempoMarker[] = [{ time: 0, bpm: initialBpm }];
  const timeSigs: TimeSigEntry[] = [{ time: 0, sig: { numerator: 4, denominator: 4 } }];

  function sortMarkers(): void {
    tempoMarkers.sort((a, b) => a.time - b.time);
  }

  function sortTimeSigs(): void {
    timeSigs.sort((a, b) => a.time - b.time);
  }

  function findActiveMarker(time: TimeSeconds): TempoMarker {
    let active = tempoMarkers[0];
    if (!active) throw new Error("No tempo markers");
    for (let i = 1; i < tempoMarkers.length; i++) {
      const m = tempoMarkers[i];
      if (m && m.time <= time) {
        active = m;
      } else {
        break;
      }
    }
    return active;
  }

  function findActiveTimeSig(time: TimeSeconds): TimeSignature {
    let active = timeSigs[0];
    if (!active) throw new Error("No time signatures");
    for (let i = 1; i < timeSigs.length; i++) {
      const entry = timeSigs[i];
      if (entry && entry.time <= time) {
        active = entry;
      } else {
        break;
      }
    }
    return active.sig;
  }

  /** Seconds per beat at a given BPM. */
  function secondsPerBeat(bpm: number): number {
    return 60 / bpm;
  }

  return {
    ppq,

    tempoAt(time: TimeSeconds): number {
      return findActiveMarker(time).bpm;
    },

    toMusical(time: TimeSeconds): MusicalPosition {
      // Walk through tempo regions accumulating beats
      let totalBeats = 0;
      let remaining = time;

      for (let i = 0; i < tempoMarkers.length; i++) {
        const marker = tempoMarkers[i];
        if (!marker) continue;
        const nextMarker = tempoMarkers[i + 1];
        const spb = secondsPerBeat(marker.bpm);

        if (nextMarker) {
          const regionDuration = nextMarker.time - marker.time;
          if (remaining <= regionDuration) {
            totalBeats += remaining / spb;
            remaining = 0;
            break;
          }
          totalBeats += regionDuration / spb;
          remaining -= regionDuration;
        } else {
          // Last region — consume all remaining
          totalBeats += remaining / spb;
          remaining = 0;
        }
      }

      // Convert total beats to bars/beats/ticks using active time sig
      const sig = findActiveTimeSig(time);
      const beatsPerBar = sig.numerator;
      const totalTicks = Math.round(totalBeats * ppq);
      const ticksPerBar = beatsPerBar * ppq;

      const bar = Math.floor(totalTicks / ticksPerBar) + 1; // 1-based
      const remainingTicks = totalTicks % ticksPerBar;
      const beat = Math.floor(remainingTicks / ppq) + 1; // 1-based
      const tick = remainingTicks % ppq;

      return { bar, beat, tick };
    },

    toSeconds(position: MusicalPosition): TimeSeconds {
      const sig = timeSigs[0];
      if (!sig) throw new Error("No time signatures");
      const beatsPerBar = sig.sig.numerator;

      // Convert position to total beats (0-based)
      const totalBeats = (position.bar - 1) * beatsPerBar + (position.beat - 1) + position.tick / ppq;

      // Walk through tempo regions consuming beats
      let beatsRemaining = totalBeats;
      let seconds = 0;

      for (let i = 0; i < tempoMarkers.length; i++) {
        const marker = tempoMarkers[i];
        if (!marker) continue;
        const nextMarker = tempoMarkers[i + 1];
        const spb = secondsPerBeat(marker.bpm);

        if (nextMarker) {
          const regionDuration = nextMarker.time - marker.time;
          const regionBeats = regionDuration / spb;
          if (beatsRemaining <= regionBeats) {
            seconds += beatsRemaining * spb;
            return seconds;
          }
          seconds += regionDuration;
          beatsRemaining -= regionBeats;
        } else {
          seconds += beatsRemaining * spb;
          return seconds;
        }
      }

      return seconds;
    },

    timeSignatureAt(time: TimeSeconds): TimeSignature {
      return findActiveTimeSig(time);
    },

    addTempo(marker: TempoMarker): void {
      // Replace if same time exists
      const idx = tempoMarkers.findIndex(m => Math.abs(m.time - marker.time) < 1e-9);
      if (idx >= 0) {
        tempoMarkers[idx] = marker;
      } else {
        tempoMarkers.push(marker);
      }
      sortMarkers();
    },

    setTimeSignature(time: TimeSeconds, sig: TimeSignature): void {
      const idx = timeSigs.findIndex(e => Math.abs(e.time - time) < 1e-9);
      if (idx >= 0) {
        timeSigs[idx] = { time, sig };
      } else {
        timeSigs.push({ time, sig });
      }
      sortTimeSigs();
    },

    getTempoMarkers(): TempoMarker[] {
      return tempoMarkers.map(m => ({ ...m }));
    },
  };
}

// ── Manual Clock (for testing) ────────────────────────────────────────────

export function createManualClock(): ManualClock {
  let currentTime = 0;
  let handleId = 0;
  const scheduled: Array<{ id: string; time: number; callback: () => void }> = [];

  function fireScheduled(): void {
    // Fire all callbacks whose time has passed, in order
    const toFire = scheduled
      .filter(s => s.time <= currentTime)
      .sort((a, b) => a.time - b.time);

    for (const entry of toFire) {
      const idx = scheduled.indexOf(entry);
      if (idx >= 0) scheduled.splice(idx, 1);
      entry.callback();
    }
  }

  return {
    now(): TimeSeconds {
      return currentTime;
    },

    schedule(time: TimeSeconds, callback: () => void): string {
      const id = `sched_${++handleId}`;
      scheduled.push({ id, time, callback });
      return id;
    },

    cancel(handle: string): void {
      const idx = scheduled.findIndex(s => s.id === handle);
      if (idx >= 0) scheduled.splice(idx, 1);
    },

    start(): void {
      // Manual clock doesn't auto-advance
    },

    stop(): void {
      // No-op
    },

    advance(seconds: TimeSeconds): void {
      currentTime += seconds;
      fireScheduled();
    },

    setTime(time: TimeSeconds): void {
      currentTime = time;
      fireScheduled();
    },
  };
}

// ── Automation Evaluation ─────────────────────────────────────────────────

function evaluateAutomation(lane: AutomationLane, time: TimeSeconds): number {
  if (lane.points.length === 0) return lane.defaultValue;

  const first = lane.points[0];
  if (!first) return lane.defaultValue;

  // Before first point
  if (time <= first.time) return first.value;

  // After last point
  const last = lane.points[lane.points.length - 1];
  if (!last) return lane.defaultValue;
  if (time >= last.time) return last.value;

  // Find surrounding points
  let prevPoint = first;
  let nextPoint = first;
  for (let i = 0; i < lane.points.length - 1; i++) {
    const p = lane.points[i];
    const pNext = lane.points[i + 1];
    if (!p || !pNext) continue;
    if (time >= p.time && time <= pNext.time) {
      prevPoint = p;
      nextPoint = pNext;
      break;
    }
  }

  if (prevPoint === nextPoint) return prevPoint.value;

  switch (prevPoint.interpolation) {
    case "step":
      return prevPoint.value;

    case "linear": {
      const t = (time - prevPoint.time) / (nextPoint.time - prevPoint.time);
      return prevPoint.value + t * (nextPoint.value - prevPoint.value);
    }

    case "bezier": {
      // Approximate bezier with smoothstep
      const t = (time - prevPoint.time) / (nextPoint.time - prevPoint.time);
      const smooth = t * t * (3 - 2 * t);
      return prevPoint.value + smooth * (nextPoint.value - prevPoint.value);
    }
  }
}

// ── Timeline Engine ───────────────────────────────────────────────────────

export interface TimelineEngineOptions {
  clock?: TimelineClock;
  ppq?: number;
  bpm?: number;
}

export function createTimelineEngine(options: TimelineEngineOptions = {}): TimelineEngine {
  const clock = options.clock ?? createManualClock();
  const tempoMap = createTempoMap(options.ppq ?? 480, options.bpm ?? 120);

  const tracks: TimelineTrack[] = [];
  const markers: TimelineMarker[] = [];
  const listeners: TimelineListener[] = [];

  let transport: TransportState = {
    status: "stopped",
    position: 0,
    speed: 1.0,
    loop: { enabled: false, start: 0, end: 0 },
  };

  // ── Helpers ───────────────────────────────────────────────

  function emit(kind: TimelineEventKind, data: Record<string, unknown> = {}): void {
    const event: TimelineEvent = { kind, timestamp: Date.now(), data };
    for (const listener of listeners) {
      listener(event);
    }
  }

  function requireTrack(trackId: string): TimelineTrack {
    const track = tracks.find(t => t.id === trackId);
    if (!track) throw new Error(`Track not found: ${trackId}`);
    return track;
  }

  function findClipGlobally(clipId: string): { track: TimelineTrack; clip: TimelineClip; clipIndex: number } | undefined {
    for (const track of tracks) {
      const clipIndex = track.clips.findIndex(c => c.id === clipId);
      if (clipIndex >= 0) {
        const clip = track.clips[clipIndex];
        if (clip) return { track, clip, clipIndex };
      }
    }
    return undefined;
  }

  function requireClip(clipId: string): { track: TimelineTrack; clip: TimelineClip; clipIndex: number } {
    const result = findClipGlobally(clipId);
    if (!result) throw new Error(`Clip not found: ${clipId}`);
    return result;
  }

  function requireLane(trackId: string, laneId: string): { track: TimelineTrack; lane: AutomationLane; laneIndex: number } {
    const track = requireTrack(trackId);
    const laneIndex = track.automationLanes.findIndex(l => l.id === laneId);
    if (laneIndex < 0) throw new Error(`Automation lane not found: ${laneId}`);
    const lane = track.automationLanes[laneIndex];
    if (!lane) throw new Error(`Automation lane not found: ${laneId}`);
    return { track, lane, laneIndex };
  }

  // ── Engine ────────────────────────────────────────────────

  const engine: TimelineEngine = {
    // ── Transport ─────────────────────────────────────────

    play(): void {
      if (transport.status === "playing") return;
      transport = {
        status: "playing",
        position: transport.position,
        speed: transport.speed,
        loop: transport.loop,
      };
      clock.start();
      emit("transport:play", { position: transport.position });
    },

    pause(): void {
      if (transport.status !== "playing") return;
      transport = {
        status: "paused",
        position: transport.position,
        speed: transport.speed,
        loop: transport.loop,
      };
      clock.stop();
      emit("transport:pause", { position: transport.position });
    },

    stop(): void {
      transport = {
        status: "stopped",
        position: 0,
        speed: transport.speed,
        loop: transport.loop,
      };
      clock.stop();
      emit("transport:stop");
    },

    seek(time: TimeSeconds): void {
      if (time < 0) time = 0;
      transport = {
        status: transport.status,
        position: time,
        speed: transport.speed,
        loop: transport.loop,
      };
      emit("transport:seek", { position: time });
    },

    scrub(time: TimeSeconds): void {
      if (time < 0) time = 0;
      transport = {
        status: transport.status,
        position: time,
        speed: transport.speed,
        loop: transport.loop,
      };
      // Scrub emits seek but doesn't change play state
      emit("transport:seek", { position: time, scrub: true });
    },

    setSpeed(speed: number): void {
      if (speed <= 0) throw new Error("Speed must be positive");
      transport = {
        status: transport.status,
        position: transport.position,
        speed,
        loop: transport.loop,
      };
    },

    setLoop(region: LoopRegion): void {
      transport = {
        status: transport.status,
        position: transport.position,
        speed: transport.speed,
        loop: region,
      };
      emit("transport:loop", { enabled: region.enabled, start: region.start, end: region.end });
    },

    getTransport(): TransportState {
      return { ...transport, loop: { ...transport.loop } };
    },

    // ── Tracks ────────────────────────────────────────────

    addTrack(kind: TrackKind, name: string): TimelineTrack {
      const track: TimelineTrack = {
        id: genId("track"),
        name,
        kind,
        muted: false,
        solo: false,
        locked: false,
        gain: 1.0,
        clips: [],
        automationLanes: [],
      };
      tracks.push(track);
      emit("track:added", { trackId: track.id, kind, name });
      return { ...track, clips: [], automationLanes: [] };
    },

    removeTrack(trackId: string): void {
      const idx = tracks.findIndex(t => t.id === trackId);
      if (idx < 0) throw new Error(`Track not found: ${trackId}`);
      tracks.splice(idx, 1);
      emit("track:removed", { trackId });
    },

    getTrack(trackId: string): TimelineTrack | undefined {
      const track = tracks.find(t => t.id === trackId);
      if (!track) return undefined;
      return {
        ...track,
        clips: track.clips.map(c => ({ ...c })),
        automationLanes: track.automationLanes.map(l => ({
          ...l,
          points: l.points.map(p => ({ ...p })),
        })),
      };
    },

    getTracks(): TimelineTrack[] {
      return tracks.map(t => ({
        ...t,
        clips: t.clips.map(c => ({ ...c })),
        automationLanes: t.automationLanes.map(l => ({
          ...l,
          points: l.points.map(p => ({ ...p })),
        })),
      }));
    },

    updateTrack(trackId: string, updates: Partial<Pick<TimelineTrack, "name" | "muted" | "solo" | "locked" | "gain">>): void {
      const track = requireTrack(trackId);
      if (updates.name !== undefined) track.name = updates.name;
      if (updates.muted !== undefined) track.muted = updates.muted;
      if (updates.solo !== undefined) track.solo = updates.solo;
      if (updates.locked !== undefined) track.locked = updates.locked;
      if (updates.gain !== undefined) track.gain = updates.gain;
      emit("track:updated", { trackId, updates });
    },

    // ── Clips ─────────────────────────────────────────────

    addClip(trackId: string, clipData: Omit<TimelineClip, "id" | "trackId">): TimelineClip {
      const track = requireTrack(trackId);
      if (track.locked) throw new Error("Track is locked");
      const clip: TimelineClip = {
        id: genId("clip"),
        trackId,
        name: clipData.name,
        startTime: clipData.startTime,
        duration: clipData.duration,
        sourceOffset: clipData.sourceOffset,
        sourceRef: clipData.sourceRef,
        muted: clipData.muted,
        locked: clipData.locked,
        gain: clipData.gain,
      };
      track.clips.push(clip);
      emit("clip:added", { trackId, clipId: clip.id });
      return { ...clip };
    },

    removeClip(trackId: string, clipId: string): void {
      const track = requireTrack(trackId);
      const idx = track.clips.findIndex(c => c.id === clipId);
      if (idx < 0) throw new Error(`Clip not found: ${clipId}`);
      track.clips.splice(idx, 1);
      emit("clip:removed", { trackId, clipId });
    },

    moveClip(clipId: string, targetTrackId: string, newStartTime: TimeSeconds): void {
      const { track: sourceTrack, clip, clipIndex } = requireClip(clipId);
      if (clip.locked) throw new Error("Clip is locked");

      const targetTrack = requireTrack(targetTrackId);
      if (targetTrack.locked) throw new Error("Target track is locked");

      // Remove from source
      sourceTrack.clips.splice(clipIndex, 1);

      // Update and add to target
      clip.trackId = targetTrackId;
      clip.startTime = newStartTime;
      targetTrack.clips.push(clip);

      emit("clip:moved", {
        clipId,
        fromTrackId: sourceTrack.id,
        toTrackId: targetTrackId,
        newStartTime,
      });
    },

    trimClip(clipId: string, newStartTime: TimeSeconds, newDuration: TimeSeconds): void {
      const { clip } = requireClip(clipId);
      if (clip.locked) throw new Error("Clip is locked");
      if (newDuration <= 0) throw new Error("Duration must be positive");

      const oldStartTime = clip.startTime;
      const startDelta = newStartTime - oldStartTime;

      clip.startTime = newStartTime;
      clip.duration = newDuration;
      clip.sourceOffset += startDelta;

      emit("clip:trimmed", { clipId, newStartTime, newDuration });
    },

    getClip(clipId: string): TimelineClip | undefined {
      const result = findClipGlobally(clipId);
      if (!result) return undefined;
      return { ...result.clip };
    },

    // ── Automation ────────────────────────────────────────

    addAutomationLane(trackId: string, parameter: string, defaultValue: number = 0): AutomationLane {
      const track = requireTrack(trackId);
      const lane: AutomationLane = {
        id: genId("lane"),
        parameter,
        defaultValue,
        points: [],
      };
      track.automationLanes.push(lane);
      return { ...lane, points: [] };
    },

    removeAutomationLane(trackId: string, laneId: string): void {
      const track = requireTrack(trackId);
      const idx = track.automationLanes.findIndex(l => l.id === laneId);
      if (idx < 0) throw new Error(`Automation lane not found: ${laneId}`);
      track.automationLanes.splice(idx, 1);
    },

    addAutomationPoint(trackId: string, laneId: string, point: AutomationPoint): void {
      const { lane } = requireLane(trackId, laneId);
      // Insert sorted by time
      const insertIdx = lane.points.findIndex(p => p.time > point.time);
      if (insertIdx < 0) {
        lane.points.push(point);
      } else {
        lane.points.splice(insertIdx, 0, point);
      }
    },

    removeAutomationPoint(trackId: string, laneId: string, time: TimeSeconds): void {
      const { lane } = requireLane(trackId, laneId);
      const idx = lane.points.findIndex(p => Math.abs(p.time - time) < 1e-9);
      if (idx >= 0) lane.points.splice(idx, 1);
    },

    getAutomationValue(trackId: string, laneId: string, time: TimeSeconds): number {
      const { lane } = requireLane(trackId, laneId);
      return evaluateAutomation(lane, time);
    },

    // ── Markers ───────────────────────────────────────────

    addMarker(time: TimeSeconds, label: string, color: string = "#ffcc00"): TimelineMarker {
      const marker: TimelineMarker = {
        id: genId("marker"),
        time,
        label,
        color,
      };
      markers.push(marker);
      markers.sort((a, b) => a.time - b.time);
      emit("marker:added", { markerId: marker.id, time, label });
      return { ...marker };
    },

    removeMarker(markerId: string): void {
      const idx = markers.findIndex(m => m.id === markerId);
      if (idx < 0) throw new Error(`Marker not found: ${markerId}`);
      markers.splice(idx, 1);
      emit("marker:removed", { markerId });
    },

    getMarkers(): TimelineMarker[] {
      return markers.map(m => ({ ...m }));
    },

    // ── Tempo ─────────────────────────────────────────────

    getTempoMap(): TempoMap {
      return tempoMap;
    },

    // ── Queries ───────────────────────────────────────────

    getDuration(): TimeSeconds {
      let maxEnd = 0;
      for (const track of tracks) {
        for (const clip of track.clips) {
          const end = clip.startTime + clip.duration;
          if (end > maxEnd) maxEnd = end;
        }
      }
      return maxEnd;
    },

    getClipsAtTime(time: TimeSeconds): TimelineClip[] {
      const result: TimelineClip[] = [];
      for (const track of tracks) {
        for (const clip of track.clips) {
          if (time >= clip.startTime && time < clip.startTime + clip.duration) {
            result.push({ ...clip });
          }
        }
      }
      return result;
    },

    // ── Events ────────────────────────────────────────────

    subscribe(listener: TimelineListener): () => void {
      listeners.push(listener);
      return () => {
        const idx = listeners.indexOf(listener);
        if (idx >= 0) listeners.splice(idx, 1);
      };
    },

    // ── Lifecycle ─────────────────────────────────────────

    dispose(): void {
      clock.stop();
      tracks.length = 0;
      markers.length = 0;
      listeners.length = 0;
    },
  };

  return engine;
}
