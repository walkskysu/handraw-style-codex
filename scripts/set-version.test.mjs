import { test } from 'node:test'
import assert from 'node:assert/strict'
import { mkdtempSync, mkdirSync, copyFileSync, readFileSync, writeFileSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { spawnSync } from 'node:child_process'

function fixture(t, { lock = true, malformed = false } = {}) {
  const root = mkdtempSync(join(tmpdir(), 'handraw-version-'))
  t.after(() => rmSync(root, { recursive: true, force: true }))
  mkdirSync(join(root, 'scripts'))
  mkdirSync(join(root, 'src-tauri'))
  copyFileSync(new URL('./set-version.mjs', import.meta.url), join(root, 'scripts/set-version.mjs'))
  const files = {
    'package.json': '{"name":"app","version":"0.1.0"}',
    'package-lock.json': '{"version":"0.1.0","packages":{"":{"version":"0.1.0"},"node_modules/example":{"version":"1.2.3"}}}',
    'src-tauri/tauri.conf.json': '{"version":"0.1.0"}',
    'src-tauri/Cargo.toml': '[package]\r\nname = "app"\r\nversion = "0.1.0"\r\n\r\n[dependencies]\r\nexample = "1.2.3"\r\n',
  }
  if (lock) files['src-tauri/Cargo.lock'] = 'version = 4\n\n[[package]]\nname = "example"\nversion = "1.2.3"\n\n[[package]]\nname = "app"\nversion = "0.1.0"\n'
  if (malformed) files['src-tauri/Cargo.lock'] = 'version = 4\n'
  for (const [file, content] of Object.entries(files)) writeFileSync(join(root, file), content)
  return {
    files,
    read: (file) => readFileSync(join(root, file), 'utf8'),
    run: (...args) => spawnSync(process.execPath, [join(root, 'scripts/set-version.mjs'), ...args], { encoding: 'utf8', cwd: tmpdir() }),
  }
}

test('synchronizes versions from any cwd, preserves dependencies and is idempotent', (t) => {
  const f = fixture(t)
  const result = f.run('0.2.4-beta.1')
  assert.equal(result.status, 0, result.stderr)
  for (const file of ['package.json', 'package-lock.json', 'src-tauri/tauri.conf.json']) {
    assert.equal(JSON.parse(f.read(file)).version, '0.2.4-beta.1')
  }
  const lock = JSON.parse(f.read('package-lock.json'))
  assert.equal(lock.packages[''].version, '0.2.4-beta.1')
  assert.equal(lock.packages['node_modules/example'].version, '1.2.3')
  assert.match(f.read('src-tauri/Cargo.toml'), /version = "0.2.4-beta.1"\r\n/)
  assert.match(f.read('src-tauri/Cargo.toml'), /example = "1.2.3"/)
  assert.match(f.read('src-tauri/Cargo.lock'), /name = "app"\nversion = "0.2.4-beta.1"/)
  assert.match(f.read('src-tauri/Cargo.lock'), /name = "example"\nversion = "1.2.3"/)
  const snapshot = Object.keys(f.files).map(f.read)
  assert.equal(f.run('0.2.4-beta.1').status, 0)
  assert.deepEqual(Object.keys(f.files).map(f.read), snapshot)
})

test('rejects invalid arguments without modifying files', (t) => {
  const f = fixture(t)
  for (const args of [[], ['v0.2.4'], ['01.2.3'], ['0.2.4-01'], ['0.2'], ['0.2.4', 'extra']]) {
    assert.equal(f.run(...args).status, 1)
    for (const [file, original] of Object.entries(f.files)) assert.equal(f.read(file), original)
  }
})

test('validates existing Cargo.lock before writing any files', (t) => {
  const f = fixture(t, { malformed: true })
  assert.equal(f.run('0.2.4').status, 1)
  for (const [file, original] of Object.entries(f.files)) assert.equal(f.read(file), original)
})

test('allows an absent Cargo.lock', (t) => {
  const f = fixture(t, { lock: false })
  const result = f.run('0.2.4')
  assert.equal(result.status, 0, result.stderr)
  assert.equal(JSON.parse(f.read('package.json')).version, '0.2.4')
})
