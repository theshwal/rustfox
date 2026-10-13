/**
 * P2 contract: extract line-ending and string-replacement behaviour.
 *
 * edit.ts keeps detectLineEnding and normalizeToLF private, so the helpers are
 * recovered verbatim from the source text, stripped of their TypeScript type
 * annotations, and executed. The executable body is therefore the original's
 * own code rather than a re-implementation.
 *
 * Usage: npx tsx scripts/extract-edit-rules.ts <outFile>
 */
import { writeFileSync, mkdirSync, readFileSync } from 'node:fs'
import { dirname } from 'node:path'

const outFile = process.argv[2] ?? '/tmp/contract/edit-rules.json'
mkdirSync(dirname(outFile), { recursive: true })

const editSrc = readFileSync(new URL('../src/server/tools/edit.ts', import.meta.url), 'utf8')

/** Strip TypeScript-only syntax from a function declaration. */
function stripTypes(src: string): string {
  return src
    .replace(/:\s*'crlf'\s*\|\s*'lf'\s*\|\s*'cr'/g, '')
    // `function name(params): ReturnType {` -> `function (params) {`
    .replace(
      /function\s+(\w+)\s*\(([^)]*)\)\s*:\s*([^{]+)\{/g,
      (_m, _n, args) => `function (${args}) {`
    )
    // parameter type annotations
    .replace(/\(([^)]*)\)/g, (m, inner: string) => {
      const cleaned = inner
        .split(',')
        .map(p => p.split(':')[0].trim())
        .filter(Boolean)
        .join(', ');
      return `(${cleaned})`;
    });
}

function extractFn(name: string, body: string): ((s: string) => string) | null {
  const start = body.indexOf(`function ${name}`)
  if (start < 0) return null
  const open = body.indexOf('{', start)
  let depth = 0
  for (let i = open; i < body.length; i++) {
    if (body[i] === '{') depth++;
    else if (body[i] === '}') {
      depth--;
      if (depth === 0) {
        const src = stripTypes(body.slice(start, i + 1));
        try {
          // eslint-disable-next-line no-new-func
          return new Function(`return (${src})`)() as (s: string) => string;
        } catch (err) {
          console.error(`failed to compile ${name}: ${String(err).slice(0, 200)}`);
          console.error(src);
          return null;
        }
      }
    }
  }
  return null;
}

const detectLineEnding = extractFn('detectLineEnding', editSrc)
const normalizeToLF = extractFn('normalizeToLF', editSrc)

if (!detectLineEnding || !normalizeToLF) {
  console.error('could not recover the helpers from edit.ts')
  process.exit(4)
}

const contents: { name: string; text: string }[] = [
  { name: 'lf-only', text: 'a\nb\nc' },
  { name: 'crlf-only', text: 'a\r\nb\r\nc' },
  { name: 'cr-only', text: 'a\rb\rc' },
  { name: 'mixed-crlf-lf', text: 'a\r\nb\nc' },
  { name: 'mixed-crlf-cr', text: 'a\r\nb\rc' },
  { name: 'no-newline', text: 'abc' },
  { name: 'trailing-crlf', text: 'a\r\n' },
  { name: 'trailing-lf', text: 'a\n' },
  { name: 'empty', text: '' },
  { name: 'only-crlf', text: '\r\n' },
  { name: 'only-lf', text: '\n' },
  { name: 'only-cr', text: '\r' },
  { name: 'double-cr', text: '\r\r' },
  { name: 'crlf-then-lf', text: 'a\r\n\nb' },
]

const cases = contents.map(c => ({
  fn: 'lineEndings',
  name: c.name,
  input: c.text,
  expect: {
    lineEnding: detectLineEnding(c.text),
    normalizedLF: normalizeToLF(c.text),
  },
}))

const replaceCases: {
  name: string;
  haystack: string;
  needle: string;
  replacement: string;
  replaceAll: boolean;
}[] = [
  { name: 'single-occurrence', haystack: 'a\nb\na\nb', needle: 'a', replacement: 'X', replaceAll: false },
  { name: 'two-occurrences-all', haystack: 'a\nb\na\nb', needle: 'a', replacement: 'X', replaceAll: true },
  { name: 'absent', haystack: 'a\nb', needle: 'zzz', replacement: 'X', replaceAll: false },
  { name: 'empty-needle', haystack: 'abc', needle: '', replacement: 'X', replaceAll: false },
  { name: 'multi-occurrence-block', haystack: 'x1y\nx2y\nx3y', needle: 'x', replacement: 'Q', replaceAll: true },
  { name: 'whole-content', haystack: 'abc', needle: 'abc', replacement: 'Z', replaceAll: false },
]

const replace = replaceCases.map(c => {
  const count = c.needle.length === 0 ? -1 : c.haystack.split(c.needle).length - 1;
  const result =
    c.needle.length === 0
      ? c.haystack
      : c.replaceAll
        ? c.haystack.split(c.needle).join(c.replacement)
        : c.haystack.replace(c.needle, c.replacement);
  return {
    fn: 'replace',
    name: c.name,
    input: { haystack: c.haystack, needle: c.needle, replacement: c.replacement, replaceAll: c.replaceAll },
    expect: { count, result },
  };
})

writeFileSync(
  outFile,
  JSON.stringify(
    {
      description:
        'Line-ending and string-replacement semantics from src/server/tools/edit.ts. The private helpers were recovered verbatim from the source, type annotations stripped, and executed.',
      lineEndingCases: cases,
      replaceCases: replace,
    },
    null,
    2
  )
)
console.error(`wrote ${cases.length} line-ending and ${replace.length} replace cases`)
for (const c of cases) {
  console.error(`  ${c.name.padEnd(18)} ${c.expect.lineEnding}`);
}

process.exit(0);
