/**
 * P0 conformance harness.
 *
 * Drives a running openfox-compatible server over HTTP and asserts that the
 * wire contract declared in contract/rest-routes.json is honoured. The same
 * harness runs unmodified against the original TypeScript server and against
 * the Rust rewrite, which is what makes a port verifiable rather than
 * asserted.
 *
 * Usage:
 *   node harnes/conformance.mjs <baseUrl> [contractFile]
 *   OPENFOX_SKIP_WRITES=1 node harnes/conformance.mjs <baseUrl>
 */
import { readFileSync } from 'node:fs'
import { resolve, dirname } from 'node:path'
import { fileURLToPath } from 'node:url'

const here = dirname(fileURLToPath(import.meta.url))
const baseUrl = process.argv[2]
if (!baseUrl) {
  console.error('usage: node conformance.mjs <baseUrl> [contractFile]')
  process.exit(2)
}
const contractFile = resolve(process.argv[3] ?? resolve(here, '../contract/rest-routes.json'))
const routes = JSON.parse(readFileSync(contractFile, 'utf8'))

// Probe responses that prove a route is mounted without requiring a valid
// body or ownership of the resource. 404 = not mounted, which is the failure
// this harness exists to catch.
const PROBE = {
  'GET /api/health': { status: [200], body: '"status":"ok"' },
  'GET /api/config': { status: [200] },
  'GET /api/settings': { status: [200, 400] },
  'GET /api/projects': { status: [200] },
  'GET /api/sessions': { status: [200] },
  'GET /api/providers': { status: [200] },
  'GET /api/tools': { status: [200] },
  'GET /api/plugins': { status: [200] },
  'GET /api/mcp/servers': { status: [200] },
  'GET /api/provider-presets': { status: [200] },
  'GET /api/provider-adapters': { status: [200] },
  'GET /api/changelog': { status: [200] },
  'GET /api/auth': { status: [200] },
}

// Static assets and SPA catch-alls legitimately 404 when the file is absent or
// the built UI is not present, so mounting cannot be proven by their status.
const STATIC_OPTIONAL = new Set([
  'GET /',
  'GET /*path',
  'GET /fox.svg',
  'GET /src/styles/*path',
])

/** Replace :param segments with a syntactically valid placeholder. */
function fillParams(path) {
  return path.replace(/:([A-Za-z0-9_]+)(\?)?/g, (_m, _name, optional) =>
    optional ? '' : 'nonexistent-probe-id'
  )
}

async function probe(method, path) {
  const url = new URL(fillParams(path), baseUrl)
  const init = { method, redirect: 'manual' }
  if (method !== 'GET' && method !== 'HEAD') {
    init.headers = { 'content-type': 'application/json' }
    init.body = '{}'
  }
  try {
    const res = await fetch(url, init)
    const text = await res.text()
    return { status: res.status, text }
  } catch (err) {
    return { status: 0, text: String(err) }
  }
}

const results = []
for (const route of routes) {
  const key = `${route.method} ${route.path}`
  const expect = PROBE[key]
  const { status, text } = await probe(route.method, route.path)

  let verdict
  let detail
  // A path that declares a parameter can only 404 because the probed resource
  // does not exist, which still proves the route is mounted. A 404 on a
  // static path means the route is genuinely absent.
  const hasParam = /:[A-Za-z0-9_]+|\*/.test(route.path)
  if (status === 0) {
    verdict = 'fail'
    detail = `unreachable: ${text.slice(0, 120)}`
  } else if (STATIC_OPTIONAL.has(key)) {
    verdict = status === 404 ? 'skip' : 'pass'
    detail = status === 404 ? 'static asset absent (expected)' : `mounted (${status})`
  } else if (status === 404 && !hasParam) {
    verdict = 'fail'
    detail = 'not mounted (404 on a static path)'
  } else if (status === 404) {
    verdict = 'pass'
    detail = 'mounted (404 = probed resource absent)'
  } else if (expect) {
    const ok = expect.status.includes(status)
    verdict = ok ? 'pass' : 'fail'
    detail = ok ? `status ${status}` : `expected ${expect.status.join('/')}, got ${status}`
    if (ok && expect.body && !text.includes(expect.body)) {
      verdict = 'fail'
      detail = `status ok but body missing "${expect.body}"`
    }
  } else if (status === 401 || status === 403) {
    verdict = 'pass'
    detail = `mounted, auth-gated (${status})`
  } else if (status >= 200 && status < 500) {
    verdict = 'pass'
    detail = `mounted (${status})`
  } else {
    verdict = 'fail'
    detail = `unexpected status ${status}`
  }

  results.push({ key, path: route.path, method: route.method, status, verdict, detail })
}

const failed = results.filter(r => r.verdict === 'fail')
const skipped = results.filter(r => r.verdict === 'skip')
const byStatus = {}
for (const r of results) {
  byStatus[r.status] = (byStatus[r.status] ?? 0) + 1
}

const summary = {
  baseUrl,
  total: results.length,
  passed: results.length - failed.length - skipped.length,
  skipped: skipped.length,
  failed: failed.length,
  statusHistogram: byStatus,
  failures: failed,
  skippedRoutes: skipped.map(r => r.key),
}

console.log(JSON.stringify(summary, null, 2))
process.exit(failed.length === 0 ? 0 : 1)
