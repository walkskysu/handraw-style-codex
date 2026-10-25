import { readFileSync, writeFileSync } from 'node:fs'
import { resolve, dirname } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const [version, ...extra] = process.argv.slice(2)
const number = '(?:0|[1-9][0-9]*)'
const identifier = '(?:0|[1-9][0-9]*|[0-9]*[A-Za-z-][0-9A-Za-z-]*)'
const semver = new RegExp(`^${number}\\.${number}\\.${number}(?:-${identifier}(?:\\.${identifier})*)?(?:\\+[0-9A-Za-z-]+(?:\\.[0-9A-Za-z-]+)*)?$`)

function replaceVersion(section, label) {
  const field = /^version\s*=\s*"[^"]*"/gm
  if ([...section.matchAll(field)].length !== 1) {
    throw new Error(`${label}: expected exactly one version field`)
  }
  return section.replace(field, `version = "${version}"`)
}

try {
  if (!version || extra.length || !semver.test(version)) {
    throw new Error('Usage: npm run version:set -- 0.2.4 (SemVer without a v prefix)')
  }

  // Prepare every change before writing, so invalid/missing files leave versions untouched.
  const changes = []
  function update(file, transform, optional = false) {
    const path = resolve(root, file)
    let before
    try {
      before = readFileSync(path, 'utf8')
    } catch (error) {
      if (optional && error.code === 'ENOENT') return
      throw error
    }
    changes.push({ file, path, before, after: transform(before) })
  }
  function json(transform) {
    return (text) => {
      const value = JSON.parse(text)
      const before = JSON.stringify(value)
      transform(value)
      if (JSON.stringify(value) === before) return text
      const newline = text.includes('\r\n') ? '\r\n' : '\n'
      return (JSON.stringify(value, null, 2) + '\n').replaceAll('\n', newline)
    }
  }

  update('package.json', json((value) => { value.version = version }))
  update('src-tauri/tauri.conf.json', json((value) => { value.version = version }))
  let crateName
  update('src-tauri/Cargo.toml', (text) => {
    const section = /^\[package\][^]*?(?=^\[|$(?![^]))/m
    const match = text.match(section)
    crateName = match?.[0].match(/^name\s*=\s*"([^"]+)"/m)?.[1]
    if (!crateName) throw new Error('Cargo.toml: missing package name')
    return text.replace(section, replaceVersion(match[0], 'Cargo.toml [package]'))
  })
  update('package-lock.json', json((value) => {
    if (!value.packages?.['']) throw new Error('package-lock.json: missing root package')
    value.version = version
    value.packages[''].version = version
  }))
  update('src-tauri/Cargo.lock', (text) => {
    let matches = 0
    const result = text.replace(/^\[\[package\]\][^]*?(?=^\[\[package\]\]|$(?![^]))/gm, (section) => {
      if (section.match(/^name\s*=\s*"([^"]+)"/m)?.[1] !== crateName || /^source\s*=/m.test(section)) return section
      matches++
      return replaceVersion(section, 'Cargo.lock application package')
    })
    if (matches !== 1) throw new Error('Cargo.lock: expected exactly one local application package')
    return result
  }, true)

  const written = []
  try {
    for (const change of changes) {
      if (change.before === change.after) continue
      written.push(change)
      writeFileSync(change.path, change.after)
    }
  } catch (error) {
    for (const change of written.reverse()) writeFileSync(change.path, change.before)
    throw error
  }
  console.log(`Version set to ${version}:`)
  for (const change of changes) console.log(`  ${change.file}`)
  console.log('Review and test the changes before committing and creating a matching v-prefixed tag.')
} catch (error) {
  console.error(error.message)
  process.exitCode = 1
}
