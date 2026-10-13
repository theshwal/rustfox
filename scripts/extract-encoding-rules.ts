/**
 * P2 contract: extract encoding detection/decoding behaviour.
 *
 * src/server/utils/encoding.ts is pure and is used by read, write and edit, so
 * its decisions have to match before any of those tools can. Captured by
 * calling the original directly.
 *
 * Usage: npx tsx scripts/extract-encoding-rules.ts <outFile>
 */
import { writeFileSync, mkdirSync } from 'node:fs'
import { dirname } from 'node:path'

const outFile = process.argv[2] ?? '/tmp/contract/encoding-rules.json'
mkdirSync(dirname(outFile), { recursive: true })

const enc: any = await import('../src/server/utils/encoding.js')

function b64(buf: Buffer): string {
  return buf.toString('base64')
}
function unb64(s: string): Buffer {
  return Buffer.from(s, 'base64')
}

const latin1 = (s: string) => Buffer.from(s, 'binary')
const utf16le = (s: string) => Buffer.from(s, 'utf16le')

const samples: { name: string; data: string }[] = [
  { name: 'empty', data: '' },
  { name: 'ascii', data: b64(Buffer.from('hello world')) },
  { name: 'utf8-accented', data: b64(Buffer.from('héllo wörld', 'utf8')) },
  { name: 'utf8-emoji', data: b64(Buffer.from('a🎉b', 'utf8')) },
  { name: 'utf8-cjk', data: b64(Buffer.from('日本語テキスト', 'utf8')) },
  { name: 'bom-utf8', data: b64(Buffer.concat([Buffer.from([0xef, 0xbb, 0xbf]), Buffer.from('hello')])) },
  { name: 'bom-utf16le', data: b64(Buffer.concat([Buffer.from([0xff, 0xfe]), utf16le('hello')])) },
  { name: 'bom-utf16be', data: b64(Buffer.concat([Buffer.from([0xfe, 0xff]), Buffer.from([0, 0x68, 0, 0x65])])) },
  { name: 'bom-utf32le', data: b64(Buffer.concat([Buffer.from([0xff, 0xfe, 0, 0]), Buffer.from([0x68, 0, 0, 0])])) },
  { name: 'bom-utf32be', data: b64(Buffer.concat([Buffer.from([0, 0, 0xfe, 0xff]), Buffer.from([0, 0, 0, 0x68])])) },
  { name: 'latin1-high', data: b64(latin1('caf\xe9 na\xefve')) },
  { name: 'latin1-invalid-utf8', data: b64(Buffer.from([0xff, 0xfe, 0x41, 0xff, 0x42])) },
  { name: 'invalid-utf8-run', data: b64(Buffer.from([0xc3, 0x28, 0xa0, 0xa1, 0xe2, 0x28, 0xa1])) },
  { name: 'single-byte-high', data: b64(latin1('na\xefve r\xe9sum\xe9')) },
]

const detectCases = samples.map(s => {
  const buf = unb64(s.data)
  let got: unknown = 'threw'
  let error: string | null = null
  try {
    const r = enc.detectEncoding(buf)
    got = { encoding: r.encoding, confidence: r.confidence, bomSize: r.bomSize }
  } catch (err) {
    error = String(err).slice(0, 200)
  }
  return { fn: 'detectEncoding', name: s.name, data: s.data, expect: got, error }
})

// decodeContent / encodeContent round-trips across the encodings the original
// supports, including the BOM flag on encode.
const encodings = ['utf-8', 'ISO-8859-1', 'windows-1252', 'Shift_JIS', 'EUC-JP', 'utf-16le', 'utf-16be']
const texts = ['plain', 'héllo', '日本', 'mixed 123 text']

const decodeCases: unknown[] = []
const encodeCases: unknown[] = []
for (const encoding of encodings) {
  for (const text of texts) {
    let decoded: unknown = 'threw'
    let encodedB64: string | null = null
    let decodedWithBom: unknown = 'threw'
    let error: string | null = null
    try {
      const bytes = enc.encodeContent(text, encoding, false)
      encodedB64 = b64(bytes)
      decoded = enc.decodeContent(bytes, encoding)
      decodedWithBom = enc.decodeContent(enc.encodeContent(text, encoding, true), encoding)
    } catch (err) {
      error = String(err).slice(0, 200)
    }
    decodeCases.push({ fn: 'decodeContent', encoding, text, expect: decoded, error })
    encodeCases.push({ fn: 'encodeContent', encoding, text, expect: encodedB64, error })
    void decodedWithBom
  }
}

writeFileSync(
  outFile,
  JSON.stringify(
    {
      description:
        'Encoding behaviour captured by calling src/server/utils/encoding.ts directly. detectEncoding fixtures include BOM variants and invalid UTF-8; decode/encode cover the supported encodings and a BOM round-trip.',
      detectCases,
      decodeCases,
      encodeCases,
    },
    null,
    2
  )
)
console.error(
  `wrote ${detectCases.length} detect, ${decodeCases.length} decode, ${encodeCases.length} encode cases`
)
for (const c of detectCases as { name: string; expect: unknown }[]) {
  console.error(`  ${c.name.padEnd(22)} ${JSON.stringify(c.expect)}`)
}

process.exit(0)
