import { afterEach, describe, expect, it, vi } from 'vitest'
import { enableAutoUnmount, mount } from '@vue/test-utils'
import OptionPicker from '../src/components/OptionPicker.vue'
import { generationOptions, optionCategories } from '../src/services/generationOptions'

enableAutoUnmount(afterEach)
afterEach(() => { Reflect.deleteProperty(HTMLElement.prototype, 'scrollIntoView') })

describe('scrollable image options', () => {
  it('resets batch size for searches and categories without dropping a later selection', async () => {
    const options = generationOptions.style
    const wrapper = mount(OptionPicker, { props: { category: 'style', label: '风格', options, modelValue: options[70]!.id } })
    expect(wrapper.findAll('.option-card')).toHaveLength(40)
    await wrapper.get('.load-more').trigger('click')
    expect(wrapper.findAll('.option-card')).toHaveLength(80)
    expect(wrapper.get(`[data-option="${options[70]!.id}"]`).attributes('aria-pressed')).toBe('true')
    await wrapper.get('input').setValue(options.at(-1)!.id)
    expect(wrapper.findAll('.option-card')).toHaveLength(1)
    expect(wrapper.get('.load-more').attributes('disabled')).toBeDefined()
    await wrapper.get('input').setValue('')
    expect(wrapper.findAll('.option-card')).toHaveLength(40)
    await wrapper.vm.reveal(options[70]!.id)
    expect(wrapper.findAll('.option-card')).toHaveLength(80)
    expect(wrapper.get(`[data-option="${options[70]!.id}"]`).attributes('aria-pressed')).toBe('true')
    await wrapper.setProps({ category: 'layout', options: generationOptions.layout })
    expect(wrapper.findAll('.option-card')).toHaveLength(40)
    expect(wrapper.props('modelValue')).toBe(options[70]!.id)
  })
  it('returns scrolling to the start after searching, switching categories and revealing without a selection', async () => {
    const wrapper = mount(OptionPicker, { props: {
      category: 'style', label: '风格', options: generationOptions.style,
    } })
    const area = wrapper.get<HTMLElement>('[role="tabpanel"]').element
    expect(area.tabIndex).toBe(0)
    area.scrollTop = 300
    await wrapper.get('input').setValue('FA-001')
    expect(area.scrollTop).toBe(0)
    area.scrollTop = 300
    await wrapper.setProps({ category: 'color', label: '主题色', options: generationOptions.color })
    expect(area.scrollTop).toBe(0)
    await wrapper.get('input').setValue('')
    area.scrollTop = 300
    await wrapper.vm.reveal()
    expect(area.scrollTop).toBe(0)
  })

  it.each(optionCategories)('loads $label images in batches and reveals the last selection', async ({ key, label }) => {
    const options = generationOptions[key]
    const wrapper = mount(OptionPicker, { props: { category: key, label, options } })
    expect(wrapper.findAll('.option-card')).toHaveLength(Math.min(40, options.length))
    while (!wrapper.get<HTMLButtonElement>('.load-more').element.disabled) await wrapper.get('.load-more').trigger('click')
    const cards = wrapper.findAll('.option-card')
    expect(cards.map((card) => card.attributes('data-option'))).toEqual(options.map(({ id }) => id))
    expect(wrapper.find('nav').exists()).toBe(false)
    for (const card of cards) {
      expect(card.get('img').attributes('src')).toBeTruthy()
      expect(card.get('img').attributes('alt')).toContain(card.attributes('data-option'))
      expect(card.get('img').attributes('loading')).toBe('lazy')
    }
    const last = options.at(-1)!
    await wrapper.get(`[data-option="${last.id}"]`).trigger('click')
    expect(wrapper.emitted('update:modelValue')?.at(-1)).toEqual([last.id])
    await wrapper.setProps({ modelValue: last.id })
    expect(wrapper.get(`[data-option="${last.id}"]`).attributes('aria-pressed')).toBe('true')
    // Revealing a later batch can remount the selected card after clearing the filter.
    await wrapper.get('input').setValue(last.id)
    expect(wrapper.props('modelValue')).toBe(last.id)
    const scrollIntoView = vi.fn()
    Object.defineProperty(HTMLElement.prototype, 'scrollIntoView', { configurable: true, value: scrollIntoView })
    await wrapper.vm.reveal(last.id)
    expect(wrapper.get<HTMLInputElement>('input').element.value).toBe('')
    expect(wrapper.findAll('.option-card')).toHaveLength(options.length)
    expect(scrollIntoView).toHaveBeenCalledWith({ block: 'nearest' })
    expect(scrollIntoView.mock.contexts[0]).toBe(wrapper.get(`[data-option="${last.id}"]`).element)
    expect(wrapper.get(`[data-option="${last.id}"]`).attributes('aria-pressed')).toBe('true')
  })

  it('filters all options, preserves selection, and handles no results', async () => {
    const wrapper = mount(OptionPicker, { props: {
      category: 'style', label: '手绘风格', options: generationOptions.style, modelValue: 'FA-001',
    } })
    await wrapper.get('input').setValue('fh-044')
    expect(wrapper.findAll('.option-card')).toHaveLength(1)
    expect(wrapper.get('.option-card').attributes('data-option')).toBe('FH-044')
    expect(wrapper.props('modelValue')).toBe('FA-001')
    await wrapper.get('input').setValue('不存在的关键词')
    expect(wrapper.findAll('.option-card')).toHaveLength(0)
    expect(wrapper.text()).toContain('没有匹配')
    await wrapper.get('input').setValue('')
    expect(wrapper.findAll('.option-card')).toHaveLength(40)
    expect(wrapper.get('[data-option="FA-001"]').attributes('aria-pressed')).toBe('true')
  })

  it('disables controls and does not emit selections while generating', async () => {
    const wrapper = mount(OptionPicker, { props: {
      category: 'color', label: '经典单色主题色', options: generationOptions.color,
      modelValue: 'C-01', disabled: true,
    } })
    expect(wrapper.get('fieldset').attributes('disabled')).toBeDefined()
    await wrapper.get('[data-option="C-02"]').trigger('click')
    expect(wrapper.emitted('update:modelValue')).toBeUndefined()
  })

  it('shows the full category after switching and searches names and groups as well as IDs', async () => {
    const wrapper = mount(OptionPicker, { props: {
      category: 'style', label: '风格', options: generationOptions.style,
    } })
    await wrapper.setProps({ category: 'color', label: '主题色', options: generationOptions.color })
    expect(wrapper.findAll('.option-card')).toHaveLength(generationOptions.color.length)
    for (const query of [generationOptions.color[0]!.name, generationOptions.color[0]!.group]) {
      await wrapper.get('input').setValue(query)
      expect(wrapper.find('[data-option="C-01"]').exists()).toBe(true)
      const expected = generationOptions.color.filter((option) =>
        `${option.id} ${option.name} ${option.group}`.toLocaleLowerCase().includes(query.toLocaleLowerCase()))
      expect(wrapper.findAll('.option-card').map((card) => card.attributes('data-option')))
        .toEqual(expected.map(({ id }) => id))
    }
  })
})
