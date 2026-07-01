//! Control-plane HTTP: an author's projects and their `.loom` files.
//!
//! Every route here is behind a signed-in author (the caller is resolved by
//! `server.ts` and passed in as `user`). Ownership is enforced at the query
//! layer — a project id the author doesn't own reads as 404, never a leak.

import type { IncomingMessage, ServerResponse } from "node:http";

import { scenarioFiles } from "../examples/load.ts";
import { readBody, sendJson, str } from "./http-util.ts";
import {
  activeEvent,
  createProject,
  deleteFile,
  deleteProject,
  getProject,
  listFiles,
  listProjects,
  renameProject,
  upsertFile,
} from "./db/queries.ts";

/** A signed-in author. */
export interface AuthUser {
  id: string;
  email: string;
  name: string;
}

const BLANK_STARTER = `# New Loom project
#
# Write your world here. A minimal event needs a lobby SPACE and a ROLE for
# guests; see the "escape-the-internet" template for a full example.

TITLE: Untitled Event
`;

/** Seed a new project's files from a named template. */
async function seedFiles(projectId: string, template: string): Promise<void> {
  if (template === "blank") {
    await upsertFile(projectId, "main.loom", BLANK_STARTER);
    return;
  }
  // Default: copy the on-disk example project verbatim (main.loom + the rest).
  for (const f of scenarioFiles("escape-the-internet")) {
    await upsertFile(projectId, f.path, f.source);
  }
}

/** The public shape of a project row + its active-event summary. */
function projectView(p: { id: string; name: string; slug: string; updated_at: string }, active: { id: string; mode: string; status: string } | null) {
  return {
    id: p.id,
    name: p.name,
    slug: p.slug,
    updatedAt: p.updated_at,
    activeEvent: active ? { id: active.id, mode: active.mode, status: active.status } : null,
  };
}

/**
 * Handle a `/api/projects…` request for `user`. Returns true when the path
 * matched a projects route (even on error), false to fall through to 404.
 */
export async function handleProjects(
  req: IncomingMessage,
  res: ServerResponse,
  method: string,
  path: string,
  user: AuthUser,
): Promise<boolean> {
  const segs = path.split("/").filter(Boolean); // ["api","projects", id?, "files"?]

  // /api/projects
  if (segs.length === 2) {
    if (method === "GET") {
      const projects = await listProjects(user.id);
      const withActive = await Promise.all(
        projects.map(async (p) => projectView(p, await activeEvent(p.id))),
      );
      sendJson(res, 200, { projects: withActive });
      return true;
    }
    if (method === "POST") {
      const body = await readBody(req);
      const name = str(body, "name").trim() || "Untitled Project";
      const template = str(body, "template") || "escape-the-internet";
      const project = await createProject(user.id, name);
      await seedFiles(project.id, template);
      sendJson(res, 200, { project: projectView(project, null) });
      return true;
    }
    return false;
  }

  // /api/projects/:id[/files]
  if (segs.length === 3 || segs.length === 4) {
    const id = segs[2]!;
    const project = await getProject(user.id, id);
    if (project === null) {
      sendJson(res, 404, { error: "no such project" });
      return true;
    }

    // /api/projects/:id
    if (segs.length === 3) {
      if (method === "GET") {
        const files = await listFiles(id);
        sendJson(res, 200, {
          project: projectView(project, await activeEvent(id)),
          files: files.map((f) => ({ path: f.path, content: f.content, updatedAt: f.updated_at })),
        });
        return true;
      }
      if (method === "PATCH") {
        const body = await readBody(req);
        const name = str(body, "name").trim();
        if (name === "") {
          sendJson(res, 400, { error: "name required" });
          return true;
        }
        const updated = await renameProject(user.id, id, name);
        sendJson(res, 200, { project: projectView(updated!, await activeEvent(id)) });
        return true;
      }
      if (method === "DELETE") {
        await deleteProject(user.id, id);
        sendJson(res, 200, { ok: true });
        return true;
      }
      return false;
    }

    // /api/projects/:id/files  — upsert / delete a single file by body path
    if (segs[3] === "files") {
      if (method === "GET") {
        const files = await listFiles(id);
        sendJson(res, 200, { files: files.map((f) => ({ path: f.path, content: f.content, updatedAt: f.updated_at })) });
        return true;
      }
      if (method === "PUT") {
        const body = await readBody(req);
        const filePath = str(body, "path").trim();
        if (filePath === "" || filePath.includes("..")) {
          sendJson(res, 400, { error: "bad file path" });
          return true;
        }
        const file = await upsertFile(id, filePath, str(body, "content"));
        sendJson(res, 200, { file: { path: file.path, content: file.content, updatedAt: file.updated_at } });
        return true;
      }
      if (method === "DELETE") {
        const body = await readBody(req);
        const ok = await deleteFile(id, str(body, "path"));
        sendJson(res, ok ? 200 : 404, ok ? { ok: true } : { error: "no such file" });
        return true;
      }
    }
  }

  return false;
}
