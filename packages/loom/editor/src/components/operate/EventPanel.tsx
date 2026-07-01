//! Operate mode — the Event page (center tab). The promoted, full-width home
//! of the Run cockpit: launch/lifecycle, the shareable codes + join QR, live
//! event stats, and a guest-QR scanner (merged from the retired operator
//! console) that pulls a scanned guest straight into the Inspector.

import { useEffect, useRef, useState } from 'react'
import clsx from 'clsx'
import { useOperate } from '@/store/operate'
import { useInspect } from './inspect'

// The Barcode Detection API isn't in the TS DOM lib; narrow shim (no `any`).
interface BarcodeDetectorLike {
  detect(source: CanvasImageSource): Promise<Array<{ rawValue: string }>>
}
type BarcodeDetectorCtor = new (opts?: { formats?: string[] }) => BarcodeDetectorLike
function barcodeCtor(): BarcodeDetectorCtor | undefined {
  return (window as unknown as { BarcodeDetector?: BarcodeDetectorCtor }).BarcodeDetector
}

function Card({ title, children }: { title?: string; children: React.ReactNode }) {
  return (
    <section className="rounded-xl border border-zinc-800 bg-zinc-950 p-3">
      {title && <div className="mb-2 text-[10px] uppercase tracking-widest text-zinc-500">{title}</div>}
      {children}
    </section>
  )
}

function CodeRow({ label, code }: { label: string; code: string }) {
  return (
    <div className="flex items-center justify-between rounded-lg border border-zinc-800 px-3 py-2">
      <div>
        <div className="text-[10px] uppercase tracking-wide text-zinc-500">{label}</div>
        <div className="font-mono text-base tracking-widest text-zinc-100">{code}</div>
      </div>
      <button
        onClick={() => void navigator.clipboard?.writeText(code)}
        className="rounded px-2 py-1 text-xs text-zinc-500 hover:bg-zinc-800 hover:text-zinc-200"
        title="Copy"
      >
        copy
      </button>
    </div>
  )
}

function Stat({ label, value }: { label: string; value: string | number }) {
  return (
    <div className="rounded-lg border border-zinc-800 px-2 py-1.5 text-center">
      <div className="truncate text-lg font-semibold text-zinc-100">{value}</div>
      <div className="text-[9px] uppercase tracking-wide text-zinc-500">{label}</div>
    </div>
  )
}

/** Camera QR scan + manual entry → hands a guest id to `onFound`. */
function GuestScanner({ onFound }: { onFound: (id: string) => void }) {
  const videoRef = useRef<HTMLVideoElement>(null)
  const streamRef = useRef<MediaStream | null>(null)
  const [scanning, setScanning] = useState(false)
  const [manual, setManual] = useState('')
  const [err, setErr] = useState<string | null>(null)

  const stop = () => {
    streamRef.current?.getTracks().forEach((t) => t.stop())
    streamRef.current = null
    setScanning(false)
  }

  useEffect(() => stop, []) // stop the camera on unmount

  const start = async () => {
    setErr(null)
    const Ctor = barcodeCtor()
    if (!Ctor) {
      setErr("Camera scanning isn't supported on this browser — enter the id below.")
      return
    }
    try {
      const stream = await navigator.mediaDevices.getUserMedia({ video: { facingMode: 'environment' } })
      streamRef.current = stream
      setScanning(true)
      // The <video> is always mounted (just hidden), so the ref is ready here.
      const video = videoRef.current!
      video.srcObject = stream
      await video.play()
      const det = new Ctor({ formats: ['qr_code'] })
      const tick = async () => {
        if (!streamRef.current) return
        try {
          const codes = await det.detect(video)
          if (codes[0]?.rawValue) {
            const id = codes[0].rawValue.trim()
            stop()
            onFound(id)
            return
          }
        } catch {
          /* transient decode error — keep polling */
        }
        requestAnimationFrame(tick)
      }
      void tick()
    } catch (e) {
      setErr('Camera error: ' + (e as Error).message)
      stop()
    }
  }

  return (
    <div className="flex flex-col gap-2">
      {/* Always mounted so the ref is ready when the camera starts; hidden when idle. */}
      <video
        ref={videoRef}
        playsInline
        className={clsx('w-full rounded-lg border border-zinc-800', !scanning && 'hidden')}
        style={{ maxHeight: 220 }}
      />
      <div className="flex gap-2">
        {scanning ? (
          <button onClick={stop} className="flex-1 rounded-lg border border-zinc-700 px-3 py-1.5 text-sm hover:bg-zinc-800">
            Stop camera
          </button>
        ) : (
          <button onClick={() => void start()} className="flex-1 rounded-lg border border-zinc-700 px-3 py-1.5 text-sm hover:bg-zinc-800">
            📷 Scan a guest's QR
          </button>
        )}
      </div>
      <div className="flex gap-2">
        <input
          className="min-w-0 flex-1 rounded border border-zinc-700 bg-zinc-950 px-2 py-1.5 text-sm outline-none focus:border-indigo-500"
          placeholder="…or enter a guest id (e.g. g-1a2b3c)"
          value={manual}
          onChange={(e) => setManual(e.target.value)}
          onKeyDown={(e) => e.key === 'Enter' && manual.trim() && onFound(manual.trim())}
        />
        <button
          onClick={() => manual.trim() && onFound(manual.trim())}
          className="shrink-0 rounded bg-zinc-800 px-3 py-1.5 text-sm text-zinc-200 hover:bg-zinc-700"
        >
          Look up
        </button>
      </div>
      {err && <div className="text-xs text-amber-400">{err}</div>}
    </div>
  )
}

export function EventPanel() {
  const event = useOperate((s) => s.event)
  const phase = useOperate((s) => s.phase)
  const busy = useOperate((s) => s.busy)
  const error = useOperate((s) => s.error)
  const scenario = useOperate((s) => s.scenario)
  const rosterLen = useOperate((s) => s.roster.length)
  const ledgerLen = useOperate((s) => s.ledgerLen)
  const launch = useOperate((s) => s.launch)
  const pause = useOperate((s) => s.pause)
  const resume = useOperate((s) => s.resume)
  const end = useOperate((s) => s.end)
  const reset = useOperate((s) => s.reset)
  const inspect = useInspect()

  const created = event ? new Date(event.createdAt) : null
  const createdLabel = created && !Number.isNaN(created.getTime()) ? created.toLocaleString() : null

  return (
    <div className="h-full overflow-auto p-4">
      <div className="mx-auto max-w-3xl">
        {error && <div className="mb-3 rounded bg-red-950 px-3 py-2 text-sm text-red-300">{error}</div>}

        {!event ? (
          <Card title="Launch">
            <p className="mb-3 text-sm text-zinc-500">
              Launch a private preview to test on the server, or go live so guests can join by code.
            </p>
            <div className="flex gap-2">
              <button
                onClick={() => void launch('preview')}
                disabled={busy}
                className="flex-1 rounded-lg border border-zinc-700 px-3 py-2 text-sm text-zinc-200 hover:bg-zinc-800 disabled:opacity-50"
              >
                Launch preview
              </button>
              <button
                onClick={() => void launch('live')}
                disabled={busy}
                className="flex-1 rounded-lg bg-emerald-600 px-3 py-2 text-sm font-medium text-white hover:bg-emerald-500 disabled:opacity-50"
              >
                Go live
              </button>
            </div>
          </Card>
        ) : (
          <div className="grid grid-cols-1 gap-4 md:grid-cols-2">
            {/* status */}
            <Card title="Status">
              <div className="grid grid-cols-2 gap-2">
                <Stat label="mode" value={event.mode} />
                <Stat label="phase" value={phase} />
                <Stat label="guests" value={rosterLen} />
                <Stat label="events" value={ledgerLen} />
              </div>
              <div className="mt-3 space-y-1 text-xs text-zinc-500">
                {scenario && (
                  <div className="truncate">
                    Scenario: <span className="text-zinc-300">{scenario}</span>
                  </div>
                )}
                {createdLabel && (
                  <div>
                    Started: <span className="text-zinc-300">{createdLabel}</span>
                  </div>
                )}
                <div className="truncate">
                  Event id: <span className="font-mono text-zinc-400">{event.id}</span>
                </div>
              </div>
            </Card>

            {/* controls */}
            <Card title="Controls">
              <div className="flex flex-col gap-2">
                <div className="flex gap-2">
                  {phase === 'open' ? (
                    <button
                      onClick={() => void pause()}
                      disabled={busy}
                      className="flex-1 rounded-lg border border-zinc-700 px-3 py-2 text-sm hover:bg-zinc-800 disabled:opacity-50"
                    >
                      Pause
                    </button>
                  ) : (
                    <button
                      onClick={() => void resume()}
                      disabled={busy}
                      className="flex-1 rounded-lg bg-emerald-600 px-3 py-2 text-sm font-medium text-white hover:bg-emerald-500 disabled:opacity-50"
                    >
                      Resume
                    </button>
                  )}
                  <button
                    onClick={() =>
                      window.confirm('Reset the event? The story restarts from the top and all chat is cleared.') && void reset()
                    }
                    disabled={busy}
                    className="flex-1 rounded-lg border border-zinc-800 px-3 py-2 text-sm text-zinc-300 hover:bg-zinc-800 disabled:opacity-50"
                  >
                    ↺ Reset
                  </button>
                </div>
                <button
                  onClick={() => window.confirm('End this event? Guests will be disconnected.') && void end()}
                  disabled={busy}
                  className="rounded-lg border border-red-900 px-3 py-2 text-sm text-red-300 hover:bg-red-950 disabled:opacity-50"
                >
                  End event
                </button>
                <a href={event.joinUrl} target="_blank" rel="noreferrer" className="text-center text-xs text-indigo-400 hover:underline">
                  → Open guest view
                </a>
              </div>
            </Card>

            {/* join */}
            <Card title="Join codes">
              <div className="flex flex-col gap-2">
                <CodeRow label="Guest event code" code={event.codes.event} />
                <CodeRow label="Performer code" code={event.codes.prime} />
                <CodeRow label="Moderator code" code={event.codes.mod} />
              </div>
            </Card>

            {/* QR */}
            <Card title="Join QR">
              <div className="rounded-lg bg-white p-2">
                <img src={`/api/qr?text=${encodeURIComponent(event.joinUrl)}`} alt="Join QR" className="mx-auto block h-44 w-44" />
              </div>
              <a href={event.joinUrl} target="_blank" rel="noreferrer" className="mt-2 block truncate text-center text-xs text-indigo-400 hover:underline">
                {event.joinUrl}
              </a>
            </Card>

            {/* scanner */}
            <div className="md:col-span-2">
              <Card title="Look up a guest">
                <p className="mb-2 text-xs text-zinc-500">Scan a guest's pass QR (or type their id) to open them in the Inspector.</p>
                <GuestScanner onFound={(id) => inspect({ kind: 'guest', id })} />
              </Card>
            </div>
          </div>
        )}
      </div>
    </div>
  )
}
