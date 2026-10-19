import { readFileSync, statSync } from 'node:fs'
import { resolve } from 'node:path'
import { describe, expect, it } from 'vitest'
import catalog from '../src/data/generationCatalog.json'

const upstream = resolve('handraw-style')
const references = resolve(upstream, 'skills/handdraw-style-prompter/references')
const read = (path: string) => readFileSync(path, 'utf8')

describe('pinned handraw-style catalog', () => {
  it('includes every upstream style, layout and classic color with local preview assets', () => {
    const styles = JSON.parse(read(resolve(references, 'styles.json')))
    const layouts = JSON.parse(read(resolve(references, 'layouts.json')))
    const colors = JSON.parse(read(resolve(references, 'colors.json')))
    expect(catalog.style.map(({ id }) => id)).toEqual(styles.map((s: { number: string }) => s.number))
    expect(catalog.layout.map(({ id }) => id)).toEqual(layouts.map((s: { id: string }) => s.id))
    expect(catalog.color.map(({ id }) => id)).toEqual(colors.map((s: { id: string }) => s.id))
    for (const entries of Object.values(catalog)) {
      expect(new Set(entries.map(({ id }) => id)).size).toBe(entries.length)
      for (const entry of entries) {
        expect(statSync(resolve(upstream, entry.image)).size).toBeGreaterThan(0)
        expect(entry.prompt.trim()).not.toBe('')
      }
    }
    for (const [index, style] of styles.entries()) {
      expect(catalog.style[index].prompt).toBe(`${style.generation_name}。${style.traits}`)
    }
    for (const [index, layout] of layouts.entries()) {
      const specification = read(resolve(references, layout.prompt_file))
        .split('<!-- en -->')[0].replace('<!-- zh -->', '').trim().replace(/\r\n/g, '\n')
      expect(catalog.layout[index].prompt).toBe(specification)
    }
    for (const [index, color] of colors.entries()) {
      expect(catalog.color[index].prompt).toBe(color.prompt_zh)
    }
  })
})
