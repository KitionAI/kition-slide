import { Thumb } from './SlidesPanel'
import { useEffect, useState } from 'react'
import { useSlides } from './slides'
import { NotesPane } from './NotesPane'

/** A page for the current slide and its editable speaker notes. */
export function NotesPage() {
  const { session, t } = useSlides()
  const [image, setImage] = useState<ImageData | null>(null)
  useEffect(() => {
    const doc = session.doc
    if (!doc) return
    let stale = false
    void doc.invoke<Uint8Array>('renderSlide', session.state?.current ?? 0, 640).then((rgba) => {
      if (!stale) setImage(new ImageData(new Uint8ClampedArray(rgba), 640, rgba.length / 4 / 640))
    })
    return () => { stale = true }
  }, [session.doc, session.state?.current, session.change.revision])
  return <div className="slides-notes-page-stage" data-testid="slides-notes-page"><div className="slides-notes-page"><Thumb image={image} /><h2>{t('speakerNotes')}</h2><NotesPane /></div></div>
}
