/**
 * P2 contract: extract checkPathsAccess behaviour.
 *
 * checkPathsAccess is what read, write, edit and shell all funnel through
 * before touching a path, so its aggregation rules matter. It is async and
 * resolves through realpath, so it is called directly here against a workdir
 * that is NOT under /tmp: /tmp is an always-allowed root, and a workdir there
 * would make every sandbox check pass trivially.
 *
 * Usage: npx tsx scripts/extract-path-access.ts <outFile>
 */
import { writeFileSync, mkdirSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { mkdirSync as mkDirSync, writeFileSync as writeFile, rmSync } from 'node:fs'
import { homedir } from 'node:os'

const outFile = process.argv[2] ?? '/tmp/contract/path-access.json'
mkdirSync(dirname(outFile), { recursive: true })

const ps: any = await import('../src/server/tools/path-security.js')

// Deliberately outside /tmp and /var/tmp, which are always allowed.
//
// The directory is a fixed path rather than a mkdtemp one, so regenerating the
// contract produces a stable diff: a random name would show up as a change on
// every run and hide real drift. It is recreated from scratch each time.
const workdir = join(homedir(), 'openfox-path-access-fixture')
rmSync(workdir, { recursive: true, force: true })
mkdirSync(workdir, { recursive: true })
mkDirSync(join(workdir, 'src'), { recursive: true })
writeFile(join(workdir, 'src', 'index.ts'), 'export {}\n')
writeFile(join(workdir, '.env'), 'SECRET=1\n')
writeFile(join(workdir, 'config.json'), '{}\n')
mkDirSync(join(workdir, 'id_rsa'), { recursive: true })

const cases: { name: string; paths: string[] }[] = [
  { name: 'empty', paths: [] },
  { name: 'single-inside', paths: ['src/index.ts'] },
  { name: 'outside-etc', paths: ['/etc/passwd'] },
  { name: 'inside-sensitive-env', paths: ['.env'] },
  { name: 'outside-and-sensitive', paths: ['/etc/passwd', '.env'] },
  { name: 'duplicates', paths: ['/etc/passwd', '/etc/passwd'] },
  { name: 'duplicates-mixed-form', paths: ['/etc/passwd', '/etc/../etc/passwd'] },
  { name: 'allowed-root-tmp', paths: ['/tmp/whatever'] },
  { name: 'sensitive-outside', paths: ['/root/.env'] },
  { name: 'id-rsa-directory', paths: ['id_rsa'] },
  { name: 'many-mixed', paths: ['src/index.ts', 'config.json', '/etc/shadow', '.env'] },
  { name: 'traversal-inside', paths: ['src/../config.json'] },
  { name: 'traversal-outside', paths: ['../outside.txt'] },
  { name: 'dev-null', paths: ['/dev/null'] },
  { name: 'sibling-workdir', paths: [join(workdir, '..', 'sibling.txt')] },
  { name: 'absolute-inside', paths: [join(workdir, 'src', 'index.ts')] },
  { name: 'pem-outside', paths: ['/etc/ssl/private/key.pem'] },
]

const results = []
for (const c of cases) {
  let got: unknown = 'threw'
  let error: string | null = null
  try {
    const r = await ps.checkPathsAccess(c.paths, workdir)
    got = {
      needsConfirmation: r.needsConfirmation,
      deniedPaths: r.deniedPaths,
      sensitivePaths: r.sensitivePaths,
    }
  } catch (err) {
    error = String(err).slice(0, 200)
  }
  results.push({ name: c.name, paths: c.paths, expect: got, error })
}

writeFileSync(
  outFile,
  JSON.stringify(
    {
      description:
        'checkPathsAccess behaviour captured by calling the original against a real workdir outside /tmp. deniedPaths/sensitivePaths are resolved paths, deduplicated. Relative inputs resolve against the process cwd, which is why they do not point at workdir.',
      workdir,
      workdirLayout: ['src/index.ts', '.env', 'config.json', 'id_rsa/ (directory)'],
      cases: results,
    },
    null,
    2
  )
)
console.error(`wrote ${results.length} cases; workdir=${workdir}`)
for (const r of results) {
  const e = r.expect as Record<string, unknown>
  console.error(
    `  ${r.name.padEnd(22)} confirm=${String(e?.needsConfirmation).padEnd(6)} denied=${JSON.stringify(e?.deniedPaths)} sensitive=${JSON.stringify(e?.sensitivePaths)}`
  )
}

process.exit(0);
