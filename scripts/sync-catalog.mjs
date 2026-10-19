import { readFileSync, existsSync, mkdirSync, writeFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { resolve, dirname } from 'node:path'

const root = fileURLToPath(new URL('../', import.meta.url))
const upstream = resolve(root, 'handraw-style')
const references = resolve(upstream, 'skills/handdraw-style-prompter/references')
const read = (path) => readFileSync(path, 'utf8')
const json = (name) => JSON.parse(read(resolve(references, `${name}.json`)))

// The checked-in snapshot is shared by Vue and Rust; images remain in the submodule.
export function buildCatalog() {
  const groups = {
    'social-card': '社媒卡', infographic: '信息图',
    'comic-storyboard': '漫画分镜', 'ip-character': 'IP设计', ecommerce: '电商',
  }
  const catalog = {
    style: json('styles').map((style) => ({
      id: style.number, name: style.generation_name, group: style.group,
      image: `images/individual/${style.number.split('-')[0]}/${style.number}.webp`,
      prompt: `${style.generation_name}。${style.traits}`,
    })),
    layout: json('layouts').map((layout) => ({
      id: layout.id, name: layout.name, group: groups[layout.category] ?? layout.category,
      image: layout.image.replace('../../../', ''),
      prompt: read(resolve(references, layout.prompt_file))
        .split('<!-- en -->')[0].replace('<!-- zh -->', '').trim().replace(/\r\n/g, '\n'),
    })),
    color: json('colors').map((color) => ({
      id: color.id, name: color.name_zh, group: color.category_zh,
      image: color.image.replace('../../../', ''), prompt: color.prompt_zh,
    })),
  }
  for (const options of Object.values(catalog)) {
    if (new Set(options.map(({ id }) => id)).size !== options.length) throw new Error('Duplicate option IDs')
    for (const option of options) {
      if (!option.prompt || !existsSync(resolve(upstream, option.image))) {
        throw new Error(`Missing prompt or preview for ${option.id}; run git submodule update --init`)
      }
    }
  }
  return catalog
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const target = resolve(root, 'src/data/generationCatalog.json')
  mkdirSync(dirname(target), { recursive: true })
  writeFileSync(target, JSON.stringify(buildCatalog(), null, 2) + '\n')
}
