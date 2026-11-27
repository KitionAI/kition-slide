/**
 * The status bar's zoom (plan 0A, walkthrough A2, A6, A7; WPS W-L1, W-L2): − and + go to the next
 * multiple of 10%, the slider covers 10%–400%, the percentage opens a field to type one in, and
 * Fit fits the slide with the margin it opens with.
 */
import { useRef, useState, type RefObject } from 'react'
import type { KitCanvasHandle } from '@kition/sdk/editor'
import { useSlides } from './slides'
import { MAX_ZOOM, MIN_ZOOM, clampZoom, fitSlide, stepZoom } from './zoom'
import { focusEditor } from './focus'

const toSlider = (zoom: number) => Math.log(zoom / MIN_ZOOM) / Math.log(MAX_ZOOM / MIN_ZOOM)
const fromSlider = (value: number) => MIN_ZOOM * Math.pow(MAX_ZOOM / MIN_ZOOM, value)

export function ZoomBox({ canvas, zoom }: { canvas: RefObject<KitCanvasHandle | null>; zoom: number }) {
  const { t, session } = useSlides()
  const [typing, setTyping] = useState<string | null>(null)
  /** The field closed by its own Enter or Escape: the blur that follows changes nothing. */
  const closed = useRef(false)
  const percent = Math.round(zoom * 100)
  const to = (value: number) => canvas.current?.zoomTo(clampZoom(value))
  const typed = (text: string) => {
    const value = Number(text.replace('%', '').trim())
    if (Number.isFinite(value) && value > 0) to(value / 100)
  }
  const done = (keep: boolean, text: string) => {
    closed.current = true
    if (keep) typed(text)
    setTyping(null)
    focusEditor()
  }
  return (
    <div className="kui-zoom" data-testid="slides-zoom">
      <button type="button" className="kui-icon-button" aria-label={t('zoomOut')} title={t('zoomOut')} onClick={() => to(stepZoom(zoom, -1))} data-testid="slides-zoom-out">
        −
      </button>
      <input type="range" min={0} max={1} step={0.001} value={toSlider(clampZoom(zoom))} onChange={(event) => to(fromSlider(Number(event.target.value)))} aria-label={t('zoom')} />
      <button type="button" className="kui-icon-button" aria-label={t('zoomIn')} title={t('zoomIn')} onClick={() => to(stepZoom(zoom, 1))} data-testid="slides-zoom-in">
        +
      </button>
      {typing === null ? (
        <button type="button" className="kui-zoom-value slides-zoom-value" title={t('zoomType')} onClick={() => {
          closed.current = false
          setTyping(String(percent))
        }} data-testid="kit-zoom">
          {percent}%
        </button>
      ) : (
        <input
          className="kui-zoom-value slides-zoom-field"
          aria-label={t('zoomType')}
          value={typing}
          autoFocus
          onFocus={(event) => event.currentTarget.select()}
          onChange={(event) => setTyping(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === 'Enter') done(true, event.currentTarget.value)
            else if (event.key === 'Escape') done(false, '')
            else return
            event.preventDefault()
            event.stopPropagation()
          }}
          onBlur={(event) => {
            if (closed.current) return
            closed.current = true
            typed(event.currentTarget.value)
            setTyping(null)
          }}
          data-testid="slides-zoom-field"
        />
      )}
      <button type="button" className="kui-button" data-variant="ghost" onClick={() => canvas.current && fitSlide(canvas.current, session.state?.area)} data-testid="slides-zoom-fit">
        {t('fit')}
      </button>
    </div>
  )
}
