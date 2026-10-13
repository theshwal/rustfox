/**
 * P0 conformance check, in-process.
 *
 * Boots the reference openfox server inside this Node process and probes it
 * directly, then exits normally. Nothing is backgrounded and nothing is
 * killed, so this needs no process management and cannot outlive the command.
 *
 * Usage: npx tsx scripts/verify-contract.ts <contractDir>
 */
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { randomUUID } from 'node:crypto'
import { readFileSync, writeFileSync } from 'node:fs'

const contractDir = process.argv[2] ?? 'contract'

process.env['OPENFOX_MOCK_LLM'] = 'true'
process.env['OPENFOX_DB_PATH'] = ':memory:'
process.env['OPENFOX_LOG_LEVEL'] = 'error'

const { loadConfig } = await import('../src/server/config.js')
const { createServerHandle } = await import('../src/server/index.js')

const config = loadConfig()
config.mode = 'test'
config.globalConfigPath = join(tmpdir(), `openfox-verify-${randomUUID()}.json`)

const handle = await createServerHandle(config)
const { port } = await handle.start(0)
const base = `http://127.0.0.1:${port}`

const routes: { method: string; path: string }[] = JSON.parse(
  readFileSync(join(contractDir, 'rest-routes.json'), 'utf8')
)

// Static assets and SPA catch-alls legitimately 404 when absent or unbuilt.
const STATIC_OPTIONAL = new Set(['GET /', 'GET /*path', 'GET /fox.svg', 'GET /src/styles/*path'])
// Routes whose parent resource must exist; they answer 404 when it does not.
const NEEDS_PARENT = new Set(['POST /api/sessions/import', 'POST /api/projects/check-permissions', 'POST /api/projects/fix-permissions'])

function fillParams(p: string): string {
  return p.replace(/:([A-Za-z0-9_]+)(\?)?/g, (_m, _n, optional) => (optional ? '' : 'nonexistent-probe-id'))
}

type Verdict = 'pass' | 'fail' | 'skip'
const results: { key: string; status: number; verdict: Verdict; detail: string }[] = []

for (const r of routes) {
  const key = `${r.method} ${r.path}`
  const url = new URL(fillParams(r.path), base)
  const init: RequestInit = { method: r.method, redirect: 'manual' }
  if (r.method !== 'GET' && r.method !== 'HEAD') {
    init.headers = { 'content-type': 'application/json' }
    init.body = '{}'
  }

  let status = 0
  let text = ''
  try {
    const res = await fetch(url, init)
    status = res.status
    text = await res.text()
  } catch (err) {
    text = String(err)
  }

  const hasParam = /:[A-Za-z0-9_]+|\*/.test(r.path)
  let verdict: Verdict
  let detail: string

  if (status === 0) {
    verdict = 'fail'
    detail = `unreachable: ${text.slice(0, 100)}`
  } else if (STATIC_OPTIONAL.has(key)) {
    verdict = status === 404 ? 'skip' : 'pass'
    detail = status === 404 ? 'static asset absent' : `mounted (${status})`
  } else if (status === 405) {
    verdict = 'fail'
    detail = 'method not allowed'
  } else if (status === 404 && (hasParam || NEEDS_PARENT.has(key))) {
    verdict = 'pass'
    detail = 'mounted (404 = probed resource absent)'
  } else if (status === 404) {
    verdict = 'fail'
    detail = 'not mounted'
  } else if (status >= 200 && status < 500) {
    verdict = 'pass'
    detail = `mounted (${status})`
  } else {
    verdict = 'fail'
    detail = `unexpected ${status}`
  }

  results.push({ key, status, verdict, detail })
}

const failed = results.filter(r => r.verdict === 'fail')
const skipped = results.filter(r => r.verdict === 'skip')
const histogram: Record<string, number> = {}
for (const r of results) histogram[r.status] = (histogram[r.status] ?? 0) + 1

const summary = {
  total: results.length,
  passed: results.length - failed.length - skipped.length,
  skipped: skipped.length,
  failed: failed.length,
  statusHistogram: histogram,
  failures: failed,
}

writeFileSync(join(contractDir, 'conformance-baseline.json'), JSON.stringify(summary, null, 2))
console.error(
  `contract: ${summary.passed} passed, ${summary.skipped} skipped, ${summary.failed} failed (of ${summary.total})`
)
for (const f of failed) console.error(`  FAIL ${f.key} — ${f.detail}`)

await handle.close()
process.exit(failed.length === 0 ? 0 : 1)
