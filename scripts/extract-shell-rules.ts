/**
 * P2 contract: extract the pure shell helpers.
 *
 * detectGitMutation, formatDuration and hasBackgroundAmpersand are exported and
 * side-effect free, so the original can be called directly.
 *
 * Usage: npx tsx scripts/extract-shell-rules.ts <outFile>
 */
import { writeFileSync, mkdirSync } from 'node:fs'
import { dirname } from 'node:path'

const outFile = process.argv[2] ?? '/tmp/contract/shell-rules.json'
mkdirSync(dirname(outFile), { recursive: true })

const sh: any = await import('../src/server/tools/shell.js')

// --- detectGitMutation --------------------------------------------------
const gitCommands = [
  'git checkout main',
  'git checkout -b feature',
  "git checkout 'main'",
  'git switch main',
  'git branch -d old',
  'git branch --delete old',
  'git branch -D old',
  'git branch -M name',
  'git merge develop',
  'git rebase main',
  'git reset --hard',
  'git cherry-pick abc123',
  'git worktree add ../wt',
  'git clone https://example.com/r.git',
  'git pull',
  'git push origin main',
  'git fetch',
  'git update-ref refs/heads/x',
  'git symbolic-ref HEAD refs/heads/main',
  'git status',
  'git log',
  'git commit -m "x"',
  'git add .',
  'git diff',
  // The quoted-verb bypass the original explicitly refuses to allow.
  'git "checkout" main',
  "git 'switch' main",
  // Not git at all.
  'echo checkout main',
  'sudo git pull',
  'mygit checkout main',
  'git-checkout main',
  'cat file | grep checkout',
  'git  checkout  main',
  'git\ncheckout\nmain',
]

const gitCases = gitCommands.map(cmd => {
  let got: unknown = 'threw'
  try {
    got = sh.detectGitMutation(cmd)
  } catch (err) {
    got = 'threw: ' + String(err).slice(0, 120)
  }
  return { fn: 'detectGitMutation', input: cmd, expect: got }
})

// --- formatDuration -----------------------------------------------------
const durations = [
  0, 1, 999, 1000, 1001, 1500, 9999, 10_000, 59_999, 60_000, 60_001, 60_500,
  61_000, 90_000, 119_999, 120_000, 125_000, 599_999, 600_000, 3_599_000,
  3_600_000, 86_399_000, -1, 1.5, 999.4, 999.5, 1059.9,
]

const durationCases = durations.map(ms => {
  let got: unknown = 'threw'
  try {
    got = sh.formatDuration(ms)
  } catch (err) {
    got = 'threw: ' + String(err).slice(0, 120)
  }
  return { fn: 'formatDuration', input: ms, expect: got }
})

// --- hasBackgroundAmpersand --------------------------------------------
const ampCommands = [
  'sleep 10 &',
  'sleep 10 & wait',
  'echo "a & b"',
  "echo 'a & b'",
  'echo \\& not-background',
  'cmd1 && cmd2',
  'cmd 2>&1',
  'cmd &> file',
  'cmd |& other',
  'cmd >&2',
  'a & b & c',
  'no ampersand',
  '',
  '&',
  '&&',
  'echo "quote with && inside"',
  "echo 'quote with |& inside'",
  'echo done\\&',
]

const ampCases = ampCommands.map(cmd => {
  let got: unknown = 'threw'
  try {
    got = sh.hasBackgroundAmpersand(cmd)
  } catch (err) {
    got = 'threw: ' + String(err).slice(0, 120)
  }
  return { fn: 'hasBackgroundAmpersand', input: cmd, expect: got }
})

writeFileSync(
  outFile,
  JSON.stringify(
    {
      description:
        'Pure helpers from src/server/tools/shell.ts, captured by calling the original. detectGitMutation refuses to strip quotes, which is what stops `git "checkout"` from bypassing detection; hasBackgroundAmpersand strips quotes and the operator forms that merely contain &.',
      gitCases,
      durationCases,
      ampCases,
    },
    null,
    2
  )
)
console.error(`wrote ${gitCases.length} git, ${durationCases.length} duration, ${ampCases.length} amp cases`)
for (const c of gitCases) console.error(`  git  ${JSON.stringify(c.input).padEnd(42)} -> ${JSON.stringify(c.expect)}`)
for (const c of durationCases) console.error(`  dur  ${String(c.input).padEnd(42)} -> ${JSON.stringify(c.expect)}`)
for (const c of ampCases) console.error(`  amp  ${JSON.stringify(c.input).padEnd(42)} -> ${JSON.stringify(c.expect)}`)

process.exit(0);
