#!/usr/bin/env node
// CyberManju OS — Generate icon set (scripts/generate-icon-set.mjs).
/**
 * Offline Iconify bundle builder.
 *
 * Scans src/** for icon names ("solar:trash-bin-trash-bold", "mdi:…") and writes only the
 * referenced icon bodies into src/assets/iconify-local.json. AppIcon loads that
 * file through addCollection() so nothing is ever fetched from api.iconify.design.
 *
 * Run automatically by `npm run icons` (predev / prebuild).
 */
import { readFileSync, writeFileSync, readdirSync, statSync, existsSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { dirname, join, resolve } from 'node:path'

const __dirname = dirname(fileURLToPath(import.meta.url))
const root = resolve(__dirname, '..')
const srcDir = join(root, 'src')
const outFile = join(srcDir, 'assets', 'iconify-local.json')

// Matches "solar:trash-bin-trash-bold". The prefix is then validated against the installed
// @iconify-json/* sets, so ordinary "class: foo" object literals are ignored.
const NAME_RE = /(?<![\w.-])([a-z][a-z0-9-]{1,31}):([a-z0-9][a-z0-9-]{1,63})/g

/** Prefixes we are allowed to reference: the sets actually installed. */
function installedPrefixes() {
  const dir = join(root, 'node_modules', '@iconify-json')
  if (!existsSync(dir)) return new Set()
  return new Set(
    readdirSync(dir).filter((entry) =>
      existsSync(join(dir, entry, 'icons.json'))
    )
  )
}

/**
 * Icon-set prefixes that must not be referenced unless their package is
 * installed. An uninstalled prefix is otherwise indistinguishable from
 * incidental "foo:bar" text, so it is silently skipped — which would render
 * an invisible spacer instead of failing the build.
 */
const KNOWN_SETS = new Set([
  'lucide',
  'mdi',
  'solar',
  'tabler',
  'ph',
  'ri',
  'bi',
  'carbon',
  'material-symbols',
  'heroicons',
  'iconoir',
  'mingcute',
  'hugeicons',
])

function walk(dir, out = []) {
  for (const entry of readdirSync(dir)) {
    const full = join(dir, entry)
    const st = statSync(full)
    if (st.isDirectory()) walk(full, out)
    else if (/\.(vue|ts|js|mjs)$/.test(entry)) out.push(full)
  }
  return out
}

/** Collect "prefix:name" references, ignoring comments and non-icon prefixes. */
function collectRefs(allowed) {
  const refs = new Set()
  const orphans = []
  for (const file of walk(srcDir)) {
    if (file.endsWith('iconify-local.json')) continue
    const text = readFileSync(file, 'utf8')
    const withoutComments = text
      .replace(/\/\*[\s\S]*?\*\//g, ' ')
      .replace(/(^|[^:])\/\/[^\n]*/g, '$1 ')
    let m
    NAME_RE.lastIndex = 0
    while ((m = NAME_RE.exec(withoutComments)) !== null) {
      const prefix = m[1]
      if (allowed.has(prefix)) {
        refs.add(`${prefix}:${m[2]}`)
      } else if (KNOWN_SETS.has(prefix)) {
        orphans.push({ file: file.slice(root.length + 1), name: `${prefix}:${m[2]}` })
      }
    }
  }
  return { refs: [...refs].sort(), orphans }
}

function loadCollection(prefix) {
  const pkg = join(root, 'node_modules', '@iconify-json', prefix, 'icons.json')
  if (!existsSync(pkg)) {
    console.error(
      `\n  Missing icon set "@iconify-json/${prefix}".\n` +
        `  Install it with:  npm i -D @iconify-json/${prefix}\n`
    )
    process.exit(1)
  }
  return JSON.parse(readFileSync(pkg, 'utf8'))
}

/** Resolve a name through the collection's aliases to its body + dimensions. */
function resolveIcon(collection, name) {
  const { icons = {}, aliases = {} } = collection
  if (icons[name]) return { name, body: icons[name] }

  const seen = new Set()
  let cursor = name
  while (aliases[cursor] && !seen.has(cursor)) {
    seen.add(cursor)
    cursor = aliases[cursor].parent
    if (icons[cursor]) return { name, body: icons[cursor] }
  }
  return null
}

const { refs, orphans } = collectRefs(installedPrefixes())

if (orphans.length) {
  const sets = [...new Set(orphans.map((o) => o.name.split(':')[0]))]
  console.error('\n  Reference(s) to an icon set that is not installed:\n')
  for (const o of orphans) console.error(`    - ${o.name}   (${o.file})`)
  console.error(
    `\n  Install with:  npm i -D ${sets.map((s) => `@iconify-json/${s}`).join(' ')}` +
      '\n  Or switch the reference to an installed set.\n'
  )
  process.exit(1)
}

const byPrefix = new Map()
for (const ref of refs) {
  const idx = ref.indexOf(':')
  const prefix = ref.slice(0, idx)
  if (!byPrefix.has(prefix)) byPrefix.set(prefix, [])
  byPrefix.get(prefix).push(ref.slice(idx + 1))
}

const missing = []
const output = {}

for (const [prefix, names] of [...byPrefix].sort()) {
  const collection = loadCollection(prefix)
  const subset = {
    prefix,
    icons: {},
  }
  for (const key of ['width', 'height', 'left', 'top', 'display']) {
    if (collection[key] !== undefined) subset[key] = collection[key]
  }

  let kept = 0
  for (const name of [...new Set(names)].sort()) {
    const hit = resolveIcon(collection, name)
    if (!hit) {
      missing.push(`${prefix}:${name}`)
      continue
    }
    subset.icons[name] = hit.body
    kept++
  }
  if (kept > 0) output[prefix] = subset
}

if (missing.length) {
  console.error('\n  Unknown icon name(s) — not in the installed icon set:\n')
  for (const name of missing) console.error(`    - ${name}`)
  console.error('\n  Fix the name or install the set that provides it.\n')
  process.exit(1)
}

let serialised = JSON.stringify(output)
// icons.json bodies are already compact; one more pass trims pretty-printing.
writeFileSync(outFile, serialised + '\n', 'utf8')

const total = Object.values(output).reduce((n, c) => n + Object.keys(c.icons).length, 0)
console.log(
  `iconify-local.json: ${total} icon(s) from ${Object.keys(output).length} set(s) ` +
    `(${(Buffer.byteLength(serialised) / 1024).toFixed(1)} KB)`
)
