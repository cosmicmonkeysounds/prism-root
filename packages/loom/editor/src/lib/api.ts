//! The control-plane HTTP client for the Loom SaaS backend.
//!
//! Talks to the same server that hosts live events (`@loom/core` server):
//! BetterAuth under `/api/auth/*`, projects + files under `/api/projects/*`,
//! and event launch/lifecycle under `/api/projects/:id/event`. All requests
//! carry the session cookie (`credentials: "include"`); in dev the Vite proxy
//! makes these same-origin so the cookie flows.

const BASE = import.meta.env.VITE_LOOM_API ?? ''

async function req<T>(method: string, path: string, body?: unknown): Promise<T> {
  const res = await fetch(`${BASE}${path}`, {
    method,
    headers: body !== undefined ? { 'content-type': 'application/json' } : undefined,
    body: body !== undefined ? JSON.stringify(body) : undefined,
    credentials: 'include',
  })
  const data = (await res.json().catch(() => ({}))) as Record<string, unknown>
  if (!res.ok) throw new Error((data['error'] as string) || (data['message'] as string) || `HTTP ${res.status}`)
  return data as T
}

// --- auth (BetterAuth) --------------------------------------------------

export interface AuthUser {
  id: string
  email: string
  name: string
}

export const authApi = {
  async signUp(name: string, email: string, password: string): Promise<void> {
    await req('POST', '/api/auth/sign-up/email', { name, email, password })
  },
  async signIn(email: string, password: string): Promise<void> {
    await req('POST', '/api/auth/sign-in/email', { email, password })
  },
  async signOut(): Promise<void> {
    await req('POST', '/api/auth/sign-out', {})
  },
  async session(): Promise<AuthUser | null> {
    const s = await req<{ user?: AuthUser } | null>('GET', '/api/auth/get-session')
    return s?.user ?? null
  },
}

// --- projects + files ---------------------------------------------------

export interface ProjectSummary {
  id: string
  name: string
  slug: string
  updatedAt: string
  activeEvent: { id: string; mode: string; status: string } | null
}

export interface ProjectFile {
  path: string
  content: string
  updatedAt?: string
}

export const projectsApi = {
  async list(): Promise<ProjectSummary[]> {
    return (await req<{ projects: ProjectSummary[] }>('GET', '/api/projects')).projects
  },
  async create(name: string, template?: string): Promise<ProjectSummary> {
    return (await req<{ project: ProjectSummary }>('POST', '/api/projects', { name, template })).project
  },
  async get(id: string): Promise<{ project: ProjectSummary; files: ProjectFile[] }> {
    return req('GET', `/api/projects/${id}`)
  },
  async rename(id: string, name: string): Promise<ProjectSummary> {
    return (await req<{ project: ProjectSummary }>('PATCH', `/api/projects/${id}`, { name })).project
  },
  async remove(id: string): Promise<void> {
    await req('DELETE', `/api/projects/${id}`)
  },
  async putFile(id: string, path: string, content: string): Promise<void> {
    await req('PUT', `/api/projects/${id}/files`, { path, content })
  },
  async deleteFile(id: string, path: string): Promise<void> {
    await req('DELETE', `/api/projects/${id}/files`, { path })
  },
}

// --- events (launch / lifecycle) ----------------------------------------

export interface EventInfo {
  id: string
  projectId: string
  mode: 'live' | 'preview'
  status: string
  codes: { event: string; prime: string; mod: string }
  joinUrl: string
  createdAt: string
}

export const eventsApi = {
  async status(projectId: string): Promise<EventInfo | null> {
    return (await req<{ event: EventInfo | null }>('GET', `/api/projects/${projectId}/event`)).event
  },
  async launch(projectId: string, mode: 'live' | 'preview'): Promise<EventInfo> {
    return (await req<{ event: EventInfo }>('POST', `/api/projects/${projectId}/event`, { mode })).event
  },
  async pause(projectId: string): Promise<EventInfo> {
    return (await req<{ event: EventInfo }>('POST', `/api/projects/${projectId}/event/pause`)).event
  },
  async resume(projectId: string): Promise<EventInfo> {
    return (await req<{ event: EventInfo }>('POST', `/api/projects/${projectId}/event/resume`)).event
  },
  async end(projectId: string): Promise<void> {
    await req('POST', `/api/projects/${projectId}/event/end`)
  },
}

// --- live moderation (per-event; authorized by the owning author's session) ---
// These hit the SAME `/e/:eventId/api/mod/*` routes the operator console uses,
// which now also accept the owning author's cookie — run + admin are one
// capability.

export type StatField = 'score' | 'faction' | 'location' | 'captured'

export const modApi = {
  async act(eventId: string, id: string, action: 'capture' | 'release' | 'signal', name?: string): Promise<void> {
    await req('POST', `/e/${eventId}/api/mod/act`, { id, action, name })
  },
  async hideMessage(eventId: string, seq: number, hidden: boolean): Promise<void> {
    await req('POST', `/e/${eventId}/api/mod/message`, { seq, hidden })
  },
  async broadcast(eventId: string, scope: string, cue: string): Promise<void> {
    await req('POST', `/e/${eventId}/api/mod/broadcast`, { scope, cue })
  },
  /** Post a message into any room, as the Operator or in a character's voice. */
  async say(eventId: string, channel: string, text: string, as?: string, parentSeq?: number | null): Promise<void> {
    await req('POST', `/e/${eventId}/api/mod/say`, { channel, text, as, parentSeq })
  },
  /** Live-edit one guest stat (score / faction / location / captured). */
  async setStat(eventId: string, id: string, field: StatField, value: string | number | boolean): Promise<void> {
    await req('POST', `/e/${eventId}/api/mod/set`, { id, field, value })
  },
  /** Fire a named story beat (optionally targeting one guest). */
  async fireBeat(eventId: string, name: string, subject?: string): Promise<void> {
    await req('POST', `/e/${eventId}/api/mod/beat`, { name, subject })
  },
  /** Fire a generic `on <name>` signal, globally or on one subject. */
  async fireSignal(eventId: string, name: string, subject?: string): Promise<void> {
    await req('POST', `/e/${eventId}/api/mod/signal`, { name, subject })
  },
  /** Scan a guest as a character — fires that character's scan reaction. */
  async scanAs(eventId: string, as: string, target: string): Promise<void> {
    await req('POST', `/e/${eventId}/api/mod/scan`, { as, target })
  },
  /** Reset the event: clear the journal + replay from the loaded scenario. */
  async reset(eventId: string): Promise<void> {
    await req('POST', `/e/${eventId}/api/mod/reset`, {})
  },
}
