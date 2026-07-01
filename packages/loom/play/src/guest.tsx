//! The guest (participant) app — a Discord/Telegram-style inbox of
//! conversation threads over the live story. Composed entirely from the
//! shared chat primitives; this file only supplies the registration screen,
//! the inbox header, and a profile sheet for out-of-band actions.

import { useState } from "react";
import { ActionRow, ChannelView, ConnDot, FactionPill, InviteSheet, MessageThread, SpaceList } from "./chat.tsx";
import { useGuestSession, type GuestSession } from "./session.ts";
import type { Action } from "./types.ts";

/** The event code can ride in on a `?code=` link (e.g. a scanned QR). */
function codeFromUrl(): string {
  try {
    return new URLSearchParams(window.location.search).get("code") ?? "";
  } catch {
    return "";
  }
}

function GuestRegister({ session }: { session: GuestSession }) {
  const [name, setName] = useState("");
  const [code, setCode] = useState(codeFromUrl);
  const [err, setErr] = useState("");
  const go = async () => {
    try {
      await session.register(name.trim() || "Guest", code.trim());
    } catch (e) {
      setErr((e as Error).message);
    }
  };
  return (
    <div className="hero">
      <div className="glyph">🌐</div>
      <h1>Escape the Internet</h1>
      <p className="sub">Log on. Choose a side. Try not to get captured.</p>
      <input
        placeholder="What do they call you?"
        value={name}
        onChange={(e) => setName(e.target.value)}
        onKeyDown={(e) => e.key === "Enter" && go()}
      />
      <input
        placeholder="Event code (from the host)"
        value={code}
        autoCapitalize="characters"
        autoCorrect="off"
        spellCheck={false}
        onChange={(e) => setCode(e.target.value)}
        onKeyDown={(e) => e.key === "Enter" && go()}
      />
      <button className="choice primary" onClick={go}>
        Enter the internet →
      </button>
      {err && <div className="err">{err}</div>}
    </div>
  );
}

/** Out-of-band actions: show your pass, defect, or leave. */
function ProfileSheet({ session, onClose, onLeave }: { session: GuestSession; onClose: () => void; onLeave: () => void }) {
  const faction = session.status?.faction ?? null;
  const other = faction === "Mods" ? "Chatters" : "Mods";
  return (
    <div className="sheet-backdrop" onClick={onClose}>
      <div className="sheet" onClick={(e) => e.stopPropagation()}>
        <h2>Your pass</h2>
        {session.me && (
          <div className="pass">
            <img alt="QR" src={`/api/qr?text=${encodeURIComponent(session.me.id)}`} />
            <div className="bigid">{session.me.id}</div>
            <div className="muted">Show this to a performer to be scanned.</div>
          </div>
        )}
        {faction && (
          <button className="choice ghost" onClick={() => void session.defect(other)}>
            Betray the {faction} → defect to {other}
          </button>
        )}
        <button
          className="choice danger"
          onClick={() => {
            session.leave();
            onLeave();
          }}
        >
          Leave the event
        </button>
        <button className="link" onClick={onClose}>
          Close
        </button>
      </div>
    </div>
  );
}

export function GuestApp({ onLeave }: { onLeave: () => void }) {
  const s = useGuestSession();
  const [profile, setProfile] = useState(false);
  const [inviting, setInviting] = useState(false);
  if (!s.me) return <GuestRegister session={s} />;

  const st = s.status;
  const captured = st?.captured ?? false;
  const t = s.threads;

  if (t.active) {
    const active = t.active;
    if (t.activeThreadRoot !== null) {
      return (
        <MessageThread
          channel={active}
          rootSeq={t.activeThreadRoot}
          onClose={t.closeThread}
          onSend={(text, parentSeq) => void s.say(active.id, text, parentSeq)}
        />
      );
    }
    // A membership-gated room the guest belongs to can invite + leave.
    const gated = active.kind === "private" || active.kind === "group" || active.kind === "dm";
    const roomActions: Action[] = [];
    if (gated && active.member) {
      roomActions.push({ label: "＋ Invite", onClick: () => setInviting(true), tone: "primary" });
      roomActions.push({ label: "🚪 Leave", onClick: () => void s.leaveChannel(active.id), tone: "danger" });
    }
    return (
      <>
        <ChannelView
          channel={active}
          onBack={t.back}
          onOpenThread={t.openThread}
          onSend={(text) => void s.say(active.id, text)}
          footer={roomActions.length > 0 ? <ActionRow actions={roomActions} /> : undefined}
        />
        {inviting && (
          <InviteSheet
            title={`Invite to ${active.title}`}
            people={st?.roster ?? []}
            onPick={(id) => void s.inviteToChannel(id, active.id)}
            onClose={() => setInviting(false)}
          />
        )}
      </>
    );
  }

  const header = (
    <header className={`inbox-head ${captured ? "trapped" : ""}`}>
      <div className="who">
        <strong>{s.me.name}</strong> <FactionPill faction={st?.faction ?? null} />
      </div>
      <div className="hud">
        <span>⭐ {st?.score ?? 0}</span>
        <span>{captured ? "🔒 captured" : `📍 ${st?.location ?? "the party"}`}</span>
        <ConnDot connected={s.connected} label="live" />
        <button className="icon-btn" title="Profile" onClick={() => setProfile(true)}>
          ☰
        </button>
      </div>
    </header>
  );

  return (
    <div className={captured ? "trapped-bg" : ""}>
      <SpaceList spaces={t.spaces} onOpen={t.open} header={header} empty="The internet is quiet… for now." />
      {profile && <ProfileSheet session={s} onClose={() => setProfile(false)} onLeave={onLeave} />}
    </div>
  );
}
