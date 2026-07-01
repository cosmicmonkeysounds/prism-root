//! Author account session (BetterAuth), the gate in front of the Studio
//! shell. Signed-out → login screen; signed-in → Projects launchpad.

import { create } from 'zustand'
import { authApi, type AuthUser } from '@/lib/api'

type AuthState = {
  user: AuthUser | null
  status: 'loading' | 'signed-out' | 'signed-in'
  error: string | null
  refresh: () => Promise<void>
  signIn: (email: string, password: string) => Promise<void>
  signUp: (name: string, email: string, password: string) => Promise<void>
  signOut: () => Promise<void>
}

export const useAuth = create<AuthState>((set) => ({
  user: null,
  status: 'loading',
  error: null,

  refresh: async () => {
    try {
      const user = await authApi.session()
      set({ user, status: user ? 'signed-in' : 'signed-out', error: null })
    } catch {
      // Control plane offline (no DB) reads as signed-out; events still work.
      set({ user: null, status: 'signed-out', error: null })
    }
  },

  signIn: async (email, password) => {
    set({ error: null })
    try {
      await authApi.signIn(email, password)
      const user = await authApi.session()
      set({ user, status: user ? 'signed-in' : 'signed-out' })
    } catch (e) {
      set({ error: (e as Error).message })
      throw e
    }
  },

  signUp: async (name, email, password) => {
    set({ error: null })
    try {
      await authApi.signUp(name, email, password)
      const user = await authApi.session()
      set({ user, status: user ? 'signed-in' : 'signed-out' })
    } catch (e) {
      set({ error: (e as Error).message })
      throw e
    }
  },

  signOut: async () => {
    try {
      await authApi.signOut()
    } finally {
      set({ user: null, status: 'signed-out' })
    }
  },
}))
