import { afterEach, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import { clearMocks, mockIPC } from '@tauri-apps/api/mocks'
import JobPreview from '../src/components/JobPreview.vue'
import type { Item, JobSnapshot } from '../src/services/creation'

afterEach(clearMocks)

it('follows a completed retry at an earlier position and preserves manual selection across polling', async () => {
  const ipc = vi.fn().mockResolvedValue('data:image/png;base64,image')
  mockIPC(ipc)
  const items = [0, 1, 2].map(position => ({
    id: `item-${position}`, position, title: `图片 ${position + 1}`,
    attempt_id: `attempt-${position}`, attempt_no: 1,
    generation_status: position === 1 ? 'failed' : 'generated',
    save_status: position === 1 ? 'none' : 'saved', artwork_id: `item-${position}`,
  } as Item))
  const initial = { id: 'job', status: 'partial', revision: 10, items, expected_count: 3, generated_count: 2, saved_count: 2, plan: null } as JobSnapshot
  const wrapper = mount(JobPreview, { props: { job: initial, disabled: false } })
  await flushPromises()
  expect(wrapper.find('figcaption').text()).toContain('第 3 / 3 张')
  const retry = { ...initial, revision: 12, items: items.map(i => i.position === 1 ? { ...i, generation_status: 'generated', save_status: 'saved', attempt_id: 'retry-2', attempt_no: 2 } : i) }
  await wrapper.setProps({ job: retry })
  await flushPromises()
  expect(wrapper.find('figcaption').text()).toContain('第 2 / 3 张')
  const callsBeforePolling = ipc.mock.calls.length
  await wrapper.setProps({ job: structuredClone(retry) })
  expect(wrapper.find('figcaption').text()).toContain('第 2 / 3 张')
  expect(ipc).toHaveBeenCalledTimes(callsBeforePolling)
  await wrapper.findAll('nav button')[0]!.trigger('click')
  await flushPromises()
  await wrapper.setProps({ job: structuredClone(retry) })
  expect(wrapper.find('figcaption').text()).toContain('第 1 / 3 张')
  await wrapper.find('input[type="checkbox"]').setValue(true)
  await flushPromises()
  expect(wrapper.find('figcaption').text()).toContain('第 2 / 3 张')
  wrapper.unmount()
})
