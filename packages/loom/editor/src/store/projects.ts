//! The author's projects (SaaS). Backs the launchpad, and opening a project
//! loads its `.loom` files into the workspace store as an editable tree.

import { create } from 'zustand'
import { projectsApi, type ProjectSummary } from '@/lib/api'
import { useWorkspace } from '@/store/workspace'

type ProjectsState = {
  projects: ProjectSummary[]
  status: 'idle' | 'loading' | 'ready' | 'error'
  error: string | null
  currentId: string | null
  currentName: string | null
  load: () => Promise<void>
  create: (name: string, template?: string) => Promise<ProjectSummary>
  open: (id: string) => Promise<void>
  close: () => Promise<void>
  remove: (id: string) => Promise<void>
}

export const useProjects = create<ProjectsState>((set, get) => ({
  projects: [],
  status: 'idle',
  error: null,
  currentId: null,
  currentName: null,

  load: async () => {
    set({ status: 'loading', error: null })
    try {
      set({ projects: await projectsApi.list(), status: 'ready' })
    } catch (e) {
      set({ status: 'error', error: (e as Error).message })
    }
  },

  create: async (name, template) => {
    const p = await projectsApi.create(name, template)
    set((s) => ({ projects: [p, ...s.projects.filter((x) => x.id !== p.id)] }))
    return p
  },

  open: async (id) => {
    const { project, files } = await projectsApi.get(id)
    await useWorkspace.getState().openServerProject({ id: project.id, name: project.name }, files)
    set({ currentId: project.id, currentName: project.name })
  },

  close: async () => {
    await useWorkspace.getState().closeRoot()
    set({ currentId: null, currentName: null })
  },

  remove: async (id) => {
    await projectsApi.remove(id)
    set((s) => ({ projects: s.projects.filter((p) => p.id !== id) }))
    if (get().currentId === id) await get().close()
  },
}))
