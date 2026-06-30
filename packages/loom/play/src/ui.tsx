//! Root: the role chooser that mounts the guest or performer app. Each app
//! lives in its own module and composes the shared chat primitives.

import { useState } from "react";
import { GuestApp } from "./guest.tsx";
import { PerformerApp } from "./performer.tsx";

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
