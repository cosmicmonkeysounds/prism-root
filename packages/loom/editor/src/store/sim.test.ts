// The Sim-mode store end-to-end against a real in-memory project:
// start (persona + entry beat + overlay + named-event enumeration),
// choices resuming the story, and the mod-command surface mapped onto
// the local engine (scan / setStat / fireSignal → composed messages).

import { beforeEach, describe, expect, it } from 'vitest'
import { lspWorkspaceSync, uriFor } from '@/lib/lsp-client'
import { GLOBAL_CHOICE_KEY, SimStatus, useSim } from '@/store/sim'
import { CockpitPhase } from '@/store/cockpit'
import { useGraph } from '@/store/graph'

const MAIN = `entry: opening

FACTION Mods
  ethos: order

LOCATION Party
  label: The Party

ROLE Guest
  score: 0 to 100 = 0

CHARACTER Greeter
  faction: Mods
  on lockdown
    <broadcast: lockdown_siren to faction(Mods)>
  on scan guest
    <set: guest.score += 5>

== opening
  The lights dim.
  GREETER
    Welcome, traveler.
  * Take the stairs
    -> stairs
  * Take the lift
    -> lift

== stairs
  You climb.

== lift
  You ride.
`

describe('sim store', () => {
  beforeEach(() => {
    useSim.getState().stop()
    lspWorkspaceSync().reset()
    lspWorkspaceSync().updateMany([[uriFor('main.loom'), MAIN]])
  })

  it('start(): persona, entry beat, runtime overlay, named events', () => {
    useSim.getState().start()
    const s = useSim.getState()
    expect(s.status).toBe(SimStatus.Running)
    expect(s.phase).toBe(CockpitPhase.Open)
    expect(s.live).toBe(true)
    // One auto-persona so the story has a subject.
    expect(s.personas).toEqual(['p1'])
    expect(s.roster.map((r) => r.id)).toEqual(['p1'])
    // The model's authored events are enumerated for the director.
    expect(s.events).toEqual(['lockdown'])
    expect(s.beats).toContain('opening')
    // The entry beat fired → the story canvas overlay lights up.
    expect(useGraph.getState().runtime.visits['opening']).toBe(1)
    // Its menu suspended unbound → a global pending choice.
    expect(s.choices[GLOBAL_CHOICE_KEY]).toEqual(['Take the stairs', 'Take the lift'])
    // Un-addressed entry dialogue is stage voice: it lands in the lobby
    // (`opening` has no setting), not a phantom nobody-can-see-it DM.
    const greet = s.messages.find((m) => m.from === 'GREETER')
    expect(greet).toMatchObject({ channel: 'lobby', audience: 'all' })
    // The speakerless prose is the Narrator's narration — visible in chat.
    const narration = s.messages.find((m) => m.kind === 'narration')
    expect(narration).toMatchObject({ from: 'Narrator', text: 'The lights dim.', channel: 'lobby', beat: 'opening' })
  })

  it('choose(): resumes the story and records the traversal', async () => {
    useSim.getState().start()
    await useSim.getState().choose(GLOBAL_CHOICE_KEY, 0)
    expect(useGraph.getState().runtime.visits['stairs']).toBe(1)
    expect(useGraph.getState().runtime.traversed['opening→stairs']).toBe(1)
    expect(useSim.getState().choices[GLOBAL_CHOICE_KEY]).toBeUndefined()
    expect(useSim.getState().log.some((l) => l.event.type === 'beatEntered')).toBe(true)
  })

  it('mod-command surface maps onto the local engine', async () => {
    useSim.getState().start()
    // Scan as a character → its scan hook fires on the persona.
    await useSim.getState().scanAs('Greeter', 'p1')
    expect(useSim.getState().roster[0]!.score).toBe(5)
    // setStat routes faction → defect, location → arrive.
    await useSim.getState().setStat('p1', 'faction', 'Mods')
    expect(useSim.getState().roster[0]!.faction).toBe('Mods')
    await useSim.getState().setStat('p1', 'location', 'Party')
    expect(useSim.getState().roster[0]!.location).toBe('Party')
    // Firing the enumerated named event runs its hook → faction broadcast.
    await useSim.getState().fireSignal('lockdown')
    expect(useSim.getState().messages.some((m) => m.channel === 'faction:Mods')).toBe(true)
    // Typed chat lands in the lobby with a seq the hide toggle can target.
    await useSim.getState().say('lobby', 'mic check')
    const chat = useSim.getState().messages.find((m) => m.text === 'mic check')
    expect(chat).toBeDefined()
    await useSim.getState().hideMessage(chat!.seq, true)
    expect(useSim.getState().messages.find((m) => m.seq === chat!.seq)!.hidden).toBe(true)
  })

  it('narrative routes into the setting room; personas speak as themselves', async () => {
    // Give `stairs` a setting — its narration must land in that room.
    lspWorkspaceSync().updateMany([
      [uriFor('main.loom'), MAIN.replace('== stairs\n', '== stairs\n  setting: Party\n')],
    ])
    useSim.getState().start()
    await useSim.getState().choose(GLOBAL_CHOICE_KEY, 0)
    const climb = useSim.getState().messages.find((m) => m.text === 'You climb.')
    expect(climb).toMatchObject({ channel: 'loc:Party', from: 'Narrator', kind: 'narration', beat: 'stairs' })
    // The location room is enumerated for the rooms rail (modView channels).
    expect(useSim.getState().channels.some((c) => c.id === 'loc:Party' && c.kind === 'location')).toBe(true)
    // A persona speaks as themselves through the same journaled say path
    // the play app uses — the message carries their display name.
    await useSim.getState().say('lobby', 'hi from me', 'p1')
    const line = useSim.getState().messages.find((m) => m.text === 'hi from me')
    expect(line?.from).toBe('Writer')
    // The perspective lens defaults to the operator god view and is settable.
    expect(useSim.getState().perspective).toBe('operator')
    useSim.getState().setPerspective('p1')
    expect(useSim.getState().perspective).toBe('p1')
  })

  it('stop() clears the session; reset() recompiles from current sources', () => {
    useSim.getState().start()
    useSim.getState().stop()
    const s = useSim.getState()
    expect(s.status).toBe(SimStatus.Idle)
    expect(s.live).toBe(false)
    expect(s.roster).toEqual([])
    expect(useGraph.getState().runtime.visits['opening']).toBeUndefined()
    useSim.getState().start()
    expect(useSim.getState().status).toBe(SimStatus.Running)
    expect(useSim.getState().compiledAt).toBe(lspWorkspaceSync().generation)
  })
})
