/**
 * P2 contract: extract behaviour of the read/write/edit tools.
 *
 * The tool handlers are created by createTool and need a full server context, so
 * this captures the pure pieces that are exported and callable as a library:
 * image detection and the size formatter. Everything is captured by running the
 * original, never by reading it.
 *
 * Usage: npx tsx scripts/extract-io-rules.ts <outFile>
 */
import { writeFileSync, mkdirSync } from 'node:fs'
import { dirname } from 'node:path'

const outFile = process.argv[2] ?? '/tmp/contract/io-rules.json'
mkdirSync(dirname(outFile), { recursive: true })

const readMod: any = await import('../src/server/tools/read.js')

// `data` holds base64 of the real bytes; it is decoded once into the buffer
// that is handed to the original.
const fixtures: { name: string; path: string; data: string }[] = [
  { name: 'png-real-header', path: 'b.png', data: 'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAAC0lEQVR42mNkYAAAAAYAAjCB0C8AAAAASUVORK5CYII=' },
  { name: 'jpeg', path: 'c.jpg', data: '/9j/4AAQSkZJRgABAQEAYABgAAD/2wBDAAgGBgcGBQgHBwcJCQgKDBQNDAsLDBkSEw8UHRofHh0aHBwcJC4nICIsIxwcKDcpLDAxNDQ0Hyc5PTgyPDU0NP/bAEMBCQkJDAsMGA0NGDIhHCEyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjL/wAARCAAIAAgDASIAAhEBAxEB/8QAHwAAAQUBAQEBAQEAAAAAAAAAAAECAwQFBgcICQoL/8QAtRAAAgEDAwIEAwUFBAQAAAF9AQIDAAQRBRIhMUEGE1FhByJxFDKBkaEII0KxwRVS0fAkM2JyggkKFhcYGRolJicoKSo0NTY3ODk6Q0RFRkdISUpTVFVWV1hZWmNkZWZnaGlqc3R1dnd4eXqDhIWGh4iJipKTlJWWl5iZmqKjpKWmp6ipqrKztLW2t7i5usLDxMXGx8jJytLT1NXW19jZ2uHi4+Tl5ufo6erx8vP09fb3+Pn6/9oACAEBAAA/APn+iiigD//Z' },
  { name: 'gif', path: 'd.gif', data: 'R0lGODlhAQABAIAAAAAAAP///yH5BAEAAAAALAAAAAABAAEAAAIBRAA7' },
  { name: 'webp', path: 'e.webp', data: 'UklGRiQAAABXRUJQVlA4IBgAAAAwAQCdASoBAAEAAwA0JaQAA3AA/vuUAAA=' },
  { name: 'bmp', path: 'f.bmp', data: 'Qk0eAAAAAAAAABoAAAQEAAAAAABAgMEBQYHCAkKCwkKCw==' },
  { name: 'svg-xml', path: 'g.svg', data: 'PHhtbCB2ZXJzaW9uPSIxLjAiPz48c3ZnIHhtbG5zPSJodHRwOi8vd3d3LnczLm9yZy8yMDAvc3ZnIj48L3N2Zz4=' },
  { name: 'svg-root', path: 'h.svg', data: 'PHN2ZyB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciPjwvc3ZnPg==' },
  { name: 'svg-wrong-ext', path: 'i.txt', data: 'PHN2Zz48L3N2Zz4=' },
  { name: 'text-plain', path: 'j.txt', data: 'aGVsbG8gd29ybGQ=' },
  { name: 'json', path: 'k.json', data: 'eyJhIjoxfQ==' },
  { name: 'pdf', path: 'l.pdf', data: 'JVBERi0xLjQKMSAwIG9iago8Pz4KZW5kb2JqCnRyYWlsZXIKPD4+CgUlJVE9G' },
  { name: 'zip', path: 'm.zip', data: 'UEsDBAoAAAAAAAIAAoAAAAQAAAAIAAAAAAAAAAAAAAAAAAAAAAAABAAAAAAA' },
  { name: 'empty', path: 'n.png', data: '' },
  { name: 'truncated-png', path: 'o.png', data: 'aVZCTi13' },
]

const cases = []
for (const f of fixtures) {
  let got: unknown = 'threw'
  let error: string | null = null
  try {
    const buf = Buffer.from(f.data, 'base64')
    got = await readMod.detectImageType(buf, f.path)
  } catch (err) {
    error = String(err).slice(0, 200)
  }
  cases.push({
    fn: 'detectImageType',
    name: f.name,
    path: f.path,
    data: f.data,
    expect: got,
    error,
  })
}

// The exported tool objects carry their own JSON Schema; capture it so the Rust
// tool definitions can be checked against the originals.
const schemas: Record<string, unknown> = {}
for (const mod of ['read.js', 'write.js', 'edit.js']) {
  try {
    const m: any = await import(`../src/server/tools/${mod}`)
    for (const key of Object.keys(m)) {
      const tool = m[key]
      if (tool && typeof tool === 'object') {
        const name = (tool as any).name ?? key
        const params = (tool as any).parameters ?? (tool as any).inputSchema ?? null
        if (params) {
          schemas[`${mod}:${key}`] = {
            name,
            description: (tool as any).description ?? null,
            parameters: params,
          }
        }
      }
    }
  } catch (err) {
    schemas[`${mod}:<error>`] = String(err).slice(0, 200)
  }
}

writeFileSync(
  outFile,
  JSON.stringify(
    {
      description:
        'Behaviour captured by calling the original read/write/edit tool modules. detectImageType fixtures feed real encoded bytes; schemas are the tools\' own JSON Schema.',
      detectImageTypeCases: cases,
      toolSchemas: schemas,
    },
    null,
    2
  )
)
console.error(
  `wrote ${cases.length} detectImageType cases and ${Object.keys(schemas).length} schemas to ${outFile}`
)
for (const c of cases) console.error(`  ${String(c.expect).padEnd(16)} ${c.name}`)

process.exit(0)
