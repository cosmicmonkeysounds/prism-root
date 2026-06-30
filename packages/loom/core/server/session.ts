//! Operator sessions = a token plus the **capabilities** it carries.
//!
//! Capabilities are additive and compose on a single session, so roles are
//! not mutually exclusive:
//!   - a **performer** holds a `character` (the identity they scan as);
//!   - an **admin** holds the moderator capability (open doors, moderate,
//!     scan-to-moderate);
//!   - a performer who also enters the moderator passcode keeps the *same*
//!     token and gains `admin` — "a performer can also be an admin";
//!   - a `{ character: null, admin: true }` session is a **headless admin**
//!     (an operator with a scanner but no character/booth).
//!
//! Kept out of the HTTP layer — and free of token *generation*, which the
//! server owns — so the capability rules stay pure and unit-testable.

export interface Session {
  /** Performer identity (the character they scan as), or null if headless. */
  character: string | null;
  /** Moderator / admin capability. */
  admin: boolean;
}

export class SessionStore {
  private byToken = new Map<string, Session>();

  /** Create (or replace) the session for a token. */
  set(token: string, session: Session): void {
    this.byToken.set(token, { character: session.character, admin: session.admin });
  }

  /**
   * Merge capabilities into an existing session. Returns false if the token
   * is unknown. `admin` is OR-ed in (never revoked here); `character` is set
   * when provided — this is how a performer upgrades to performer+admin, or
   * an admin picks up a character.
   */
  grant(token: string, patch: Partial<Session>): boolean {
    const s = this.byToken.get(token);
    if (s === undefined) return false;
    if (patch.character !== undefined) s.character = patch.character;
    if (patch.admin === true) s.admin = true;
    return true;
  }

  get(token: string | undefined): Session | undefined {
    return token ? this.byToken.get(token) : undefined;
  }
  has(token: string | undefined): boolean {
    return !!token && this.byToken.has(token);
  }
  delete(token: string): void {
    this.byToken.delete(token);
  }

  // --- capability predicates ---------------------------------------------

  /** Can this token scan a guest at all? (performer OR admin) */
  canScan(token: string | undefined): boolean {
    const s = this.get(token);
    return s !== undefined && (s.character !== null || s.admin);
  }
  /** Can this token perform moderator/admin actions? */
  canModerate(token: string | undefined): boolean {
    return this.get(token)?.admin === true;
  }
  /** The character a token scans as, or null when headless. */
  characterOf(token: string | undefined): string | null {
    return this.get(token)?.character ?? null;
  }

  // --- persistence -------------------------------------------------------

  entries(): Array<[string, Session]> {
    return [...this.byToken.entries()].map(([t, s]) => [t, { ...s }]);
  }
  load(entries: Array<[string, Session]>): void {
    if (!Array.isArray(entries)) return; // tolerate a stale/incompatible store
    for (const [t, s] of entries) {
      this.byToken.set(t, { character: s.character ?? null, admin: s.admin === true });
    }
  }
}
