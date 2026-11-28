import { useEffect, useState } from 'react'
import { useReadOnly } from '@kition/sdk/editor'
import { useSlides } from './slides'
import { Thumb } from './thumbnails'

export function DesignIdeas() {
  const { query, session, run } = useSlides()
  // A viewer sees the ideas; applying one edits the slide.
  const readOnly = useReadOnly()
  const [ideas, setIdeas] = useState<Array<{ name: string; image: ImageData }>>([])
  useEffect(() => {
    const doc = session.doc
    if (!doc) return
    let stale = false
    void (async () => {
      const themes = await query<{ themes: Array<{ name: string }> }>('theme.list')
      const result = []
      for (const { name } of themes?.themes ?? []) {
        if (stale) return
        const bytes = await doc.invoke<Uint8Array>('renderTheme', name, session.state?.current ?? 0, 256)
        result.push({ name, image: new ImageData(new Uint8ClampedArray(bytes), 256, bytes.length / 4 / 256) })
        if (!stale) setIdeas([...result])
      }
    })()
    return () => { stale = true }
  }, [query, session.doc, session.state?.current, session.change.revision])
  return <>{ideas.map((idea) => <button type="button" className="slides-design-idea" key={idea.name} disabled={readOnly} onClick={() => void run('theme.set', { name: idea.name })}><Thumb image={idea.image} /><span>{idea.name}</span></button>)}</>
}
