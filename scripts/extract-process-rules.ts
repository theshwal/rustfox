/**
 * P2 contract: extract the background-process store rules.
 *
 * The store is module state, so every scenario runs against a fresh session id
 * and the process ids are reported back rather than assumed.
 *
 * Usage: npx tsx scripts/extract-process-rules.ts <outFile>
 */
import { writeFileSync, mkdirSync } from 'node:fs'
import { dirname } from 'node:path'

const outFile = process.argv[2] ?? '/tmp/contract/process-rules.json'
mkdirSync(dirname(outFile), { recursive: true })

const store: any = await import('../src/server/tools/background-process/store.js')

const rules: Record<string, unknown> = {}

// The cap, and what it counts.
rules.maxPerSession = store.getMaxPerSession()

// How many processes a session may hold while some are already exited.
const capScenarios: unknown[] = []
for (const exitedFirst of [0, 2, 5, 6, 8]) {
  const session = `s-cap-${exitedFirst}`
  const created: string[] = []
  for (let i = 0; i < 8; i++) {
    const p = store.createProcess(session, `p${i}`, `cmd ${i}`, '/w')
    if (p) {
      created.push(p.id)
      // Mark the ones we want out of the running count.
      if (i < exitedFirst) {
        store.updateStatus(p.id, session, 'exited', 0)
      }
    }
  }
  capScenarios.push({
    exitedFirst,
    created: created.length,
    liveCount: store.getSessionProcessCount(session),
    totalCount: store.getSessionProcesses(session).length,
  })
}
rules.capScenarios = capScenarios

// Log offsets and pagination.
const session = 's-logs'
const proc = store.createProcess(session, 'p', 'echo hi', '/w')
const pid = proc.id

const appended = 10
for (let i = 0; i < appended; i++) {
  store.appendLog(pid, `line ${i}`, i % 3 === 2 ? 'stderr' : 'stdout')
}

const getLogs: unknown[] = []
for (const [offset, limit] of [
  [0, undefined], [0, 3], [2, 3], [5, 100], [100, 3], [10, 5], [-5, 3], [3, 0],
]) {
  const lines = store.getLogs(pid, offset, limit)
  getLogs.push({
    offset,
    limit: limit ?? null,
    count: lines.length,
    first: lines[0]?.content ?? null,
    last: lines[lines.length - 1]?.content ?? null,
  })
}
rules.getLogs = getLogs

const paginated: unknown[] = []
for (const [since, maxLines] of [
  [0, 500], [0, 3], [0, 10], [0, 20], [3, 4], [10, 5], [100, 5], [0, 0], [5, 0],
]) {
  const r = store.getLogsPaginated(pid, since, maxLines)
  paginated.push({
    since,
    maxLines,
    count: r.lines.length,
    totalLines: r.totalLines,
    nextOffset: r.nextOffset,
    hasMore: r.hasMore,
    first: r.lines[0]?.content ?? null,
    last: r.lines[r.lines.length - 1]?.content ?? null,
  })
}
rules.paginated = paginated

// Offsets are dense and independent of the stream.
const offsetCheck = store.getLogs(pid).map(l => ({ offset: l.offset, stream: l.stream }))
rules.offsetCheck = offsetCheck

// clearLogs keeps the process but empties the buffer.
store.clearLogs(pid)
rules.afterClear = {
  total: store.getLogsPaginated(pid).totalLines,
  nextOffset: store.getLogsPaginated(pid).nextOffset,
  hasMore: store.getLogsPaginated(pid).hasMore,
}

// An unknown process id yields empty results rather than throwing.
rules.unknownProcess = {
  getLogs: store.getLogs('no-such-process').length,
  paginated: store.getLogsPaginated('no-such-process'),
  getProcess: store.getProcess('no-such-process', session) ?? null,
}

// Process lookup is keyed by (processId, sessionId), not processId alone.
const other = store.createProcess('s-other', 'p', 'echo', '/w')
rules.crossSession = {
  rightSession: Boolean(store.getProcess(other.id, 's-other')),
  wrongSession: store.getProcess(other.id, 's-logs') ?? null,
  foundById: Boolean(store.getProcessById(other.id)),
}

writeFileSync(
  outFile,
  JSON.stringify(
    {
      description:
        'Background-process store rules captured from src/server/tools/background-process/store.ts. The cap counts only processes whose status is not "exited"; log offsets are dense and pagination derives nextOffset from the last returned line.',
      rules,
    },
    null,
    2
  )
)
console.error('maxPerSession =', rules.maxPerSession)
console.error('cap scenarios:', JSON.stringify(capScenarios))
console.error('pagination:', JSON.stringify(paginated, null, 1).slice(0, 1200))

process.exit(0);
