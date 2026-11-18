import { useEffect, useRef, useState } from 'react'
import type { KitEngine } from '@kition/kit-editor'
import { Button } from '@kition/kit-editor'
import type { SlideShowOptions } from './useSlideView'
import { useSlides } from './slides'

/**
 * The slide show: one slide at a time, full screen, rendered at the screen's resolution. Click,
 * Space, Enter, or the right arrow go on; the left arrow goes back; Esc ends it. Transitions and
 * animations are not played yet.
 */
export function SlideShow({ doc, from, slides, onEnd, hint, options }: { doc: KitEngine; from: number; slides: number; onEnd: () => void; hint: string; options: SlideShowOptions }) {
  const { query, run, t } = useSlides()
  const [index, setIndex] = useState(from)
  const stage = useRef<HTMLDivElement>(null)
  const canvas = useRef<HTMLCanvasElement>(null)
  const nextCanvas = useRef<HTMLCanvasElement>(null)
  const [notes, setNotes] = useState('')
  const entered = useRef(performance.now())
  const advance = () => {
    if (options.rehearse) void run('transition.timing', { index, after: Math.round(performance.now() - entered.current) })
    entered.current = performance.now()
    setIndex((value) => value + 1)
  }

  useEffect(() => {
    if (options.reading) return
    void stage.current?.requestFullscreen?.().catch(() => undefined)
    const onFullscreen = () => {
      if (!document.fullscreenElement) onEnd()
    }
    document.addEventListener('fullscreenchange', onFullscreen)
    return () => {
      document.removeEventListener('fullscreenchange', onFullscreen)
      if (document.fullscreenElement) void document.exitFullscreen().catch(() => undefined)
    }
  }, [onEnd, options.reading])

  useEffect(() => {
    if (!options.presenter) return
    let stale = false
    void query<{ notes?: string }>('slide.inspect', { index, select: false }).then((slide) => { if (!stale) setNotes(slide?.notes ?? '') })
    if (index + 1 < slides) void doc.invoke<Uint8Array>('renderSlide', index + 1, 480).then((rgba) => {
      if (!stale && nextCanvas.current) {
        nextCanvas.current.width = 480; nextCanvas.current.height = rgba.length / 4 / 480
        nextCanvas.current.getContext('2d')?.putImageData(new ImageData(new Uint8ClampedArray(rgba), 480, rgba.length / 4 / 480), 0, 0)
      }
    })
    return () => { stale = true }
  }, [doc, index, options.presenter, query, slides])

  useEffect(() => {
    if (index >= slides) {
      onEnd()
      return
    }
    let stale = false
    const width = Math.round(Math.min(window.screen.width, 3840) * Math.min(window.devicePixelRatio || 1, 2))
    void doc.invoke<Uint8Array>('renderSlide', index, width).then((rgba) => {
      if (stale || !canvas.current) return
      const height = Math.round(rgba.length / 4 / width)
      canvas.current.width = width
      canvas.current.height = height
      canvas.current.getContext('2d')?.putImageData(new ImageData(new Uint8ClampedArray(rgba.buffer as ArrayBuffer, rgba.byteOffset, rgba.byteLength), width, height), 0, 0)
    })
    return () => {
      stale = true
    }
  }, [doc, index, onEnd, slides])

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (['ArrowRight', 'ArrowDown', 'PageDown', ' ', 'Enter'].includes(event.key)) advance()
      else if (['ArrowLeft', 'ArrowUp', 'PageUp', 'Backspace'].includes(event.key)) setIndex((value) => Math.max(0, value - 1))
      else if (event.key === 'Escape') onEnd()
      else if (event.key === 'Home') setIndex(0)
      else if (event.key === 'End') setIndex(slides - 1)
      else return
      event.preventDefault()
      event.stopPropagation()
    }
    window.addEventListener('keydown', onKey, true)
    return () => window.removeEventListener('keydown', onKey, true)
  }, [onEnd, slides, index, options.rehearse])

  return (
    <div ref={stage} className="slides-show" data-reading={options.reading} data-presenter={options.presenter} onClick={advance} data-testid="slides-show" title={hint}>
      <canvas ref={canvas} className="slides-show-slide" />
      {options.presenter ? <aside className="slides-presenter"><h2>{t('nextSlide')}</h2><canvas ref={nextCanvas} /><h2>{t('speakerNotes')}</h2><p>{notes || t('noNotes')}</p></aside> : null}
      <div className="slides-show-controls" onClick={(event) => event.stopPropagation()}><span>{t('slideOf', { slide: index + 1, slides })}</span><Button onClick={advance}>{t('nextSlide')}</Button><Button onClick={onEnd}>{t('close')}</Button></div>
    </div>
  )
}
