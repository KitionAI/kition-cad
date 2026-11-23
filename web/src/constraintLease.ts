import type { KitLeases } from '@kition/sdk/editor'

/**
 * Solving a drawing's constraints while someone else solves them does not merge into anything
 * either of them meant (Kition platform rule M21, `docs/platform/data-model.md` 5.3): so a command
 * that adds, changes, removes or infers constraints takes the drawing's constraint lease
 * (`constraints`, declared in `kit.json` `collaboration.leases`) through the host before it runs,
 * keeps it while such a command runs (an interactive one waits for its picks), and lets it go a
 * moment after. A constraint command while someone else holds the lease is refused, with who
 * holds it. Without co-editing every command goes ahead as before. An agent's step takes the same
 * lease (`agentLeaseScope`).
 */

export const CONSTRAINT_SCOPE = 'constraints'
/** How long the lease outlives the last constraint command, so a run of them keeps it. */
export const CONSTRAINT_IDLE_MS = 3_000
/** One renewal's length; the host renews it while it is held. */
export const CONSTRAINT_LEASE_TTL = 15_000

/** Whether an engine command solves the drawing's constraints: adds, changes, removes or infers them. */
export function solvesConstraints(command: string): boolean {
  const id = command.toLowerCase()
  if (id.startsWith('dcdisplay')) return false
  return (
    id.startsWith('gc') ||
    id.startsWith('dc') ||
    id.startsWith('parameters') ||
    id === 'geomconstraint' ||
    id === 'dimconstraint' ||
    id === 'autoconstrain' ||
    id === 'delconstraint'
  )
}

/** The engine command a call runs: the command itself, or the first word typed on the command line. */
export function commandOf(command: string, params: Record<string, unknown> = {}): string {
  if (command !== 'commandLine') return command
  const text = typeof params.text === 'string' ? params.text.trim() : ''
  return (text.split(/\s+/)[0] ?? '').replace(/^[-_.']+/, '').toLowerCase()
}

/** The lease a call needs: the constraint lease for a command that solves constraints, else none. */
export function constraintLeaseScope(command: string, params: Record<string, unknown> = {}): string | null {
  return solvesConstraints(commandOf(command, params)) ? CONSTRAINT_SCOPE : null
}

type Timers = { setTimeout: (run: () => void, ms: number) => unknown; clearTimeout: (handle: unknown) => void }

/**
 * The person's constraint lease. `take` is asked before a command runs (false: someone else
 * holds the lease, and `refuse` said who); `follow` after the running command changed, holding
 * the lease while a constraint command runs; `idle` after a command, which lets the lease go
 * `CONSTRAINT_IDLE_MS` later unless a constraint command is running then.
 */
export class ConstraintLease {
  private held = false
  private timer: unknown = null
  private readonly leases: KitLeases
  private readonly refuse: (holder: string) => void
  private readonly running: () => string | null
  private readonly timers: Timers

  constructor(leases: KitLeases, refuse: (holder: string) => void, running: () => string | null, timers?: Timers) {
    this.leases = leases
    this.refuse = refuse
    this.running = running
    this.timers = timers ?? { setTimeout: (run, ms) => setTimeout(run, ms), clearTimeout: (handle) => clearTimeout(handle as number) }
  }

  /** Whether `command` may run: it needs no lease, nobody coordinates, or this session holds or got it. */
  async take(command: string, params: Record<string, unknown> = {}): Promise<boolean> {
    if (!constraintLeaseScope(command, params) || !this.leases.available) return true
    this.cancelIdle()
    if (this.held) return true
    const other = this.leases.heldByOther(CONSTRAINT_SCOPE)
    if (other) {
      this.refuse(other.holder.name)
      return false
    }
    const outcome = await this.leases.acquire(CONSTRAINT_SCOPE, CONSTRAINT_LEASE_TTL)
    if (outcome.granted) {
      this.held = outcome.lease !== null
      return true
    }
    if (outcome.reason === 'held') this.refuse(outcome.holder?.name ?? '')
    return false
  }

  /** Keeps the lease while a constraint command runs; lets it idle otherwise. */
  follow() {
    const running = this.running()
    if (running && solvesConstraints(running)) {
      this.cancelIdle()
      if (!this.held && this.leases.available) {
        void this.leases.acquire(CONSTRAINT_SCOPE, CONSTRAINT_LEASE_TTL).then((outcome) => {
          if (outcome.granted && outcome.lease) this.held = true
        })
      }
      return
    }
    this.idle()
  }

  idle() {
    this.cancelIdle()
    if (!this.held) return
    this.timer = this.timers.setTimeout(() => {
      this.timer = null
      const running = this.running()
      if (!(running && solvesConstraints(running))) this.release()
    }, CONSTRAINT_IDLE_MS)
  }

  release() {
    this.cancelIdle()
    if (!this.held) return
    this.held = false
    void this.leases.release(CONSTRAINT_SCOPE)
  }

  private cancelIdle() {
    if (this.timer === null) return
    this.timers.clearTimeout(this.timer)
    this.timer = null
  }
}

/** `run`, with a command that solves constraints taking the constraint lease first (refused: it runs nothing, answering `null`). */
export function withConstraintLease<A extends unknown[]>(
  run: (command: string, params?: Record<string, unknown>, ...rest: A) => Promise<string | null>,
  lease: ConstraintLease,
): (command: string, params?: Record<string, unknown>, ...rest: A) => Promise<string | null> {
  return async (command, params, ...rest) => {
    if (!(await lease.take(command, params ?? {}))) return null
    try {
      return await run(command, params, ...rest)
    } finally {
      lease.idle()
    }
  }
}
