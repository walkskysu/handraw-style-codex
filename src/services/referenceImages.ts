import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { isDesktop } from './imageGeneration'
import { message } from './gallery'

export interface Reference { id: string; name: string }
export interface ReferenceImage extends Reference { thumbnail: string }
export const MAX_REFERENCES = 12
export const referenceImages = ref<ReferenceImage[]>([])
export const referenceError = ref('')
export const referenceLoading = ref(false)
const MAX_BYTES = 20 * 1024 * 1024

export async function loadReferences() {
  if (!isDesktop() || referenceLoading.value) return
  referenceLoading.value = true; referenceError.value = ''
  try {
    const loaded = await invoke<ReferenceImage[]>('reference_list')
    referenceImages.value = [...loaded, ...referenceImages.value.filter(r => !loaded.some(item => item.id === r.id))]
  }
  catch (error) { referenceError.value = message(error) }
  finally { referenceLoading.value = false }
}

function readDataUrl(file: Blob): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader()
    reader.onload = () => resolve(String(reader.result))
    reader.onerror = () => reject(new Error('无法读取参考图像，请重试。'))
    reader.readAsDataURL(file)
  })
}

// WebView decodes JPEG locally; Rust revalidates the resulting PNG before storing it.
export async function referenceData(file: File): Promise<string> {
  if (file.size > MAX_BYTES) throw new Error('参考图像不能超过 20 MB。')
  if (!['image/png', 'image/jpeg', 'image/webp'].includes(file.type)) throw new Error('参考图像仅支持 PNG、JPEG 和 WebP。')
  const data = await readDataUrl(file)
  if (file.type !== 'image/jpeg') return data
  return new Promise((resolve, reject) => {
    const image = new Image()
    image.onerror = () => reject(new Error('无法读取 JPEG 参考图像。'))
    image.onload = () => {
      if (!image.naturalWidth || !image.naturalHeight || image.naturalWidth > 8192 || image.naturalHeight > 8192 || image.naturalWidth * image.naturalHeight > 16 * 1024 * 1024) {
        reject(new Error('参考图像分辨率过大，请缩小图片。')); return
      }
      try {
        const canvas = document.createElement('canvas')
        canvas.width = image.naturalWidth; canvas.height = image.naturalHeight
        const context = canvas.getContext('2d')
        if (!context) throw new Error('无法转换参考图像。')
        context.drawImage(image, 0, 0)
        canvas.toBlob(blob => {
          if (!blob || blob.size > MAX_BYTES) { reject(new Error('转换后的参考图像超过大小限制，请缩小图片。')); return }
          readDataUrl(blob).then(resolve, reject)
        }, 'image/png')
      } catch (error) { reject(error) }
    }
    image.src = data
  })
}

export async function uploadReferences(files: File[]): Promise<void> {
  if (files.length > MAX_REFERENCES) throw new Error(`每次最多上传 ${MAX_REFERENCES} 张参考图像。`)
  for (const file of files) {
    const added = await invoke<ReferenceImage>('reference_import', { name: file.name, dataUrl: await referenceData(file) })
    // Preserve successful uploads if a later file fails.
    referenceImages.value = [added, ...referenceImages.value]
  }
}
