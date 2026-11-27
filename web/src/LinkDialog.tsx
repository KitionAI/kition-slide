/**
 * Insert › Link and Cmd+K (plan 0A, walkthrough G6): a link to a web address or to a slide of this
 * file, on the selected shapes or the selected text. A link the selected shape already has fills
 * the dialog in. An address without a scheme is taken as a web address (`https://`).
 */
import { useEffect, useState } from 'react'
import { Button, Dialog } from '@kition/sdk/editor'
import { useSlides } from './slides'
import { returnKeyboardLater } from './focus'

type Click = { action?: { kind?: string; url?: string; slide?: number } }

/** The address as the link keeps it: a scheme added to a bare host name. */
export function linkAddress(text: string) {
  const address = text.trim()
  if (!address) return ''
  if (/^[a-z][a-z0-9+.-]*:/i.test(address)) return address
  if (/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(address)) return `mailto:${address}`
  return `https://${address}`
}

export function LinkDialog({ onClose }: { onClose: () => void }) {
  const { t, run, query, facts, session } = useSlides()
  const [target, setTarget] = useState<'url' | 'slide'>('url')
  const [url, setUrl] = useState('')
  const [slide, setSlide] = useState(String(Math.min((session.state?.current ?? 0) + 1, Math.max(0, facts.slides.length - 1))))
  const shape = facts.selection?.shapes?.length === 1 ? facts.selection.shapes[0] : null

  // The selected shape's own link, when it has one.
  useEffect(() => {
    if (shape == null) return
    let stale = false
    void query<{ shape?: { click?: Click } }>('shape.inspect', { id: shape }).then((found) => {
      const action = found?.shape?.click?.action
      if (stale || !action) return
      if (action.kind === 'url' && action.url) {
        setTarget('url')
        setUrl(action.url)
      } else if (action.kind === 'slide') {
        const index = facts.slides.findIndex((item) => item.id === action.slide)
        if (index >= 0) {
          setTarget('slide')
          setSlide(String(index))
        }
      }
    })
    return () => {
      stale = true
    }
  }, [shape, query, facts.slides])

  const close = () => {
    onClose()
    returnKeyboardLater()
  }
  const address = linkAddress(url)
  const ready = target === 'url' ? address !== '' : facts.slides.length > 0
  const apply = () => {
    if (!ready) return
    close()
    void run('link.insert', target === 'url' ? { url: address } : { slide: Number(slide) })
  }

  return (
    <Dialog
      title={t('link.title')}
      onClose={close}
      testId="slides-link-dialog"
      actions={
        <>
          <Button variant="ghost" onClick={close}>
            {t('cancel')}
          </Button>
          <Button variant="primary" disabled={!ready} onClick={apply} data-testid="slides-link-apply">
            {t('link.apply')}
          </Button>
        </>
      }
    >
      <form
        className="slides-link-form"
        onSubmit={(event) => {
          event.preventDefault()
          apply()
        }}
      >
        <label className="slides-field-row">
          <input type="radio" name="slides-link-target" checked={target === 'url'} onChange={() => setTarget('url')} data-testid="slides-link-to-url" />
          {t('link.url')}
        </label>
        <input
          className="kui-field"
          type="text"
          inputMode="url"
          autoFocus
          placeholder="https://"
          value={url}
          onFocus={() => setTarget('url')}
          onChange={(event) => setUrl(event.target.value)}
          aria-label={t('link.url')}
          data-testid="slides-link-url"
        />
        <label className="slides-field-row">
          <input type="radio" name="slides-link-target" checked={target === 'slide'} onChange={() => setTarget('slide')} data-testid="slides-link-to-slide" />
          {t('link.slide')}
        </label>
        <select className="kui-field" value={slide} onFocus={() => setTarget('slide')} onChange={(event) => { setTarget('slide'); setSlide(event.target.value) }} aria-label={t('link.slide')} data-testid="slides-link-slide">
          {facts.slides.map((item, index) => (
            <option key={item.id} value={index}>
              {t('link.slideItem', { slide: index + 1, title: item.title || t('link.untitled') })}
            </option>
          ))}
        </select>
      </form>
    </Dialog>
  )
}
