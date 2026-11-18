import { useEffect, useRef, useState } from 'react'
import { useSlides } from './slides'

export function MediaPlayer() {
  const { query, session, view, t } = useSlides()
  const [media, setMedia] = useState<{ url: string; video: boolean } | null>(null)
  const player = useRef<HTMLVideoElement | HTMLAudioElement | null>(null)
  useEffect(() => {
    let stale = false
    let url: string | undefined
    const doc = session.doc
    if (doc) void (async () => {
      const info = await query<{ id: number; video: boolean; contentType: string }>('media.info')
      if (!info) return
      const bytes = await doc.invoke<Uint8Array>('mediaData', info.id)
      if (stale) return
      url = URL.createObjectURL(new Blob([new Uint8Array(bytes)], { type: info.contentType }))
      setMedia({ url, video: info.video })
    })()
    return () => { stale = true; if (url) URL.revokeObjectURL(url) }
  }, [query, session.doc, session.state?.current, session.change.revision])
  useEffect(() => {
    if (!player.current) return
    if (view.mediaAction.kind === 'play') void player.current.play().catch(() => undefined)
    else { player.current.pause(); if (view.mediaAction.kind === 'stop') player.current.currentTime = 0 }
  }, [view.mediaAction, media])
  if (!media) return <p>{t('empty')}</p>
  return media.video ? <video ref={(element) => { player.current = element }} src={media.url} controls style={{ width: '100%' }} /> : <audio ref={(element) => { player.current = element }} src={media.url} controls style={{ width: '100%' }} />
}
