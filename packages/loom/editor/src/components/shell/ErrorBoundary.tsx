// A top-level error boundary so a render crash (e.g. a bad zustand
// selector spinning an infinite loop) shows a recoverable fallback
// instead of blanking the whole tab — which previously forced a full
// dev-server reboot. On HMR update it auto-clears, so a code fix
// recovers without a manual reload.

import { Component, type ErrorInfo, type ReactNode } from 'react'

type Props = { children: ReactNode }
type State = { error: Error | null }

export class ErrorBoundary extends Component<Props, State> {
  state: State = { error: null }
  private onHot?: () => void

  static getDerivedStateFromError(error: Error): State {
    return { error }
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error('Loom UI error boundary caught:', error, info.componentStack)
  }

  componentDidMount() {
    const hot = import.meta.hot
    if (hot) {
      this.onHot = () => this.setState({ error: null })
      hot.on('vite:afterUpdate', this.onHot)
    }
  }

  componentWillUnmount() {
    if (import.meta.hot && this.onHot) {
      import.meta.hot.off('vite:afterUpdate', this.onHot)
    }
  }

  render() {
    const { error } = this.state
    if (!error) return this.props.children
    return (
      <div className="h-full w-full grid place-items-center bg-zinc-950 p-6 text-zinc-300">
        <div className="max-w-xl">
          <div className="text-rose-400 font-semibold text-sm">The editor UI hit an error.</div>
          <pre className="mt-2 text-xs text-zinc-500 whitespace-pre-wrap break-words max-h-60 overflow-auto">
            {error.message}
          </pre>
          <div className="mt-3 text-zinc-500 text-xs">
            Fix the code and it should recover automatically (HMR), or reload the view:
          </div>
          <button
            type="button"
            onClick={() => this.setState({ error: null })}
            className="mt-2 px-3 py-1 rounded border border-white/15 text-zinc-200 hover:bg-white/10 text-sm"
          >
            Reload view
          </button>
        </div>
      </div>
    )
  }
}
