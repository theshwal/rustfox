/**
 * P2 contract: extract encodeContent and the tool schemas.
 *
 * write.ts exposes its JSON Schema through the tool object; that schema is what
 * the model actually sees, so it is part of the contract. encodeContent is pure
 * and is the piece write depends on.
 *
 * Usage: npx tsx scripts/extract-write-rules.ts <outFile>
 */
import { writeFileSync, mkdirSync } from 'node:fs'
import { dirname } from 'node:path'

const outFile = process.argv[2] ?? '/tmp/contract/write-rules.json'
mkdirSync(dirname(outFile), { recursive: true })

const enc: any = await import('../src/server/utils/encoding.js')

const encodings = [
  'utf-8', 'UTF-8', 'utf8', 'ascii',
  'utf-16le', 'UTF-16', 'utf-16be',
  'utf-32le', 'utf-32', 'utf-32be',
  'ISO-8859-1', 'latin1', 'windows-1252',
  'Shift_JIS', 'EUC-JP', 'EUC-KR', 'GB2312', 'GB18030', 'Big5',
  'KOI8-R', 'KOI8-U',
  'unknown-encoding',
]

const texts = ['', 'plain', 'héllo', '日本語', 'a🎉b', 'line\nbreak', 'tab\there']

const encodeCases: unknown[] = []
for (const encoding of encodings) {
  for (const text of texts) {
    for (const addBom of [false, true]) {
      let got: unknown = 'threw'
      let error: string | null = null
      try {
        got = (enc.encodeContent(text, encoding, addBom) as Buffer).toString('base64')
      } catch (err) {
        error = String(err).slice(0, 160)
      }
      encodeCases.push({ encoding, text, addBom, expect: got, error })
    }
  }
}

// Round trips, which is what actually matters for write: encode then decode.
const roundTrips: unknown[] = []
for (const encoding of ['utf-8', 'utf-16le', 'utf-16be', 'utf-32le', 'utf-32be', 'ISO-8859-1', 'windows-1252']) {
  for (const text of ['', 'plain', 'héllo', 'a🎉b']) {
    let got: unknown = 'threw'
    try {
      const bytes = enc.encodeContent(text, encoding, false)
      got = enc.decodeContent(bytes, encoding)
    } catch (err) {
      got = 'threw';
    }
    roundTrips.push({ encoding, text, expect: got })
  }
}

// Tool schemas as the model sees them.
const schemas: Record<string, unknown> = {}
for (const mod of ['write.js', 'edit.js', 'read.js']) {
  try {
    const m: any = await import(`../src/server/tools/${mod}`)
    for (const key of Object.keys(m)) {
      const t = m[key]
      // createTool nests the schema at definition.function, with the handler in
      // `execute`.
      const def =
        t && typeof t === 'object'
          ? t.definition?.function ?? t.definition ?? t.function
          : null
      if (def && typeof def === 'object' && def.name) {
        schemas[`${mod}:${key}`] = {
          name: def.name,
          description: def.description,
          parameters: def.parameters,
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
        'encodeContent behaviour and the JSON Schemas the tools expose to the model. Encoding names are normalised first, so utf8/ascii/UTF-16 all route to a canonical target before dispatch.',
      encodeCases,
      roundTrips,
      toolSchemas: schemas,
    },
    null,
    2
  )
)
console.error(`wrote ${(encodeCases as unknown[]).length} encode, ${(roundTrips as unknown[]).length} roundtrip cases, ${Object.keys(schemas).length} schemas`)
for (const s of Object.keys(schemas)) console.error(`  schema ${s}`)

process.exit(0);
