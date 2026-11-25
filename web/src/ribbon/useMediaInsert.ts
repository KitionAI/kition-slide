import { base64, toast } from '@kition/sdk/editor'
import { useSlides } from '../slides'

/** Media bytes are picked and read only through the host, as with pictures. */
export function useMediaInsert(kind: 'audio' | 'video') {
  const { host, run, t, enabled } = useSlides()
  const can = host.init.capabilities.includes('pick-file') && enabled(`${kind}.insert`)
  const insert = async () => {
    try {
      const path = await host.pick(kind, kind === 'audio' ? ['mp3', 'wav', 'm4a', 'ogg', 'flac'] : ['mp4', 'webm', 'mov'])
      if (!path) return
      const { bytes } = await host.files.read(path)
      await run(`${kind}.insert`, { data: base64(bytes), name: path.split('/').pop() })
    } catch (error) {
      toast('error', t('commandFailed'), error instanceof Error ? error.message : String(error))
    }
  }
  return { can, insert }
}
