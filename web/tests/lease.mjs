// The constraint lease (`src/constraintLease.ts`, feature cad.collab.constraint-lease): a command that
// solves constraints takes the drawing's `constraints` lease before it runs, is refused while
// someone else holds it, keeps it while a constraint command runs and lets it go a moment after;
// other commands and a document nobody co-edits never wait. Runs in Node (type stripping).
import assert from 'node:assert/strict'
import {
  CONSTRAINT_IDLE_MS,
  CONSTRAINT_SCOPE,
  ConstraintLease,
  constraintLeaseScope,
  solvesConstraints,
  withConstraintLease,
} from '../src/constraintLease.ts'

function fakeLeases({ available = true, other = null } = {}) {
  const calls = []
  let held = other
  return {
    calls,
    get available() {
      return available
    },
    list: () => (held ? [held] : []),
    subscribe: () => () => undefined,
    heldByOther: (scope) => (held && !held.holder.self && (held.scope === scope || held.scope === '*') ? held : null),
    acquire: async (scope, ttl) => {
      calls.push(['acquire', scope, ttl])
      if (held && !held.holder.self) return { granted: false, reason: 'held', holder: held.holder }
      held = { scope, holder: { principalId: 'me', name: 'Me', self: true } }
      return { granted: true, lease: held }
    },
    release: async (scope) => {
      calls.push(['release', scope])
      held = null
    },
  }
}

function fakeTimers() {
  const pending = new Map()
  let next = 1
  return {
    setTimeout: (run, ms) => {
      const id = next++
      pending.set(id, { run, ms })
      return id
    },
    clearTimeout: (id) => pending.delete(id),
    fire() {
      const due = [...pending.values()]
      pending.clear()
      for (const { run } of due) run()
    },
    pending,
  }
}

// Which commands solve constraints, also typed on the command line.
assert.ok(solvesConstraints('constraint.coincident') && solvesConstraints('constraint.dimensionAligned') && solvesConstraints('constraint.auto'))
assert.ok(solvesConstraints('constraint.addGeometric') && solvesConstraints('constraint.addDimensional') && solvesConstraints('constraint.delete') && solvesConstraints('parameters'))
assert.ok(!solvesConstraints('line') && !solvesConstraints('constraint.dynamicDimensions') && !solvesConstraints('move'))
assert.equal(constraintLeaseScope('constraint.horizontal', {}), CONSTRAINT_SCOPE)
assert.equal(constraintLeaseScope('command.runLine', { text: '  GCVERTICAL 1A2' }), CONSTRAINT_SCOPE)
assert.equal(constraintLeaseScope('command.runLine', { text: 'line 0,0 1,1' }), null)
assert.equal(constraintLeaseScope('circle', {}), null)

// A constraint command takes the lease, runs, and lets it go a moment after.
{
  const leases = fakeLeases()
  const timers = fakeTimers()
  let running = null
  const lease = new ConstraintLease(leases, () => assert.fail('nobody else holds it'), () => running, timers)
  const ran = []
  const run = withConstraintLease(async (command) => {
    ran.push(command)
    return '{}'
  }, lease)
  assert.equal(await run('constraint.coincident', {}), '{}')
  assert.deepEqual(ran, ['constraint.coincident'])
  assert.deepEqual(leases.calls[0], ['acquire', CONSTRAINT_SCOPE, 15_000])
  assert.equal([...timers.pending.values()][0].ms, CONSTRAINT_IDLE_MS)
  timers.fire()
  assert.deepEqual(leases.calls.at(-1), ['release', CONSTRAINT_SCOPE])
  // Other commands never ask.
  const calls = leases.calls.length
  assert.equal(await run('line', { points: [[0, 0], [1, 1]] }), '{}')
  assert.equal(leases.calls.length, calls)
  // An interactive constraint command keeps the lease while it runs.
  assert.equal(await run('constraint.parallel', {}), '{}')
  running = 'constraint.parallel'
  lease.follow()
  timers.fire()
  assert.notDeepEqual(leases.calls.at(-1), ['release', CONSTRAINT_SCOPE], 'kept while the command runs')
  running = null
  lease.follow()
  timers.fire()
  assert.deepEqual(leases.calls.at(-1), ['release', CONSTRAINT_SCOPE])
}

// Someone else solving constraints: the command is refused, and the person is told who.
{
  const leases = fakeLeases({ other: { scope: CONSTRAINT_SCOPE, holder: { principalId: 'ada', name: 'Ada', self: false } } })
  const refused = []
  const lease = new ConstraintLease(leases, (holder) => refused.push(holder), () => null, fakeTimers())
  const ran = []
  const run = withConstraintLease(async (command) => {
    ran.push(command)
    return '{}'
  }, lease)
  assert.equal(await run('constraint.addDimensional', {}), null)
  assert.deepEqual(ran, [])
  assert.deepEqual(refused, ['Ada'])
  assert.equal(await run('line', {}), '{}', 'commands that solve nothing go ahead')
}

// Without co-editing nothing is leased.
{
  const leases = fakeLeases({ available: false })
  const lease = new ConstraintLease(leases, () => assert.fail('no refusal'), () => null, fakeTimers())
  const run = withConstraintLease(async () => '{}', lease)
  assert.equal(await run('constraint.auto', {}), '{}')
  assert.deepEqual(leases.calls, [])
}

console.log('constraint lease: ok')
