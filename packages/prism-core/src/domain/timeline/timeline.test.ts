import { describe, it, expect, beforeEach } from "vitest";
import {
  createTimelineEngine,
  createManualClock,
  createTempoMap,
  resetIdCounter,
} from "./timeline.js";
import type {
  TimelineEngine,
  TimelineEvent,
  ManualClock,
} from "./timeline-types.js";

// ── Helpers ───────────────────────────────────────────────────────────────

function makeClip(overrides: Record<string, unknown> = {}) {
  return {
    name: "clip",
    startTime: 0,
    duration: 5,
    sourceOffset: 0,
    sourceRef: "audio.wav",
    muted: false,
    locked: false,
    gain: 1.0,
    ...overrides,
  };
}

// ── Tests ─────────────────────────────────────────────────────────────────

describe("Timeline Engine", () => {
  let engine: TimelineEngine;
  let clock: ManualClock;
  let events: TimelineEvent[];

  beforeEach(() => {
    resetIdCounter();
    clock = createManualClock();
    engine = createTimelineEngine({ clock });
    events = [];
    engine.subscribe(e => events.push(e));
  });

  // ── Transport ─────────────────────────────────────────────────────────

  describe("Transport", () => {
    it("starts in stopped state", () => {
      const t = engine.getTransport();
      expect(t.status).toBe("stopped");
      expect(t.position).toBe(0);
      expect(t.speed).toBe(1.0);
    });

    it("transitions: stopped → playing → paused → stopped", () => {
      engine.play();
      expect(engine.getTransport().status).toBe("playing");

      engine.pause();
      expect(engine.getTransport().status).toBe("paused");

      engine.stop();
      expect(engine.getTransport().status).toBe("stopped");
      expect(engine.getTransport().position).toBe(0);
    });

    it("play is idempotent when already playing", () => {
      engine.play();
      engine.play();
      // Only one play event
      const playEvents = events.filter(e => e.kind === "transport:play");
      expect(playEvents).toHaveLength(1);
    });

    it("pause is no-op when not playing", () => {
      engine.pause();
      expect(events).toHaveLength(0);
    });

    it("seek updates position", () => {
      engine.seek(10);
      expect(engine.getTransport().position).toBe(10);
      expect(events[0]?.kind).toBe("transport:seek");
    });

    it("seek clamps to 0", () => {
      engine.seek(-5);
      expect(engine.getTransport().position).toBe(0);
    });

    it("scrub updates position without changing play state", () => {
      engine.play();
      engine.scrub(15);
      expect(engine.getTransport().status).toBe("playing");
      expect(engine.getTransport().position).toBe(15);
    });

    it("setSpeed validates positive values", () => {
      engine.setSpeed(2.0);
      expect(engine.getTransport().speed).toBe(2.0);
      expect(() => engine.setSpeed(0)).toThrow("Speed must be positive");
      expect(() => engine.setSpeed(-1)).toThrow("Speed must be positive");
    });

    it("setLoop configures loop region", () => {
      engine.setLoop({ enabled: true, start: 5, end: 15 });
      const t = engine.getTransport();
      expect(t.loop.enabled).toBe(true);
      expect(t.loop.start).toBe(5);
      expect(t.loop.end).toBe(15);
    });

    it("emits transport events", () => {
      engine.play();
      engine.seek(5);
      engine.pause();
      engine.stop();

      const kinds = events.map(e => e.kind);
      expect(kinds).toEqual([
        "transport:play",
        "transport:seek",
        "transport:pause",
        "transport:stop",
      ]);
    });
  });

  // ── Tracks ────────────────────────────────────────────────────────────

  describe("Tracks", () => {
    it("adds and retrieves tracks", () => {
      const track = engine.addTrack("audio", "Vocals");
      expect(track.name).toBe("Vocals");
      expect(track.kind).toBe("audio");
      expect(track.muted).toBe(false);
      expect(track.gain).toBe(1.0);

      const retrieved = engine.getTrack(track.id);
      expect(retrieved?.name).toBe("Vocals");
    });

    it("lists all tracks", () => {
      engine.addTrack("audio", "Track 1");
      engine.addTrack("video", "Track 2");
      engine.addTrack("midi", "Track 3");
      expect(engine.getTracks()).toHaveLength(3);
    });

    it("removes tracks", () => {
      const track = engine.addTrack("audio", "Vocals");
      engine.removeTrack(track.id);
      expect(engine.getTracks()).toHaveLength(0);
    });

    it("throws on removing non-existent track", () => {
      expect(() => engine.removeTrack("nonexistent")).toThrow("Track not found");
    });

    it("updates track properties", () => {
      const track = engine.addTrack("audio", "Vocals");
      engine.updateTrack(track.id, { muted: true, gain: 0.5, name: "Lead Vox" });
      const updated = engine.getTrack(track.id);
      expect(updated?.muted).toBe(true);
      expect(updated?.gain).toBe(0.5);
      expect(updated?.name).toBe("Lead Vox");
    });

    it("returns undefined for non-existent track", () => {
      expect(engine.getTrack("nonexistent")).toBeUndefined();
    });

    it("emits track events", () => {
      const track = engine.addTrack("lighting", "DMX");
      engine.updateTrack(track.id, { solo: true });
      engine.removeTrack(track.id);

      const kinds = events.map(e => e.kind);
      expect(kinds).toEqual(["track:added", "track:updated", "track:removed"]);
    });

    it("returns defensive copies", () => {
      const track = engine.addTrack("audio", "Test");
      const retrieved = engine.getTrack(track.id);
      if (retrieved) {
        retrieved.name = "Mutated";
        expect(engine.getTrack(track.id)?.name).toBe("Test");
      }
    });
  });

  // ── Clips ─────────────────────────────────────────────────────────────

  describe("Clips", () => {
    it("adds clips to tracks", () => {
      const track = engine.addTrack("audio", "Vocals");
      const clip = engine.addClip(track.id, makeClip({ name: "Verse 1", startTime: 2, duration: 8 }));
      expect(clip.name).toBe("Verse 1");
      expect(clip.startTime).toBe(2);
      expect(clip.duration).toBe(8);
      expect(clip.trackId).toBe(track.id);
    });

    it("retrieves clips by ID", () => {
      const track = engine.addTrack("audio", "Vocals");
      const clip = engine.addClip(track.id, makeClip({ name: "Chorus" }));
      const retrieved = engine.getClip(clip.id);
      expect(retrieved?.name).toBe("Chorus");
    });

    it("removes clips", () => {
      const track = engine.addTrack("audio", "Vocals");
      const clip = engine.addClip(track.id, makeClip());
      engine.removeClip(track.id, clip.id);
      expect(engine.getClip(clip.id)).toBeUndefined();
    });

    it("throws on adding to locked track", () => {
      const track = engine.addTrack("audio", "Vocals");
      engine.updateTrack(track.id, { locked: true });
      expect(() => engine.addClip(track.id, makeClip())).toThrow("Track is locked");
    });

    it("moves clips between tracks", () => {
      const t1 = engine.addTrack("audio", "Track 1");
      const t2 = engine.addTrack("audio", "Track 2");
      const clip = engine.addClip(t1.id, makeClip({ name: "movable" }));

      engine.moveClip(clip.id, t2.id, 10);

      expect(engine.getTrack(t1.id)?.clips).toHaveLength(0);
      expect(engine.getTrack(t2.id)?.clips).toHaveLength(1);
      const moved = engine.getClip(clip.id);
      expect(moved?.trackId).toBe(t2.id);
      expect(moved?.startTime).toBe(10);
    });

    it("throws on moving locked clip", () => {
      const t1 = engine.addTrack("audio", "Track 1");
      const t2 = engine.addTrack("audio", "Track 2");
      const clip = engine.addClip(t1.id, makeClip({ locked: true }));
      expect(() => engine.moveClip(clip.id, t2.id, 0)).toThrow("Clip is locked");
    });

    it("throws on moving to locked track", () => {
      const t1 = engine.addTrack("audio", "Track 1");
      const t2 = engine.addTrack("audio", "Track 2");
      engine.updateTrack(t2.id, { locked: true });
      const clip = engine.addClip(t1.id, makeClip());
      expect(() => engine.moveClip(clip.id, t2.id, 0)).toThrow("Target track is locked");
    });

    it("trims clips adjusting sourceOffset", () => {
      const track = engine.addTrack("audio", "Vocals");
      const clip = engine.addClip(track.id, makeClip({ startTime: 5, duration: 10, sourceOffset: 0 }));

      engine.trimClip(clip.id, 7, 6);

      const trimmed = engine.getClip(clip.id);
      expect(trimmed?.startTime).toBe(7);
      expect(trimmed?.duration).toBe(6);
      expect(trimmed?.sourceOffset).toBe(2); // shifted by 2s
    });

    it("throws on trimming with non-positive duration", () => {
      const track = engine.addTrack("audio", "Vocals");
      const clip = engine.addClip(track.id, makeClip());
      expect(() => engine.trimClip(clip.id, 0, 0)).toThrow("Duration must be positive");
    });

    it("emits clip events", () => {
      const track = engine.addTrack("audio", "Vocals");
      events.length = 0; // clear track:added
      const clip = engine.addClip(track.id, makeClip());
      engine.removeClip(track.id, clip.id);

      const kinds = events.map(e => e.kind);
      expect(kinds).toEqual(["clip:added", "clip:removed"]);
    });

    it("returns defensive copies", () => {
      const track = engine.addTrack("audio", "Test");
      const clip = engine.addClip(track.id, makeClip({ name: "original" }));
      const retrieved = engine.getClip(clip.id);
      if (retrieved) {
        retrieved.name = "mutated";
        expect(engine.getClip(clip.id)?.name).toBe("original");
      }
    });
  });

  // ── Automation ────────────────────────────────────────────────────────

  describe("Automation", () => {
    it("adds automation lanes to tracks", () => {
      const track = engine.addTrack("audio", "Vocals");
      const lane = engine.addAutomationLane(track.id, "volume", 0.8);
      expect(lane.parameter).toBe("volume");
      expect(lane.defaultValue).toBe(0.8);
      expect(lane.points).toHaveLength(0);
    });

    it("removes automation lanes", () => {
      const track = engine.addTrack("audio", "Vocals");
      const lane = engine.addAutomationLane(track.id, "volume");
      engine.removeAutomationLane(track.id, lane.id);
      const t = engine.getTrack(track.id);
      expect(t?.automationLanes).toHaveLength(0);
    });

    it("adds points in sorted order", () => {
      const track = engine.addTrack("audio", "Vocals");
      const lane = engine.addAutomationLane(track.id, "volume");

      engine.addAutomationPoint(track.id, lane.id, { time: 5, value: 0.5, interpolation: "linear" });
      engine.addAutomationPoint(track.id, lane.id, { time: 2, value: 0.2, interpolation: "linear" });
      engine.addAutomationPoint(track.id, lane.id, { time: 8, value: 0.9, interpolation: "step" });

      const t = engine.getTrack(track.id);
      const points = t?.automationLanes[0]?.points;
      expect(points?.map(p => p.time)).toEqual([2, 5, 8]);
    });

    it("removes points by time", () => {
      const track = engine.addTrack("audio", "Vocals");
      const lane = engine.addAutomationLane(track.id, "volume");
      engine.addAutomationPoint(track.id, lane.id, { time: 5, value: 0.5, interpolation: "linear" });
      engine.addAutomationPoint(track.id, lane.id, { time: 10, value: 1.0, interpolation: "linear" });

      engine.removeAutomationPoint(track.id, lane.id, 5);

      const t = engine.getTrack(track.id);
      expect(t?.automationLanes[0]?.points).toHaveLength(1);
    });

    it("evaluates step interpolation", () => {
      const track = engine.addTrack("audio", "Vocals");
      const lane = engine.addAutomationLane(track.id, "volume", 0);
      engine.addAutomationPoint(track.id, lane.id, { time: 0, value: 0.0, interpolation: "step" });
      engine.addAutomationPoint(track.id, lane.id, { time: 5, value: 1.0, interpolation: "step" });

      expect(engine.getAutomationValue(track.id, lane.id, 0)).toBe(0.0);
      expect(engine.getAutomationValue(track.id, lane.id, 2.5)).toBe(0.0); // step holds
      expect(engine.getAutomationValue(track.id, lane.id, 5)).toBe(1.0);
    });

    it("evaluates linear interpolation", () => {
      const track = engine.addTrack("audio", "Vocals");
      const lane = engine.addAutomationLane(track.id, "volume", 0);
      engine.addAutomationPoint(track.id, lane.id, { time: 0, value: 0.0, interpolation: "linear" });
      engine.addAutomationPoint(track.id, lane.id, { time: 10, value: 1.0, interpolation: "linear" });

      expect(engine.getAutomationValue(track.id, lane.id, 5)).toBeCloseTo(0.5);
      expect(engine.getAutomationValue(track.id, lane.id, 7.5)).toBeCloseTo(0.75);
    });

    it("evaluates bezier (smoothstep) interpolation", () => {
      const track = engine.addTrack("audio", "Vocals");
      const lane = engine.addAutomationLane(track.id, "volume", 0);
      engine.addAutomationPoint(track.id, lane.id, { time: 0, value: 0.0, interpolation: "bezier" });
      engine.addAutomationPoint(track.id, lane.id, { time: 10, value: 1.0, interpolation: "bezier" });

      // Smoothstep at midpoint t=0.5: 0.5*0.5*(3-2*0.5) = 0.5
      expect(engine.getAutomationValue(track.id, lane.id, 5)).toBeCloseTo(0.5);
      // Smoothstep at t=0.25: 0.0625*(3-0.5) = 0.15625
      expect(engine.getAutomationValue(track.id, lane.id, 2.5)).toBeCloseTo(0.15625);
    });

    it("returns default value for empty lanes", () => {
      const track = engine.addTrack("audio", "Vocals");
      const lane = engine.addAutomationLane(track.id, "volume", 0.75);
      expect(engine.getAutomationValue(track.id, lane.id, 5)).toBe(0.75);
    });

    it("clamps before first / after last point", () => {
      const track = engine.addTrack("audio", "Vocals");
      const lane = engine.addAutomationLane(track.id, "volume", 0);
      engine.addAutomationPoint(track.id, lane.id, { time: 5, value: 0.3, interpolation: "linear" });
      engine.addAutomationPoint(track.id, lane.id, { time: 15, value: 0.9, interpolation: "linear" });

      expect(engine.getAutomationValue(track.id, lane.id, 0)).toBe(0.3); // before first
      expect(engine.getAutomationValue(track.id, lane.id, 20)).toBe(0.9); // after last
    });
  });

  // ── Markers ───────────────────────────────────────────────────────────

  describe("Markers", () => {
    it("adds markers in sorted order", () => {
      engine.addMarker(10, "Chorus");
      engine.addMarker(5, "Verse");
      engine.addMarker(20, "Bridge");

      const m = engine.getMarkers();
      expect(m.map(x => x.label)).toEqual(["Verse", "Chorus", "Bridge"]);
    });

    it("removes markers", () => {
      const marker = engine.addMarker(5, "Intro");
      engine.removeMarker(marker.id);
      expect(engine.getMarkers()).toHaveLength(0);
    });

    it("uses default color", () => {
      const marker = engine.addMarker(0, "Start");
      expect(marker.color).toBe("#ffcc00");
    });

    it("accepts custom color", () => {
      const marker = engine.addMarker(0, "Start", "#ff0000");
      expect(marker.color).toBe("#ff0000");
    });

    it("throws on removing non-existent marker", () => {
      expect(() => engine.removeMarker("nonexistent")).toThrow("Marker not found");
    });

    it("emits marker events", () => {
      const marker = engine.addMarker(5, "Drop");
      engine.removeMarker(marker.id);

      const kinds = events.filter(e => e.kind.startsWith("marker:")).map(e => e.kind);
      expect(kinds).toEqual(["marker:added", "marker:removed"]);
    });
  });

  // ── Queries ───────────────────────────────────────────────────────────

  describe("Queries", () => {
    it("getDuration returns max clip end time", () => {
      const t1 = engine.addTrack("audio", "T1");
      const t2 = engine.addTrack("audio", "T2");
      engine.addClip(t1.id, makeClip({ startTime: 0, duration: 10 }));
      engine.addClip(t2.id, makeClip({ startTime: 5, duration: 20 }));

      expect(engine.getDuration()).toBe(25);
    });

    it("getDuration returns 0 for empty timeline", () => {
      expect(engine.getDuration()).toBe(0);
    });

    it("getClipsAtTime finds overlapping clips", () => {
      const track = engine.addTrack("audio", "T1");
      engine.addClip(track.id, makeClip({ name: "A", startTime: 0, duration: 10 }));
      engine.addClip(track.id, makeClip({ name: "B", startTime: 8, duration: 10 }));
      engine.addClip(track.id, makeClip({ name: "C", startTime: 20, duration: 5 }));

      const at9 = engine.getClipsAtTime(9);
      expect(at9.map(c => c.name)).toEqual(["A", "B"]);

      const at20 = engine.getClipsAtTime(20);
      expect(at20.map(c => c.name)).toEqual(["C"]);
    });

    it("getClipsAtTime excludes clips at their end boundary", () => {
      const track = engine.addTrack("audio", "T1");
      engine.addClip(track.id, makeClip({ name: "A", startTime: 0, duration: 5 }));
      // time=5 is exclusive end of clip A
      expect(engine.getClipsAtTime(5)).toHaveLength(0);
    });
  });

  // ── Events ────────────────────────────────────────────────────────────

  describe("Events", () => {
    it("unsubscribes listeners", () => {
      const localEvents: TimelineEvent[] = [];
      const unsub = engine.subscribe(e => localEvents.push(e));

      engine.addTrack("audio", "T1");
      expect(localEvents).toHaveLength(1);

      unsub();
      engine.addTrack("audio", "T2");
      expect(localEvents).toHaveLength(1); // no new events
    });
  });

  // ── Lifecycle ─────────────────────────────────────────────────────────

  describe("Lifecycle", () => {
    it("dispose clears all state", () => {
      engine.addTrack("audio", "T1");
      engine.addMarker(5, "Mark");
      engine.dispose();

      expect(engine.getTracks()).toHaveLength(0);
      expect(engine.getMarkers()).toHaveLength(0);
    });
  });
});

// ── Manual Clock ────────────────────────────────────────────────────────

describe("ManualClock", () => {
  it("starts at time 0", () => {
    const clock = createManualClock();
    expect(clock.now()).toBe(0);
  });

  it("advances time", () => {
    const clock = createManualClock();
    clock.advance(5);
    expect(clock.now()).toBe(5);
    clock.advance(3);
    expect(clock.now()).toBe(8);
  });

  it("sets time directly", () => {
    const clock = createManualClock();
    clock.setTime(10);
    expect(clock.now()).toBe(10);
  });

  it("fires scheduled callbacks when time passes", () => {
    const clock = createManualClock();
    let fired = false;
    clock.schedule(5, () => { fired = true; });

    clock.advance(3);
    expect(fired).toBe(false);

    clock.advance(3); // now at 6, past the 5s mark
    expect(fired).toBe(true);
  });

  it("cancels scheduled callbacks", () => {
    const clock = createManualClock();
    let fired = false;
    const handle = clock.schedule(5, () => { fired = true; });

    clock.cancel(handle);
    clock.advance(10);
    expect(fired).toBe(false);
  });

  it("fires multiple callbacks in time order", () => {
    const clock = createManualClock();
    const order: number[] = [];
    clock.schedule(3, () => order.push(3));
    clock.schedule(1, () => order.push(1));
    clock.schedule(5, () => order.push(5));

    clock.advance(10);
    expect(order).toEqual([1, 3, 5]);
  });
});

// ── Tempo Map ───────────────────────────────────────────────────────────

describe("TempoMap", () => {
  it("defaults to 120 BPM, 4/4", () => {
    const tm = createTempoMap();
    expect(tm.tempoAt(0)).toBe(120);
    expect(tm.timeSignatureAt(0)).toEqual({ numerator: 4, denominator: 4 });
  });

  it("accepts custom initial BPM", () => {
    const tm = createTempoMap(480, 140);
    expect(tm.tempoAt(0)).toBe(140);
  });

  it("converts seconds to musical position at constant tempo", () => {
    const tm = createTempoMap(480, 120); // 120 BPM = 2 beats/sec
    // At 1 second = 2 beats = bar 1, beat 3
    const pos = tm.toMusical(1);
    expect(pos.bar).toBe(1);
    expect(pos.beat).toBe(3);
    expect(pos.tick).toBe(0);
  });

  it("converts musical position back to seconds", () => {
    const tm = createTempoMap(480, 120);
    // Bar 1, beat 3 = 2 beats = 1 second at 120 BPM
    const seconds = tm.toSeconds({ bar: 1, beat: 3, tick: 0 });
    expect(seconds).toBeCloseTo(1.0);
  });

  it("round-trips musical position", () => {
    const tm = createTempoMap(480, 90);
    const time = 7.5;
    const pos = tm.toMusical(time);
    const backToTime = tm.toSeconds(pos);
    expect(backToTime).toBeCloseTo(time, 4);
  });

  it("handles tempo changes", () => {
    const tm = createTempoMap(480, 120);
    tm.addTempo({ time: 4, bpm: 60 }); // slow down at 4s

    expect(tm.tempoAt(0)).toBe(120);
    expect(tm.tempoAt(4)).toBe(60);
    expect(tm.tempoAt(10)).toBe(60);
  });

  it("replaces tempo at same time", () => {
    const tm = createTempoMap(480, 120);
    tm.addTempo({ time: 0, bpm: 90 });
    expect(tm.tempoAt(0)).toBe(90);
    expect(tm.getTempoMarkers()).toHaveLength(1);
  });

  it("lists tempo markers", () => {
    const tm = createTempoMap(480, 120);
    tm.addTempo({ time: 10, bpm: 140 });
    tm.addTempo({ time: 5, bpm: 100 });

    const markers = tm.getTempoMarkers();
    expect(markers).toHaveLength(3);
    expect(markers.map(m => m.time)).toEqual([0, 5, 10]);
  });

  it("handles time signature changes", () => {
    const tm = createTempoMap(480, 120);
    tm.setTimeSignature(8, { numerator: 3, denominator: 4 });

    expect(tm.timeSignatureAt(0)).toEqual({ numerator: 4, denominator: 4 });
    expect(tm.timeSignatureAt(8)).toEqual({ numerator: 3, denominator: 4 });
    expect(tm.timeSignatureAt(20)).toEqual({ numerator: 3, denominator: 4 });
  });

  it("handles ticks in musical position", () => {
    const tm = createTempoMap(480, 120);
    // Half a beat = 240 ticks at ppq=480
    // At 120 BPM, half a beat = 0.25s
    const pos = tm.toMusical(0.25);
    expect(pos.bar).toBe(1);
    expect(pos.beat).toBe(1);
    expect(pos.tick).toBe(240);
  });

  it("converts position with ticks back to seconds", () => {
    const tm = createTempoMap(480, 120);
    const seconds = tm.toSeconds({ bar: 1, beat: 1, tick: 240 });
    expect(seconds).toBeCloseTo(0.25);
  });
});
