//! The performer (character) app — the same threaded chat surface as the
//! guest, recomposed for the booth: each guest is a conversation the
//! performer can scan into, the broadcast feed and every guest thread are
//! moderatable by an admin, and a pinned Scanner thread holds the camera +
//! scan readouts.

import { useEffect, useRef, useState } from "react";
import { ActionRow, ChannelView, ConnDot, FactionPill, InviteSheet, MessageThread, SpaceList } from "./chat.tsx";
import { usePrimeSession, type PrimeSession } from "./session.ts";
import type { Action, Channel } from "./types.ts";

function PrimeLogin({ session }: { session: PrimeSession }) {
  const [character, setCharacter] = useState("");
  const [passcode, setPasscode] = useState("");
  const [err, setErr] = useState("");
  const go = async () => {
    try {
      await session.login(character.trim(), passcode);
    } catch (e) {
      setErr((e as Error).message);
    }
  };
  return (
    <div className="hero">
      <div className="glyph">🎭</div>
      <h1>Performer station</h1>
      <p className="sub">Sign in as your character to scan guests and deliver the story.</p>
      <input placeholder="Character (e.g. Moderator_Prime)" value={character} onChange={(e) => setCharacter(e.target.value)} />
      <input type="password" placeholder="Performer passcode (from the host)" value={passcode} onChange={(e) => setPasscode(e.target.value)} />
      <button className="choice primary" onClick={go}>
        Sign in
      </button>
      {err && <div className="err">{err}</div>}
    </div>
  );
}

function Scanner({ onScan }: { onScan: (id: string) => void }) {
  const [manual, setManual] = useState("");
  const [camOn, setCamOn] = useState(false);
  const [err, setErr] = useState("");
  const videoRef = useRef<HTMLVideoElement>(null);

  useEffect(() => {
    if (!camOn) return;
    let stop = () => {};
    let raf = 0;
    (async () => {
      const Detector = (window as unknown as { BarcodeDetector?: unknown }).BarcodeDetector;
      if (!Detector) {
        setErr("Camera scanning isn't supported on this device — type the id.");
        setCamOn(false);
        return;
      }
      try {
        const det = new (Detector as new (o: unknown) => { detect: (v: unknown) => Promise<Array<{ rawValue: string }>> })({ formats: ["qr_code"] });
        const stream = await navigator.mediaDevices.getUserMedia({ video: { facingMode: "environment" } });
        const v = videoRef.current!;
        v.srcObject = stream;
        await v.play();
        stop = () => stream.getTracks().forEach((t) => t.stop());
        const tick = async () => {
          try {
            const codes = await det.detect(v);
            if (codes[0]) {
              onScan(codes[0].rawValue);
              setCamOn(false);
              return;
            }
          } catch {
            /* frame skip */
          }
          raf = requestAnimationFrame(tick);
        };
        tick();
      } catch (e) {
        setErr("Camera error: " + (e as Error).message);
        setCamOn(false);
      }
    })();
    return () => {
      cancelAnimationFrame(raf);
      stop();
    };
  }, [camOn, onScan]);

  return (
    <div className="card">
      <input placeholder="Guest id (e.g. g-1a2b3c)" value={manual} onChange={(e) => setManual(e.target.value)} />
      <button className="choice primary" onClick={() => onScan(manual.trim())}>
        Scan
      </button>
      <button className="choice ghost" onClick={() => setCamOn((v) => !v)}>
        {camOn ? "Stop camera" : "📷 Use camera"}
      </button>
      {camOn && <video ref={videoRef} playsInline className="cam" />}
      {err && <div className="err">{err}</div>}
    </div>
  );
}

/** The pinned Scanner thread: camera/manual entry + the live scan readouts. */
function ScannerScreen({ session, onBack }: { session: PrimeSession; onBack: () => void }) {
  const [err, setErr] = useState("");
  const doScan = (id: string) => {
    if (!id) return;
    setErr("");
    void session.scan(id).catch((e) => setErr((e as Error).message));
  };
  return (
    <div className="screen">
      <header className="thread-head">
        <button className="back" onClick={onBack}>
          ‹
        </button>
        <div className="avatar scanner">📷</div>
        <div className="thread-id">
          <strong>Scanner</strong>
          <span className="muted">scan a guest's pass</span>
        </div>
      </header>
      <div className="station">
        <Scanner onScan={doScan} />
        {err && <div className="err">{err}</div>}
        <div className="card">
          <h2>Readouts</h2>
          {session.responses.length === 0 ? (
            <div className="muted">Scan a guest to deliver their beat.</div>
          ) : (
            session.responses.map((r) => (
              <div key={r.id} className="readout">
                🖥️ {r.text}
              </div>
            ))
          )}
        </div>
      </div>
    </div>
  );
}

/** Inline "elevate me to admin" + sign-out sheet. */
function PerformerSheet({ session, onClose, onLeave }: { session: PrimeSession; onClose: () => void; onLeave: () => void }) {
  const [pass, setPass] = useState("");
  const [err, setErr] = useState("");
  return (
    <div className="sheet-backdrop" onClick={onClose}>
      <div className="sheet" onClick={(e) => e.stopPropagation()}>
        {!session.auth?.admin && (
          <>
            <h2>Add moderator powers</h2>
            <input type="password" placeholder="Moderator passcode" value={pass} onChange={(e) => setPass(e.target.value)} />
            <button
              className="choice primary"
              onClick={async () => {
                try {
                  await session.becomeAdmin(pass);
                  onClose();
                } catch (e) {
                  setErr((e as Error).message);
                }
              }}
            >
              Become an admin
            </button>
            {err && <div className="err">{err}</div>}
          </>
        )}
        {session.auth?.admin && <div className="muted">You hold moderator powers — hide/show messages and capture/release from any guest thread.</div>}
        <button
          className="choice danger"
          onClick={() => {
            session.leave();
            onLeave();
          }}
        >
          Sign out
        </button>
        <button className="link" onClick={onClose}>
          Close
        </button>
      </div>
    </div>
  );
}

export function PerformerApp({ onLeave }: { onLeave: () => void }) {
  const s = usePrimeSession();
  const [sheet, setSheet] = useState(false);
  if (!s.auth) return <PrimeLogin session={s} />;
  const admin = s.auth.admin;
  const t = s.threads;

  if (t.active) {
    const active = t.active;
    if (active.id === "__scanner") return <ScannerScreen session={s} onBack={t.back} />;
    const mod = admin ? { setHidden: (seq: number, hidden: boolean) => void s.setHidden(seq, hidden) } : undefined;
    // The broadcast feed posts to the room (lobby); guest threads keep their id
    // (the server maps `guest:<id>` to this character's DM with that guest).
    const postChannel = active.id === "__feed" ? "lobby" : active.id;
    if (t.activeThreadRoot !== null) {
      return (
        <MessageThread
          channel={active}
          rootSeq={t.activeThreadRoot}
          onClose={t.closeThread}
          moderate={mod}
          onSend={(text, parentSeq) => void s.say(postChannel, text, parentSeq)}
        />
      );
    }
    if (active.id === "__feed") {
      return (
        <ChannelView
          channel={active}
          onBack={t.back}
          moderate={mod}
          onOpenThread={t.openThread}
          onSend={(text) => void s.say("lobby", text)}
        />
      );
    }
    if (active.id.startsWith("room:")) {
      return <RoomThread session={s} channel={active} admin={admin} onBack={t.back} onOpenThread={t.openThread} />;
    }
    return <GuestThread session={s} channel={active} admin={admin} onBack={t.back} onOpenThread={t.openThread} />;
  }

  const header = (
    <header className="inbox-head">
      <div className="who">
        🎭 <strong>{s.auth.character}</strong> {admin && <span className="pill Mods">admin</span>}{" "}
        <FactionPill faction={s.view?.faction ?? null} />
      </div>
      <div className="hud">
        <ConnDot connected={s.connected} label="live" />
        <button className="icon-btn" title="Settings" onClick={() => setSheet(true)}>
          ☰
        </button>
      </div>
    </header>
  );

  return (
    <>
      <SpaceList spaces={t.spaces} onOpen={t.open} header={header} empty="No guests yet." />
      {sheet && <PerformerSheet session={s} onClose={() => setSheet(false)} onLeave={onLeave} />}
    </>
  );
}

/** An authored channel (SPACE/CHANNEL): post + a guest invite picker + leave. */
function RoomThread({
  session,
  channel,
  admin,
  onBack,
  onOpenThread,
}: {
  session: PrimeSession;
  channel: Channel;
  admin: boolean;
  onBack: () => void;
  onOpenThread?: (rootSeq: number) => void;
}) {
  const [picking, setPicking] = useState(false);
  const gated = channel.kind === "private" || channel.kind === "group" || channel.kind === "dm";
  const actions: Action[] = [];
  if (gated) actions.push({ label: "＋ Invite a guest", onClick: () => setPicking(true), tone: "primary" });
  if (gated && channel.member) actions.push({ label: "🚪 Leave", onClick: () => void session.leaveChannel(channel.id) });
  return (
    <>
      <ChannelView
        channel={channel}
        onBack={onBack}
        onOpenThread={onOpenThread}
        moderate={admin ? { setHidden: (seq, hidden) => void session.setHidden(seq, hidden) } : undefined}
        onSend={(text) => void session.say(channel.id, text)}
        footer={actions.length > 0 ? <ActionRow actions={actions} /> : undefined}
      />
      {picking && (
        <InviteSheet
          title={`Invite to ${channel.title}`}
          people={(session.view?.guests ?? []).map((g) => ({ id: g.id, name: g.name }))}
          onPick={(id) => void session.inviteToChannel(id, channel.id)}
          onClose={() => setPicking(false)}
        />
      )}
    </>
  );
}

/** One guest's thread, with scan + (admin) capture/release + moderation. */
function GuestThread({
  session,
  channel,
  admin,
  onBack,
  onOpenThread,
}: {
  session: PrimeSession;
  channel: Channel;
  admin: boolean;
  onBack: () => void;
  onOpenThread?: (rootSeq: number) => void;
}) {
  const gid = channel.id.slice("guest:".length);
  const guest = session.view?.guests.find((g) => g.id === gid);
  const actions: Action[] = [{ label: "📡 Scan this guest", onClick: () => void session.scan(gid), tone: "primary" }];
  if (admin) {
    actions.push(
      guest?.captured
        ? { label: "🔓 Release", onClick: () => void session.moderate(gid, "release") }
        : { label: "🔒 Capture", onClick: () => void session.moderate(gid, "capture"), tone: "danger" },
    );
  }
  return (
    <ChannelView
      channel={channel}
      onBack={onBack}
      showChannel
      moderate={admin ? { setHidden: (seq, hidden) => void session.setHidden(seq, hidden) } : undefined}
      footer={<ActionRow actions={actions} />}
      onOpenThread={onOpenThread}
      onSend={(text) => void session.say(channel.id, text)}
    />
  );
}
