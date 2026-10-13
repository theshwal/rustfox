/**
 * P2 contract: extract the path-confirmation allowlist and denial messages.
 *
 * These are exported, so they can be called directly. The allowlist is module
 * state in the original, so each scenario runs in a fresh session id to stay
 * independent.
 *
 * Usage: npx tsx scripts/extract-confirmation.ts <outFile>
 */
import { writeFileSync, mkdirSync } from 'node:fs'
import { dirname } from 'node:path'

const outFile = process.argv[2] ?? '/tmp/contract/confirmation.json'
mkdirSync(dirname(outFile), { recursive: true })

const ps: any = await import('../src/server/tools/path-security.js')

const scenarios: {
  name: string;
  session: string;
  add: string[];
  check: string[];
  clear: boolean;
}[] = [
  { name: 'empty-allowlist', session: 's-empty', add: [], check: ['/etc/passwd'], clear: false },
  { name: 'exact-match', session: 's-exact', add: ['/etc/passwd'], check: ['/etc/passwd'], clear: false },
  { name: 'other-path', session: 's-other', add: ['/etc/passwd'], check: ['/etc/shadow'], clear: false },
  { name: 'trailing-slash', session: 's-slash', add: ['/etc/passwd'], check: ['/etc/passwd/'], clear: false },
  { name: 'dot-segment', session: 's-dot', add: ['/etc/passwd'], check: ['/etc/./passwd'], clear: false },
  { name: 'dotdot-normalised', session: 's-dd', add: ['/etc/passwd'], check: ['/etc/shadow/../passwd'], clear: false },
  { name: 'relative-add', session: 's-rel', add: ['relative/file.txt'], check: ['relative/file.txt'], clear: false },
  { name: 'multiple-adds', session: 's-multi', add: ['/a', '/b', '/c'], check: ['/a', '/b', '/c', '/d'], clear: false },
  { name: 'duplicate-add', session: 's-dup', add: ['/a', '/a', '/a'], check: ['/a'], clear: false },
  { name: 'cleared', session: 's-clear', add: ['/etc/passwd'], check: ['/etc/passwd'], clear: true },
  { name: 'unknown-session', session: 's-never', add: [], check: ['/anything'], clear: false },
]

const allowlist = scenarios.map(sc => {
  ps.addAllowedPaths(sc.session, sc.add)
  const beforeClear = sc.check.map(p => ({ path: p, allowed: ps.isPathAllowed(sc.session, p) }))
  if (sc.clear) ps.clearAllowedPaths(sc.session)
  const afterClear = sc.check.map(p => ({ path: p, allowed: ps.isPathAllowed(sc.session, p) }))
  return {
    name: sc.name,
    add: sc.add,
    check: sc.check,
    beforeClear,
    afterClear,
  }
})

// PathAccessDeniedError message construction, across all five reasons.
const reasons = ['outside_workdir', 'sensitive_file', 'both', 'dangerous_command', 'git_no_verify']
const denialMessages: { reason: string; paths: string[]; customMessage: string | null; message: string; name: string }[] = []

for (const reason of reasons) {
  let message = ''
  let errName = ''
  try {
    const e = new ps.PathAccessDeniedError(['/etc/passwd', '/root/.env'], 'read_file', reason)
    message = e.message
    errName = e.name
  } catch (err) {
    message = 'threw: ' + String(err).slice(0, 120)
  }
  denialMessages.push({ reason, paths: ['/etc/passwd', '/root/.env'], customMessage: null, message, name: errName })
}

// The custom message path.
let custom = ''
try {
  const e = new ps.PathAccessDeniedError(['/a'], 'write_file', 'sensitive_file', 'my own message')
  custom = e.message
} catch (err) {
  custom = 'threw'
}
// A custom message with the default reason.
let customDefault = ''
try {
  const e = new ps.PathAccessDeniedError(['/a'], 'write_file', 'outside_workdir', 'just this')
  customDefault = e.message
} catch (err) {
  customDefault = 'threw'
}

// Cancellation of unknown confirmations: both must be false / 0.
const cancelUnknown = ps.cancelPathConfirmation('no-such-call', 'because')
const cancelSessionEmpty = ps.cancelPathConfirmationsForSession('no-such-session', 'because')

writeFileSync(
  outFile,
  JSON.stringify(
    {
      description:
        'Session allowlist and denial-message behaviour captured from src/server/tools/path-security.ts. The allowlist normalises on both add and check, which is what makes trailing slashes and dot segments match.',
      allowlist,
      denialMessages,
      customMessages: { withCustom: custom, withCustomAndDefaultReason: customDefault },
      cancelUnknown,
      cancelSessionEmpty,
    },
    null,
    2
  )
)
console.error(`wrote ${allowlist.length} allowlist and ${denialMessages.length} denial cases`)
for (const a of allowlist) {
  console.error(
    `  ${a.name.padEnd(18)} before=${a.beforeClear.map(c => (c.allowed ? 'Y' : 'n')).join('')} after=${a.afterClear.map(c => (c.allowed ? 'Y' : 'n')).join('')}`
  );
}
for (const d of denialMessages) console.error(`  ${d.reason.padEnd(18)} ${d.message}`)

process.exit(0);
