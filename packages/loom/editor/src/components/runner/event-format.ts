// Shared helpers for rendering `loom_runtime::ledger::Event` envelopes.
// The wire shape is an externally-tagged enum: `{ "Action": { "text":
// "..." } }`, `{ "Dialogue": { speaker, text, … } }`, `"Ended"`, etc.
// Used by both the Transcript and Ledger panels.

export type LedgerEvent = unknown;

export function eventTag(event: LedgerEvent): [string, Record<string, unknown>] {
  if (typeof event === "string") return [event, {}];
  if (typeof event !== "object" || event === null) return ["?", {}];
  for (const k in event as Record<string, unknown>) {
    const v = (event as Record<string, unknown>)[k];
    return [k, (v && typeof v === "object" ? (v as Record<string, unknown>) : { value: v })];
  }
  return ["?", {}];
}

// Compact one-line summary for the Ledger panel.
export function eventSummary(event: LedgerEvent): string {
  const [tag, body] = eventTag(event);
  switch (tag) {
    case "Action":
      return String(body.text ?? "");
    case "Dialogue": {
      const speakers = (body.speakers as string[]) ?? [body.speaker as string ?? "?"];
      const paren = body.parenthetical ? ` (${body.parenthetical})` : "";
      return `${speakers.join(" | ")}${paren}: ${body.text ?? ""}`;
    }
    case "Scene":
      return String(body.text ?? "");
    case "ChoiceTaken":
      return `▸ ${body.text ?? ""}`;
    case "BeatEntered":
      return `╴ ${body.beat ?? ""}`;
    case "Diverted":
    case "Tunneled":
      return `→ ${body.target ?? body.beat ?? ""}`;
    case "WorldSet":
      return `${body.key ?? "?"} = ${body.value ?? ""}`;
    case "KnowledgeChanged":
      return `${body.character ?? "?"}.knows.${body.field ?? "?"} = ${body.value ?? ""}`;
    case "HookFired":
      return `↯ ${body.hook ?? body.event ?? ""}`;
    case "Ended":
      return "— end —";
    default:
      return tag;
  }
}

// Per-envelope-kind colour. Picked to read against the dark Tailwind
// background and to group thematically (movement = teal, dialogue =
// warm, world writes = orange, hooks = red, booth = bright).
export const EVENT_COLORS: Record<string, string> = {
  CellEntered: "#37474f",
  CellExited: "#263238",
  BeatEntered: "#5c6bc0",
  Dialogue: "#e0a800",
  Action: "#cfd8dc",
  Scene: "#80cbc4",
  ChoiceTaken: "#aed581",
  Diverted: "#80deea",
  Tunneled: "#80deea",
  Returned: "#90caf9",
  WorldSet: "#ffb74d",
  KnowledgeChanged: "#ffd54f",
  HookFired: "#ef5350",
  CastBound: "#ff8a65",
  CastReleased: "#ff8a65",
  CastSwapped: "#ff8a65",
  RolePromoted: "#ff8a65",
  RosterLoaded: "#ff8a65",
  ParticipantJoined: "#9ccc65",
  ParticipantEnteredLocation: "#9ccc65",
  CohortEnrolled: "#9ccc65",
  Ended: "#b0bec5",
};

export const TRACK_COLORS: Record<string, string> = {
  booth: "#ff7043",
  main: "#90a4ae",
  role: "#e0a800",
  person: "#42a5f5",
  cohort: "#7c4dff",
  generator: "#26a69a",
};
