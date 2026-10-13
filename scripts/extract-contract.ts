/**
 * P0 contract extractor (REST), static pass.
 *
 * The server mounts routers built by factory functions (createPluginRoutes(),
 * createSkillRoutes(), ...), which populate their router before returning, so
 * registration-time interception cannot observe their inner routes. Instead:
 *   1. scan src/server/index.ts for `app.use('<prefix>', createXxxRoutes(...))`
 *      to build a factory -> mount-prefix table;
 *   2. scan each route module for the local paths it registers;
 *   3. join both, then validate the result against a live server, because the
 *      static join is only trustworthy once the conformance harness confirms
 *      every declared path is actually mounted.
 *
 * Usage: npx tsx scripts/extract-contract.ts <outDir>
 */
import { readFileSync, readdirSync, writeFileSync, mkdirSync } from 'node:fs'
import { join } from 'node:path'
import { tmpdir } from 'node:os'
import { randomUUID } from 'node:crypto'

const repoRoot = new URL('..', import.meta.url).pathname
const outDir = process.argv[2] ?? '/tmp/contract'
mkdirSync(outDir, { recursive: true })

const read = (p: string) => readFileSync(join(repoRoot, p), 'utf8')

/** Normalise an Express path to a clean absolute form. */
function joinPaths(base: string, p: string): string {
  const cleanP = p.replace(/^\/+/, '')
  const merged = `${base}/${cleanP}`.replace(/\/{2,}/g, '/')
  const trimmed = merged.replace(/\/+$/, '')
  return trimmed === '' ? '/' : trimmed
}

const indexSrc = read('src/server/index.ts')

// 1. mount table: factory name -> mount prefix
const mounts: Record<string, string> = {}
for (const m of indexSrc.matchAll(/app\.use\(\s*'([^']*)'\s*,\s*create([A-Za-z]+)Routes\(/g)) {
  mounts[`create${m[2]}Routes`] = m[1]
}
// routers built inline and mounted in index.ts
for (const m of indexSrc.matchAll(/const\s+(\w+)\s*=\s*express\.Router\(\)/g)) {
  mounts[m[1]] = ''
}
const inlineMounts: Record<string, string> = {}
for (const m of indexSrc.matchAll(/app\.use\(\s*'([^']*)'\s*,\s*(\w+)\s*\)/g)) {
  if (mounts[m[2]] !== undefined) inlineMounts[m[2]] = m[1]
}
for (const [router, prefix] of Object.entries(inlineMounts)) {
  mounts[router] = prefix
}

console.error(`mount table: ${Object.keys(mounts).length} routers`)
for (const [k, v] of Object.entries(mounts)) console.error(`  ${k} -> '${v}'`)

// 2. scan route modules
const EXTRA_MOUNTS: Record<string, string> = {
  tasks: '/api',
  providerAuth: '/api/provider-auth',
  'session-favorite': '/api',
}
const routeDir = join(repoRoot, 'src/server/routes')
const files = readdirSync(routeDir).filter(
  f =>
    f.endsWith('.ts') &&
    !f.endsWith('.test.ts') &&
    // crud-helpers exports a router factory that is mounted under several
    // prefixes; scanning it directly would emit unprefixed phantom routes.
    f !== 'crud-helpers.ts'
)

interface Reg {
  method: string
  path: string
}
const collected: Reg[] = []

for (const file of files) {
  const src = readFileSync(join(routeDir, file), 'utf8')
  // Factory names are camelCase (createPluginsRoutes) while files are
  // lower-case (plugins.ts), so capitalise before building the lookup key.
  // Some factories are singular while the file is plural
  // (plugins.ts -> createPluginRoutes), so try both spellings.
  const baseName = file.replace(/\.ts$/, '')
  // camelCase the file stem, dropping separators: dev-server.ts -> DevServer
  const camelName = baseName
    .split(/[-_.]/)
    .filter(Boolean)
    .map(part => part.charAt(0).toUpperCase() + part.slice(1))
    .join('')
  const singular = camelName.endsWith('s') ? camelName.slice(0, -1) : camelName
  // Some modules are mounted under a hand-written router variable rather than
  // a create<Name>Routes factory (e.g. tasks.ts -> tasksRouter -> /api).
  const prefix =
    mounts[`create${camelName}Routes`] ??
    mounts[`create${singular}Routes`] ??
    EXTRA_MOUNTS[baseName] ??
    ''

  // Match router.<verb>('path' — verbs include `all`, and the path literal may
  // be preceded by arbitrary whitespace/newlines.
  const verbRe = /\b(?:(\w*[Rr]outer)|router)\.(get|post|put|patch|delete|all)\(\s*'([^']*)'/g
  for (const m of src.matchAll(verbRe)) {
    collected.push({ method: m[2].toUpperCase(), path: joinPaths(prefix, m[3]) })
  }
}

// 2b. createCrudRoutes() returns a generic CRUD router reused by several
// resources. Its inner paths are declared once in crud-helpers.ts but are
// mounted under each resource's prefix, so expand them per resource.
const crudHelpers = read('src/server/routes/crud-helpers.ts')
const crudSrc = crudHelpers.slice(
  crudHelpers.indexOf('export function createCrudRoutes')
)
const crudRoutes = [...crudSrc.matchAll(/\brouter\.(get|post|put|patch|delete|all)\(\s*'([^']*)'/g)].map(m => ({
  method: m[1].toUpperCase(),
  path: m[2],
}))
const CRUD_MOUNTS: Record<string, string> = {
  createAgentRoutes: '/api/agents',
  createSkillRoutes: '/api/skills',
  createCommandRoutes: '/api/commands',
  createWorkflowRoutes: '/api/workflows',
}
for (const [factory, prefix] of Object.entries(CRUD_MOUNTS)) {
  for (const r of crudRoutes) collected.push({ method: r.method, path: joinPaths(prefix, r.path) })
}
console.error(`crud-helpers expanded onto ${Object.keys(CRUD_MOUNTS).length} resources`)

// 3. routes declared directly on the app in index.ts
for (const m of indexSrc.matchAll(/\bapp\.(get|post|put|patch|delete|all)\(\s*'([^']*)'/g)) {
  collected.push({ method: m[1].toUpperCase(), path: joinPaths('', m[2]) })
}

const seen = new Set<string>()
const resolved = collected
  .flatMap(r =>
    r.method === 'ALL'
      ? ['GET', 'POST', 'PUT', 'PATCH', 'DELETE'].map(m => ({ method: m, path: r.path }))
      : [r]
  )
  .map(r => ({ method: r.method, path: r.path || '/' }))
  .filter(r => {
    const key = `${r.method} ${r.path}`
    if (seen.has(key)) return false
    seen.add(key)
    return true
  })
  .sort((a, b) => (a.path === b.path ? a.method.localeCompare(b.method) : a.path.localeCompare(b.path)))

writeFileSync(join(outDir, 'rest-routes.json'), JSON.stringify(resolved, null, 2))
console.error(`wrote ${resolved.length} REST routes`)
