/**
 * P0: extract the WebSocket contract from the original server.
 *
 * The handler's switch labels give the authoritative inbound vocabulary, and
 * each case is classified by actually driving a live socket: a message is
 * either deprecated (server answers DEPRECATED_MESSAGE_TYPE), invalid (server
 * answers INVALID_PAYLOAD), or live (accepted and routed).
 *
 * Usage: npx tsx scripts/extract-ws.ts <outFile>
 */
import { writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { randomUUID } from 'node:crypto'
import { readFileSync } from 'node:fs'

const outFile = process.argv[2] ?? '/tmp/contract/ws-protocol.json'

process.env['OPENFOX_MOCK_LLM'] = 'true'
process.env['OPENFOX_DB_PATH'] = ':memory:'
process.env['OPENFOX_LOG_LEVEL'] = 'error'
process.env['OPENFOX_HOST'] = '127.0.0.1'
process.env['OPENFOX_PORT'] = '0'

const { loadConfig } = await import('../src/server/config.js')
const { createServerHandle } = await import('../src/server/index.js')

// Pull the inbound vocabulary straight from the handler source.
const handlerSrc = readFileSync(new URL('../src/server/ws/server.ts', import.meta.url), 'utf8')
const inbound = [...handlerSrc.matchAll(/case '([a-z]+\.[a-zA-Z.]+)':/g)].map(m => m[1])
const vocabulary = [...new Set(inbound)].sort()

const config = loadConfig()
config.mode = 'test'
config.globalConfigPath = join(tmpdir(), `openfox-ws-${randomUUID()}.json`)

const handle = await createServerHandle(config)
const { port } = await handle.start(0)
console.error(`server on ${port}`)

const { WebSocket } = await import('ws')

function classify(type) {
  return new Promise(resolvePromise => {
    const ws = new WebSocket(`ws://127.0.0.1:${port}/ws`)
    const timer = setTimeout(() => {
      ws.close()
      resolvePromise({ type, classification: 'timeout' })
    }, 5000)

    ws.on('open', () => {
      ws.send(JSON.stringify({ id: 'probe-1', type, payload: {} }))
    })
    ws.on('message', data => {
      let msg
      try {
        msg = JSON.parse(data.toString())
      } catch {
        return
      }
      const code = msg?.error?.code ?? msg?.code ?? msg?.payload?.code
      const kind = String(code ?? msg?.type ?? '')
      let classification = 'accepted'
      if (kind.includes('DEPRECATED')) classification = 'deprecated'
      else if (kind.includes('INVALID')) classification = 'invalid_payload'
      else if (kind === 'error') classification = 'error'
      clearTimeout(timer)
      ws.close()
      resolvePromise({ type, classification, code: code ?? null, responseType: msg?.type ?? null })
    })
    ws.on('error', err => {
      clearTimeout(timer)
      resolvePromise({ type, classification: 'socket_error', error: String(err) })
    })
  })
}

const messages = []
for (const type of vocabulary) {
  messages.push(await classify(type))
}

const schema = {
  $schema: 'https://json-schema.org/draft/2020-12/schema',
  title: 'openfox websocket protocol',
  description:
    'Inbound message vocabulary extracted from co-l/openfox develop by driving a live server. classification: deprecated = server must answer DEPRECATED_MESSAGE_TYPE; invalid_payload = answer INVALID_PAYLOAD for the empty payload sent; accepted = routed for processing.',
  transport: { path: '/ws', protocol: 'ws', envelope: { id: 'string', type: 'string', payload: 'object' } },
  inbound: messages,
  vocabularySize: vocabulary.length,
}

writeFileSync(outFile, JSON.stringify(schema, null, 2))
console.error(`wrote ${messages.length} inbound message types to ${outFile}`)

await handle.close()
process.exit(0)
