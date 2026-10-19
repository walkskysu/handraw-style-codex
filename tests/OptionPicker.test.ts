import { afterEach, describe, expect, it } from 'vitest'
import { enableAutoUnmount, mount } from '@vue/test-utils'
import OptionPicker from '../src/components/OptionPicker.vue'
import { generationOptions, optionCategories } from '../src/services/generationOptions'

enableAutoUnmount(afterEach)

describe('paginated image options', () => {
  it.each(optionCategories)('shows every $label image across bounded pages', async ({ key, label }) => {
    const options = generationOptions[key]
    const wrapper = mount(OptionPicker, { props: { category: key, label, options } })
    const seen: string[] = []
    expect(wrapper.get('summary').text()).toContain('未选择')
    expect(wrapper.get('nav button').attributes('disabled')).toBeDefined()
    const next = () => wrapper.findAll('nav button')[1]
    while (true) {
      const cards = wrapper.findAll('.option-card')
      expect(cards.length).toBeGreaterThan(0)
      expect(cards.length).toBeLessThanOrEqual(6)
      for (const card of cards) {
        seen.push(card.attributes('data-option')!)
        expect(card.get('img').attributes('src')).toBeTruthy()
        expect(card.get('img').attributes('alt')).toContain(card.attributes('data-option'))
      }
      if (next().attributes('disabled') !== undefined) break
      await next().trigger('click')
    }
    expect(seen).toEqual(options.map(({ id }) => id))
    const last = options.at(-1)!
    await wrapper.get(`[data-option="${last.id}"]`).trigger('click')
    expect(wrapper.emitted('update:modelValue')?.at(-1)).toEqual([last.id])
    await wrapper.setProps({ modelValue: last.id })
    expect(wrapper.get(`[data-option="${last.id}"]`).attributes('aria-pressed')).toBe('true')
    await wrapper.get('nav button').trigger('click')
    expect(wrapper.get('summary').text()).toContain(last.name)
    await wrapper.get('.clear-selection').trigger('click')
    expect(wrapper.emitted('update:modelValue')?.at(-1)).toEqual([undefined])
  })

  it('resets pagination when searching, preserves selection, and handles no results', async () => {
    const wrapper = mount(OptionPicker, { props: {
      category: 'style', label: '手绘风格', options: generationOptions.style, modelValue: 'FA-001',
    } })
    await wrapper.findAll('nav button')[1].trigger('click')
    await wrapper.get('input').setValue('fh-044')
    expect(wrapper.findAll('.option-card')).toHaveLength(1)
    expect(wrapper.get('nav').text()).toContain('第 1 / 1 页')
    expect(wrapper.get('summary').text()).toContain('FA-001')
    await wrapper.get('input').setValue('不存在的关键词')
    expect(wrapper.findAll('.option-card')).toHaveLength(0)
    expect(wrapper.text()).toContain('没有匹配')
    expect(wrapper.findAll('nav button').every((button) => button.attributes('disabled') !== undefined)).toBe(true)
    await wrapper.get('input').setValue('')
    expect(wrapper.findAll('.option-card')).toHaveLength(6)
    expect(wrapper.get('[data-option="FA-001"]').attributes('aria-pressed')).toBe('true')
  })

  it('disables controls and does not emit selections while generating', async () => {
    const wrapper = mount(OptionPicker, { props: {
      category: 'color', label: '经典单色主题色', options: generationOptions.color,
      modelValue: 'C-01', disabled: true,
    } })
    expect(wrapper.get('fieldset').attributes('disabled')).toBeDefined()
    await wrapper.get('[data-option="C-02"]').trigger('click')
    await wrapper.get('.clear-selection').trigger('click')
    expect(wrapper.emitted('update:modelValue')).toBeUndefined()
  })
})
