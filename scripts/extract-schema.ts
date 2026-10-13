/**
 * P0: extract the authoritative SQLite schema by creating a real database
 * with the original code, then dumping its structure. The output is the
 * schema as actually created, not a transcription.
 *
 * Usage: npx tsx scripts/extract-schema.ts <outFile>
 */
import { mkdtempSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import Database from 'better-sqlite3'

const outFile = process.argv[2] ?? '/tmp/contract/db-schema.sql'
const dir = mkdtempSync(join(tmpdir(), 'openfox-schema-'))
const dbPath = join(dir, 'schema.db')

process.env['OPENFOX_DB_PATH'] = dbPath
process.env['OPENFOX_LOG_LEVEL'] = 'error'
process.env['OPENFOX_MOCK_LLM'] = 'true'

const { loadConfig } = await import('../src/server/config.js')
const { initDatabase } = await import('../src/server/db/index.js')

const config = loadConfig()
config.database.path = dbPath
const db = initDatabase(config)
const rows = db
  .prepare(
    `SELECT type, name, sql FROM sqlite_master
     WHERE sql IS NOT NULL AND name NOT LIKE 'sqlite_%'
     ORDER BY CASE type WHEN 'table' THEN 0 WHEN 'index' THEN 1 ELSE 2 END, name`
  )
  .all() as { type: string; name: string; sql: string }[]

const header = `-- Extracted from co-l/openfox develop (runtime-generated, not hand-written).
-- Source: src/server/db/index.ts executed against a fresh SQLite database.

PRAGMA foreign_keys = ON;
`
const body = rows.map(r => `${r.sql.trim()};`).join('\n\n')
writeFileSync(outFile, header + '\n' + body + '\n')
console.error(`wrote ${rows.length} objects to ${outFile}`)
db.close()
process.exit(0)
