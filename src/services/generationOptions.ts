import catalog from '../data/generationCatalog.json'

export type OptionCategory = 'style' | 'layout' | 'color'
export type GenerationSelections = Partial<Record<OptionCategory, string>>
export interface GenerationOption {
  id: string
  name: string
  group: string
  image: string
  prompt: string
}

const previews = import.meta.glob<string>(
  [
    '../../handraw-style/images/individual/*/F?-???.webp',
    '../../handraw-style/images/layouts/*/??-???.webp',
    '../../handraw-style/images/colors/C-??.webp',
  ],
  { eager: true, query: '?url', import: 'default' },
)

export const optionCategories: { key: OptionCategory; label: string }[] = [
  { key: 'style', label: '手绘风格' },
  { key: 'layout', label: '排版图型' },
  { key: 'color', label: '经典单色主题色' },
]

export const generationOptions = Object.fromEntries(optionCategories.map(({ key }) => [
  key, catalog[key].map((option) => {
    const image = previews[`../../handraw-style/${option.image}`]
    if (!image) throw new Error(`缺少预览图：${option.id}，请初始化 handraw-style 子模块。`)
    return { ...option, image }
  }),
])) as Record<OptionCategory, GenerationOption[]>
