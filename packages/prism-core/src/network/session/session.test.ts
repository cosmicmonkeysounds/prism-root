import { describe, it, expect } from "vitest";
import {
  createTranscriptTimeline,
  createPlaybackController,
  createTestTransport,
  createTestTranscriptionProvider,
  createSessionManager,
} from "./session.js";
import type {
  TranscriptSegment,
  SessionParticipant,
  MediaTrack,
  SessionChangeType,
  TransportEvent,
} from "./session-types.js";

// ── Test Helpers ───────────────────────────────────────────────────────────

function seg(
  id: string,
  speakerId: string,
  text: string,
  startMs: number,
  endMs: number,
  isFinal = true,
): TranscriptSegment {
  return {
    id,
    speakerId,
    speakerName: `Speaker ${speakerId}`,
    text,
    startMs,
    endMs,
    confidence: 0.95,
    language: "en",
    isFinal,
  };
}

function alice(): SessionParticipant {
  return {
    id: "alice",
    displayName: "Alice",
    role: "speaker",
    activeMedia: [],
    muted: false,
    videoEnabled: false,
    joinedAt: new Date().toISOString(),
    canDelegate: true,
  };
}

function bob(): SessionParticipant {
  return {
    id: "bob",
    displayName: "Bob",
    role: "listener",
    activeMedia: [],
    muted: false,
    videoEnabled: false,
    joinedAt: new Date().toISOString(),
    canDelegate: false,
  };
}

function audioTrack(participantId: string, id = "audio-1"): MediaTrack {
  return {
    id,
    participantId,
    kind: "audio",
    state: "live",
    label: "Microphone",
    muted: false,
  };
}

function defaultConfig() {
  return {
    name: "Test Session",
    roomId: "room-1",
    localParticipant: { id: "host", displayName: "Host", role: "host" as const },
  };
}

// ── TranscriptTimeline ─────────────────────────────────────────────────────

describe("TranscriptTimeline", () => {
  it("starts empty", () => {
    const tl = createTranscriptTimeline();
    expect(tl.segments).toHaveLength(0);
    expect(tl.durationMs).toBe(0);
  });

  it("adds segments in sorted order", () => {
    const tl = createTranscriptTimeline();
    tl.addSegment(seg("s2", "a", "second", 2000, 4000));
    tl.addSegment(seg("s1", "a", "first", 0, 2000));
    tl.addSegment(seg("s3", "b", "third", 4000, 6000));

    expect(tl.segments.map(s => s.id)).toEqual(["s1", "s2", "s3"]);
  });

  it("computes duration from latest endMs", () => {
    const tl = createTranscriptTimeline();
    tl.addSegment(seg("s1", "a", "hello", 0, 3000));
    tl.addSegment(seg("s2", "b", "world", 2000, 5000));
    expect(tl.durationMs).toBe(5000);
  });

  it("updates non-final segments in place", () => {
    const tl = createTranscriptTimeline();
    tl.addSegment(seg("s1", "a", "hel", 0, 1000, false));
    tl.addSegment(seg("s1", "a", "hello world", 0, 2000, false));
    expect(tl.segments).toHaveLength(1);
    expect(tl.segments[0].text).toBe("hello world");
  });

  it("does not update finalized segments", () => {
    const tl = createTranscriptTimeline();
    tl.addSegment(seg("s1", "a", "original", 0, 1000, true));
    tl.addSegment(seg("s1", "a", "modified", 0, 1500, false));
    expect(tl.segments[0].text).toBe("original");
  });

  it("finalizes a segment", () => {
    const tl = createTranscriptTimeline();
    tl.addSegment(seg("s1", "a", "partial", 0, 1000, false));
    expect(tl.segments[0].isFinal).toBe(false);
    tl.finalizeSegment("s1");
    expect(tl.segments[0].isFinal).toBe(true);
  });

  it("getRange returns overlapping segments", () => {
    const tl = createTranscriptTimeline();
    tl.addSegment(seg("s1", "a", "one", 0, 2000));
    tl.addSegment(seg("s2", "a", "two", 2000, 4000));
    tl.addSegment(seg("s3", "a", "three", 4000, 6000));

    const range = tl.getRange(1500, 4500);
    expect(range.map(s => s.id)).toEqual(["s1", "s2", "s3"]);
  });

  it("getRange excludes non-overlapping", () => {
    const tl = createTranscriptTimeline();
    tl.addSegment(seg("s1", "a", "one", 0, 1000));
    tl.addSegment(seg("s2", "a", "two", 5000, 6000));

    const range = tl.getRange(2000, 4000);
    expect(range).toHaveLength(0);
  });

  it("getAtTime finds the segment at a given time", () => {
    const tl = createTranscriptTimeline();
    tl.addSegment(seg("s1", "a", "hello", 0, 2000));
    tl.addSegment(seg("s2", "b", "world", 2000, 4000));

    expect(tl.getAtTime(1000)?.id).toBe("s1");
    expect(tl.getAtTime(3000)?.id).toBe("s2");
    expect(tl.getAtTime(5000)).toBeNull();
  });

  it("search finds segments by text", () => {
    const tl = createTranscriptTimeline();
    tl.addSegment(seg("s1", "a", "Hello World", 0, 2000));
    tl.addSegment(seg("s2", "b", "Goodbye", 2000, 4000));
    tl.addSegment(seg("s3", "a", "Hello Again", 4000, 6000));

    const results = tl.search("hello");
    expect(results.map(s => s.id)).toEqual(["s1", "s3"]);
  });

  it("search is case-insensitive", () => {
    const tl = createTranscriptTimeline();
    tl.addSegment(seg("s1", "a", "TypeScript", 0, 2000));

    expect(tl.search("typescript")).toHaveLength(1);
    expect(tl.search("TYPESCRIPT")).toHaveLength(1);
  });

  it("toPlainText formats all segments", () => {
    const tl = createTranscriptTimeline();
    tl.addSegment(seg("s1", "a", "Hello", 0, 2000));
    tl.addSegment(seg("s2", "b", "World", 65000, 67000));

    const text = tl.toPlainText();
    expect(text).toContain("[00:00] Speaker a: Hello");
    expect(text).toContain("[01:05] Speaker b: World");
  });

  it("clear removes all segments", () => {
    const tl = createTranscriptTimeline();
    tl.addSegment(seg("s1", "a", "hello", 0, 1000));
    tl.addSegment(seg("s2", "b", "world", 1000, 2000));
    tl.clear();
    expect(tl.segments).toHaveLength(0);
    expect(tl.durationMs).toBe(0);
  });
});

// ── PlaybackController ─────────────────────────────────────────────────────

describe("PlaybackController", () => {
  it("starts at position 0, not playing", () => {
    const tl = createTranscriptTimeline();
    const pb = createPlaybackController(tl);
    expect(pb.positionMs).toBe(0);
    expect(pb.playing).toBe(false);
    expect(pb.speed).toBe(1.0);
  });

  it("play/pause toggles state", () => {
    const tl = createTranscriptTimeline();
    const pb = createPlaybackController(tl);
    pb.play();
    expect(pb.playing).toBe(true);
    pb.pause();
    expect(pb.playing).toBe(false);
  });

  it("seek updates position", () => {
    const tl = createTranscriptTimeline();
    const pb = createPlaybackController(tl);
    pb.seek(5000);
    expect(pb.positionMs).toBe(5000);
  });

  it("seek clamps to 0", () => {
    const tl = createTranscriptTimeline();
    const pb = createPlaybackController(tl);
    pb.seek(-100);
    expect(pb.positionMs).toBe(0);
  });

  it("setSpeed clamps to valid range", () => {
    const tl = createTranscriptTimeline();
    const pb = createPlaybackController(tl);
    pb.setSpeed(2.0);
    expect(pb.speed).toBe(2.0);
    pb.setSpeed(0.1);
    expect(pb.speed).toBe(0.25);
    pb.setSpeed(10.0);
    expect(pb.speed).toBe(4.0);
  });

  it("seekToSegment jumps to segment start", () => {
    const tl = createTranscriptTimeline();
    tl.addSegment(seg("s1", "a", "hello", 5000, 7000));
    const pb = createPlaybackController(tl);
    pb.seekToSegment("s1");
    expect(pb.positionMs).toBe(5000);
  });

  it("seekToSegment no-ops for unknown segment", () => {
    const tl = createTranscriptTimeline();
    const pb = createPlaybackController(tl);
    pb.seek(1000);
    pb.seekToSegment("nonexistent");
    expect(pb.positionMs).toBe(1000);
  });

  it("notifies listeners on seek", () => {
    const tl = createTranscriptTimeline();
    const pb = createPlaybackController(tl);
    const positions: number[] = [];
    pb.onPositionChange(p => positions.push(p));
    pb.seek(3000);
    pb.seek(5000);
    expect(positions).toEqual([3000, 5000]);
  });

  it("unsubscribe stops notifications", () => {
    const tl = createTranscriptTimeline();
    const pb = createPlaybackController(tl);
    const positions: number[] = [];
    const unsub = pb.onPositionChange(p => positions.push(p));
    pb.seek(1000);
    unsub();
    pb.seek(2000);
    expect(positions).toEqual([1000]);
  });
});

// ── TestTransport ──────────────────────────────────────────────────────────

describe("TestTransport", () => {
  it("starts disconnected", () => {
    const t = createTestTransport();
    expect(t.connected).toBe(false);
    expect(t.kind).toBe("test");
  });

  it("connect/disconnect lifecycle", async () => {
    const t = createTestTransport();
    const events: TransportEvent[] = [];
    t.onEvent(e => events.push(e));

    await t.connect("room-1", "token-abc");
    expect(t.connected).toBe(true);
    expect(events.some(e => e.type === "connected")).toBe(true);

    await t.disconnect();
    expect(t.connected).toBe(false);
    expect(events.some(e => e.type === "disconnected")).toBe(true);
  });

  it("publish/unpublish tracks", async () => {
    const t = createTestTransport();
    await t.connect("room-1", "token");
    const events: TransportEvent[] = [];
    t.onEvent(e => events.push(e));

    const track = audioTrack("alice");
    await t.publishTrack(track);
    expect(events.some(e => e.type === "track-published")).toBe(true);

    await t.unpublishTrack(track.id);
    expect(events.some(e => e.type === "track-unpublished")).toBe(true);
  });

  it("dispose cleans up", async () => {
    const t = createTestTransport();
    await t.connect("room-1", "token");
    const events: TransportEvent[] = [];
    t.onEvent(e => events.push(e));

    await t.dispose();
    expect(t.connected).toBe(false);
  });

  it("unsubscribe stops events", async () => {
    const t = createTestTransport();
    const events: TransportEvent[] = [];
    const unsub = t.onEvent(e => events.push(e));
    unsub();

    await t.connect("room-1", "token");
    expect(events).toHaveLength(0);
  });
});

// ── TestTranscriptionProvider ──────────────────────────────────────────────

describe("TestTranscriptionProvider", () => {
  it("feeds segments to listeners when running", async () => {
    const tp = createTestTranscriptionProvider();
    const segments: TranscriptSegment[] = [];
    tp.onSegment(s => segments.push(s));

    await tp.start({ language: "en" });
    tp.feedSegment(seg("s1", "a", "hello", 0, 1000));
    expect(segments).toHaveLength(1);

    await tp.stop();
    tp.feedSegment(seg("s2", "a", "ignored", 1000, 2000));
    expect(segments).toHaveLength(1);
  });

  it("is always available", async () => {
    const tp = createTestTranscriptionProvider();
    expect(await tp.isAvailable()).toBe(true);
  });

  it("name is test", () => {
    const tp = createTestTranscriptionProvider();
    expect(tp.name).toBe("test");
  });

  it("unsubscribe stops segment delivery", async () => {
    const tp = createTestTranscriptionProvider();
    const segments: TranscriptSegment[] = [];
    const unsub = tp.onSegment(s => segments.push(s));

    await tp.start({});
    tp.feedSegment(seg("s1", "a", "one", 0, 1000));
    unsub();
    tp.feedSegment(seg("s2", "a", "two", 1000, 2000));
    expect(segments).toHaveLength(1);
  });
});

// ── SessionManager ─────────────────────────────────────────────────────────

describe("SessionManager", () => {
  it("starts idle with no config", () => {
    const sm = createSessionManager();
    expect(sm.status).toBe("idle");
    expect(sm.config).toBeNull();
    expect(sm.participants).toHaveLength(0);
  });

  it("create sets status to active and adds local participant", () => {
    const sm = createSessionManager();
    sm.create(defaultConfig());
    expect(sm.status).toBe("active");
    expect(sm.config?.name).toBe("Test Session");
    expect(sm.participants).toHaveLength(1);
    expect(sm.participants[0].id).toBe("host");
    expect(sm.participants[0].role).toBe("host");
  });

  it("join sets status to active", () => {
    const sm = createSessionManager();
    sm.join({
      name: "Existing Session",
      roomId: "room-2",
      localParticipant: { id: "guest", displayName: "Guest", role: "listener" },
    });
    expect(sm.status).toBe("active");
    expect(sm.participants[0].role).toBe("listener");
  });

  it("throws when creating while active", () => {
    const sm = createSessionManager();
    sm.create(defaultConfig());
    expect(() => sm.create(defaultConfig())).toThrow("already active");
  });

  it("end sets status to ended", () => {
    const sm = createSessionManager();
    sm.create(defaultConfig());
    sm.end();
    expect(sm.status).toBe("ended");
  });

  it("pause/resume cycle", () => {
    const sm = createSessionManager();
    sm.create(defaultConfig());
    sm.pause();
    expect(sm.status).toBe("paused");
    sm.resume();
    expect(sm.status).toBe("active");
  });

  it("pause is no-op when not active", () => {
    const sm = createSessionManager();
    sm.create(defaultConfig());
    sm.end();
    sm.pause(); // should not throw
    expect(sm.status).toBe("ended");
  });
});

describe("SessionManager participants", () => {
  it("adds and removes participants", () => {
    const sm = createSessionManager();
    sm.create(defaultConfig());
    sm.addParticipant(alice());
    sm.addParticipant(bob());
    expect(sm.participants).toHaveLength(3); // host + alice + bob

    sm.removeParticipant("bob");
    expect(sm.participants).toHaveLength(2);
    expect(sm.participants.find(p => p.id === "bob")).toBeUndefined();
  });

  it("duplicate participant is no-op", () => {
    const sm = createSessionManager();
    sm.create(defaultConfig());
    sm.addParticipant(alice());
    sm.addParticipant(alice());
    expect(sm.participants.filter(p => p.id === "alice")).toHaveLength(1);
  });

  it("enforces maxParticipants", () => {
    const sm = createSessionManager({ maxParticipants: 2 });
    sm.create(defaultConfig()); // adds host (1/2)
    sm.addParticipant(alice()); // 2/2
    expect(() => sm.addParticipant(bob())).toThrow("Maximum participants");
  });

  it("setParticipantRole updates role", () => {
    const sm = createSessionManager();
    sm.create(defaultConfig());
    sm.addParticipant(bob());
    sm.setParticipantRole("bob", "speaker");
    const p = sm.participants.find(p => p.id === "bob");
    expect(p?.role).toBe("speaker");
  });

  it("setParticipantMuted updates muted state", () => {
    const sm = createSessionManager();
    sm.create(defaultConfig());
    sm.addParticipant(alice());
    sm.setParticipantMuted("alice", true);
    const p = sm.participants.find(p => p.id === "alice");
    expect(p?.muted).toBe(true);
  });

  it("removing participant removes their tracks", () => {
    const sm = createSessionManager();
    sm.create(defaultConfig());
    sm.addParticipant(alice());
    sm.addTrack(audioTrack("alice", "a-audio"));
    expect(sm.tracks).toHaveLength(1);

    sm.removeParticipant("alice");
    expect(sm.tracks).toHaveLength(0);
  });
});

describe("SessionManager media tracks", () => {
  it("adds and removes tracks", () => {
    const sm = createSessionManager();
    sm.create(defaultConfig());
    sm.addTrack(audioTrack("host", "track-1"));
    expect(sm.tracks).toHaveLength(1);

    sm.removeTrack("track-1");
    expect(sm.tracks).toHaveLength(0);
  });

  it("updates participant activeMedia on track add", () => {
    const sm = createSessionManager();
    sm.create(defaultConfig());
    sm.addTrack(audioTrack("host", "track-1"));
    const host = sm.participants.find(p => p.id === "host");
    expect(host?.activeMedia).toContain("audio");
  });

  it("updates participant activeMedia on track remove", () => {
    const sm = createSessionManager();
    sm.create(defaultConfig());
    sm.addTrack(audioTrack("host", "track-1"));
    sm.removeTrack("track-1");
    const host = sm.participants.find(p => p.id === "host");
    expect(host?.activeMedia).not.toContain("audio");
  });

  it("setTrackMuted updates track state", () => {
    const sm = createSessionManager();
    sm.create(defaultConfig());
    sm.addTrack(audioTrack("host", "track-1"));
    sm.setTrackMuted("track-1", true);
    expect(sm.tracks[0].muted).toBe(true);
  });

  it("duplicate track add is no-op", () => {
    const sm = createSessionManager();
    sm.create(defaultConfig());
    sm.addTrack(audioTrack("host", "track-1"));
    sm.addTrack(audioTrack("host", "track-1"));
    expect(sm.tracks).toHaveLength(1);
  });
});

describe("SessionManager transcript integration", () => {
  it("provides transcript timeline", () => {
    const sm = createSessionManager();
    sm.create(defaultConfig());
    sm.transcript.addSegment(seg("s1", "host", "Hello everyone", 0, 2000));
    expect(sm.transcript.segments).toHaveLength(1);
  });

  it("provides playback controller linked to transcript", () => {
    const sm = createSessionManager();
    sm.create(defaultConfig());
    sm.transcript.addSegment(seg("s1", "host", "Hello", 5000, 7000));
    sm.playback.seekToSegment("s1");
    expect(sm.playback.positionMs).toBe(5000);
  });

  it("pause pauses playback", () => {
    const sm = createSessionManager();
    sm.create(defaultConfig());
    sm.playback.play();
    expect(sm.playback.playing).toBe(true);
    sm.pause();
    expect(sm.playback.playing).toBe(false);
  });
});

describe("SessionManager delegation", () => {
  it("creates a delegation request targeting a capable peer", () => {
    const sm = createSessionManager();
    sm.create(defaultConfig());
    sm.addParticipant(alice()); // alice.canDelegate = true

    const req = sm.requestDelegation("transcription", { language: "en" });
    expect(req.taskType).toBe("transcription");
    expect(req.requesterId).toBe("host");
    expect(req.delegateeId).toBe("alice");
    expect(req.status).toBe("pending");
  });

  it("rejects delegation when no capable peer exists", () => {
    const sm = createSessionManager();
    sm.create(defaultConfig());
    sm.addParticipant(bob()); // bob.canDelegate = false

    const req = sm.requestDelegation("ai-inference", {});
    expect(req.status).toBe("rejected");
    expect(req.delegateeId).toBeNull();
  });

  it("respondToDelegation accepts a request", () => {
    const sm = createSessionManager();
    sm.create(defaultConfig());
    sm.addParticipant(alice());

    const req = sm.requestDelegation("transcription", {});
    sm.respondToDelegation(req.id, true);
    expect(sm.delegations[0].status).toBe("accepted");
  });

  it("respondToDelegation rejects a request", () => {
    const sm = createSessionManager();
    sm.create(defaultConfig());
    sm.addParticipant(alice());

    const req = sm.requestDelegation("transcription", {});
    sm.respondToDelegation(req.id, false);
    expect(sm.delegations[0].status).toBe("rejected");
  });

  it("cannot respond to already resolved delegation", () => {
    const sm = createSessionManager();
    sm.create(defaultConfig());
    sm.addParticipant(alice());

    const req = sm.requestDelegation("transcription", {});
    sm.respondToDelegation(req.id, true);
    sm.respondToDelegation(req.id, false); // should be no-op
    expect(sm.delegations[0].status).toBe("accepted");
  });
});

describe("SessionManager events", () => {
  it("emits status-changed on create", () => {
    const sm = createSessionManager();
    const changes: SessionChangeType[] = [];
    sm.onChange(c => changes.push(c));
    sm.create(defaultConfig());
    expect(changes.some(c => c.type === "status-changed")).toBe(true);
  });

  it("emits participant-joined on addParticipant", () => {
    const sm = createSessionManager();
    sm.create(defaultConfig());
    const changes: SessionChangeType[] = [];
    sm.onChange(c => changes.push(c));
    sm.addParticipant(alice());
    expect(changes.some(c => c.type === "participant-joined")).toBe(true);
  });

  it("emits participant-left on removeParticipant", () => {
    const sm = createSessionManager();
    sm.create(defaultConfig());
    sm.addParticipant(alice());
    const changes: SessionChangeType[] = [];
    sm.onChange(c => changes.push(c));
    sm.removeParticipant("alice");
    expect(changes.some(c => c.type === "participant-left")).toBe(true);
  });

  it("emits track-changed on addTrack", () => {
    const sm = createSessionManager();
    sm.create(defaultConfig());
    const changes: SessionChangeType[] = [];
    sm.onChange(c => changes.push(c));
    sm.addTrack(audioTrack("host"));
    expect(changes.some(c => c.type === "track-changed")).toBe(true);
  });

  it("emits delegation-updated on delegation", () => {
    const sm = createSessionManager();
    sm.create(defaultConfig());
    sm.addParticipant(alice());
    const changes: SessionChangeType[] = [];
    sm.onChange(c => changes.push(c));
    sm.requestDelegation("task", {});
    expect(changes.some(c => c.type === "delegation-updated")).toBe(true);
  });

  it("unsubscribe stops events", () => {
    const sm = createSessionManager();
    const changes: SessionChangeType[] = [];
    const unsub = sm.onChange(c => changes.push(c));
    unsub();
    sm.create(defaultConfig());
    expect(changes).toHaveLength(0);
  });
});

describe("SessionManager dispose", () => {
  it("clears all state", () => {
    const sm = createSessionManager();
    sm.create(defaultConfig());
    sm.addParticipant(alice());
    sm.addTrack(audioTrack("host"));
    sm.transcript.addSegment(seg("s1", "host", "test", 0, 1000));
    sm.requestDelegation("task", {});

    sm.dispose();
    expect(sm.status).toBe("ended");
    expect(sm.participants).toHaveLength(0);
    expect(sm.tracks).toHaveLength(0);
    expect(sm.delegations).toHaveLength(0);
    expect(sm.transcript.segments).toHaveLength(0);
  });
});
