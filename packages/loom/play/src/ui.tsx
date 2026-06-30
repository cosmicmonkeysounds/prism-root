//! React UI for the participant app — the guest's branching dialogue
//! experience and the performer's scanner station.

import { useEffect, useRef, useState } from "react";
import {
  cueText,
  useGuestSession,
  usePrimeSession,
  type GuestSession,
  type PrimeSession,
} from "./session.ts";
import type { Beat } from "./types.ts";

// ---------------------------------------------------------------------
// Shared bits
// ---------------------------------------------------------------------

function FactionPill({ faction }: { faction: string | null }) {
  const f = faction ?? "none";
  return <span className={`pill ${f}`}>{faction ?? "unaligned"}</span>;
}

function ConnDot({ connected, label }: { connected: boolean; label: string }) {
  return (
    <span className="conn">
      <span className={`dot ${connected ? "on" : ""}`} /> {label}
    </span>
  );
}

interface Option {
  label: string;
  onClick: () => void;
  tone?: "primary" | "danger" | "ghost";
}

/** The branching-dialogue decision tray (game choice UI). */
function ChoiceTray({ title, options }: { title: string; options: Option[] }) {
  return (
    <div className="tray">
      <div className="tray-title">{title}</div>
      {options.map((o, i) => (
        <button key={i} className={`choice ${o.tone ?? "primary"}`} onClick={o.onClick}>
          {o.label}
        </button>
      ))}
    </div>
  );
}

function StoryFeed({ story }: { story: Beat[] }) {
  const end = useRef<HTMLDivElement>(null);
  useEffect(() => {
    end.current?.scrollIntoView({ behavior: "smooth", block: "end" });
  }, [story]);
  return (
    <div className="story">
      {story.map((b) => (
        <BeatView key={b.id} beat={b} />
      ))}
      <div ref={end} />
    </div>
  );
}

function BeatView({ beat }: { beat: Beat }) {
  switch (beat.kind) {
    case "narration":
      return <div className="beat narration">{beat.text}</div>;
    case "system":
      return <div className="beat system">{beat.text}</div>;
    case "signal":
      return <div className="beat signal">{cueText(beat.cue)}</div>;
    case "line":
      return (
        <div className="beat line">
          <div className="speaker">{beat.speaker}</div>
          <div className="bubble">{beat.text}</div>
        </div>
      );
  }
}

// ---------------------------------------------------------------------
// Guest experience
// ---------------------------------------------------------------------

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

function GuestApp({ onLeave }: { onLeave: () => void }) {
  const s = useGuestSession();
  const [showPass, setShowPass] = useState(false);
  if (!s.me) return <GuestRegister session={s} />;
  const st = s.status;
  const captured = st?.captured ?? false;

  const tray = (() => {
    if (!st) return null;
    if (st.pendingChoice) {
      return (
        <ChoiceTray
          title="A decision…"
          options={st.pendingChoice.map((opt, i) => ({ label: opt, onClick: () => void s.choose(i) }))}
        />
      );
    }
    if (!st.faction) {
      return (
        <ChoiceTray
          title="Choose your side"
          options={[
            { label: "🛡️ Side with the Mods", onClick: () => void s.join("Mods") },
            { label: "💬 Side with the Chatters", onClick: () => void s.join("Chatters"), tone: "ghost" },
          ]}
        />
      );
    }
    if (captured) {
      return (
        <ChoiceTray
          title="You're trapped in the Internet"
          options={[{ label: "🏃 Make a break for it", onClick: () => void s.escape(), tone: "danger" }]}
        />
      );
    }
    const other = st.faction === "Mods" ? "Chatters" : "Mods";
    return (
      <ChoiceTray
        title="Lay low and watch the room"
        options={[{ label: `Betray the ${st.faction} → defect to ${other}`, onClick: () => void s.defect(other), tone: "ghost" }]}
      />
    );
  })();

  return (
    <div className={`screen ${captured ? "trapped" : ""}`}>
      <header>
        <div className="who">
          <strong>{s.me.name}</strong> <FactionPill faction={st?.faction ?? null} />
        </div>
        <div className="hud">
          <span>⭐ {st?.score ?? 0}</span>
          <span>{captured ? "🔒 captured" : `📍 ${st?.location ?? "the party"}`}</span>
          <ConnDot connected={s.connected} label="live" />
        </div>
      </header>

      <StoryFeed story={s.story} />

      <footer>
        {tray}
        <div className="footrow">
          <button className="link" onClick={() => setShowPass((v) => !v)}>
            {showPass ? "Hide pass" : "Show my pass"}
          </button>
          <button className="link" onClick={() => { s.leave(); onLeave(); }}>
            Leave
          </button>
        </div>
        {showPass && s.me && (
          <div className="pass">
            <img alt="QR" src={`/api/qr?text=${encodeURIComponent(s.me.id)}`} />
            <div className="bigid">{s.me.id}</div>
            <div className="muted">Show this to a performer to be scanned.</div>
          </div>
        )}
      </footer>
    </div>
  );
}

// ---------------------------------------------------------------------
// Performer (character) station
// ---------------------------------------------------------------------

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
      <h2>Scan a guest</h2>
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

function PerformerApp({ onLeave }: { onLeave: () => void }) {
  const s = usePrimeSession();
  const [scanErr, setScanErr] = useState("");
  if (!s.auth) return <PrimeLogin session={s} />;
  const doScan = (id: string) => {
    if (!id) return;
    setScanErr("");
    void s.scan(id).catch((e) => setScanErr((e as Error).message));
  };
  return (
    <div className="screen">
      <header>
        <div className="who">
          🎭 <strong>{s.auth.character}</strong> <FactionPill faction={s.view?.faction ?? null} />
        </div>
        <ConnDot connected={s.connected} label="live" />
      </header>

      <main className="station">
        <Scanner onScan={doScan} />
        {scanErr && <div className="err">{scanErr}</div>}

        <div className="card">
          <h2>Your scanner shows</h2>
          {s.responses.length === 0 ? (
            <div className="muted">Scan a guest to deliver their beat.</div>
          ) : (
            s.responses.map((r) => (
              <div key={r.id} className="readout">
                🖥️ {r.text}
              </div>
            ))
          )}
        </div>

        <div className="card">
          <h2>Guests present</h2>
          {!s.view?.guests.length ? (
            <div className="muted">No guests yet.</div>
          ) : (
            s.view.guests.map((g) => (
              <button key={g.id} className="rowbtn" onClick={() => doScan(g.id)}>
                <span>
                  {g.name} {g.captured ? "🔒" : ""}
                </span>
                <span className="muted">{g.id}</span>
              </button>
            ))
          )}
        </div>
      </main>

      <footer>
        <button className="link" onClick={() => { s.leave(); onLeave(); }}>
          Sign out
        </button>
      </footer>
    </div>
  );
}

// ---------------------------------------------------------------------
// Root — role selection
// ---------------------------------------------------------------------

type Role = "none" | "guest" | "prime";

export function App() {
  const [role, setRole] = useState<Role>(() => {
    if (localStorage.getItem("loom.guest")) return "guest";
    if (localStorage.getItem("loom.prime")) return "prime";
    return "none";
  });

  if (role === "guest") return <GuestApp onLeave={() => setRole("none")} />;
  if (role === "prime") return <PerformerApp onLeave={() => setRole("none")} />;

  return (
    <div className="hero">
      <div className="glyph">🌐</div>
      <h1>Escape the Internet</h1>
      <p className="sub">Who are you tonight?</p>
      <button className="choice primary" onClick={() => setRole("guest")}>
        🎟️ I'm a Guest
      </button>
      <button className="choice ghost" onClick={() => setRole("prime")}>
        🎭 I'm a Performer
      </button>
      <a className="link console" href="/console">
        Operator console →
      </a>
    </div>
  );
}
