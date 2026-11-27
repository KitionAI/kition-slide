import { useEffect, useRef, type RefObject } from 'react'
import type { KitCanvasHandle, RasterRect, RasterView } from '@kition/sdk/editor'
import type { Slides } from './slides'
import { notchZoom, pinchZoom, wheelNotches, zoomAround } from './zoom'

/** How far past the slide's edge an enlarged slide scrolls, in screen pixels. */
export const CANVAS_MARGIN = 28
const PAGE_DELAY_MS = 350
const PAGE_THRESHOLD = 18
/** How far a trackpad's fine scroll travels before it turns the page, in CSS pixels. */
const SWIPE_DISTANCE = 60
/** A pause this long ends a trackpad swipe, its momentum included. */
const SWIPE_END_MS = 220

/**
 * The native slide canvas's wheel policy: page when fitted, bounded pan when enlarged. With Cmd or
 * Ctrl the wheel zooms about the pointer: a mouse notch to the next multiple of 10% at least 10
 * points away, a pinch continuously, within 10%–400% (`zoom.ts`).
 */
export function useSlideWheel(
  region: RefObject<HTMLDivElement | null>,
  canvas: RefObject<KitCanvasHandle | null>,
  viewMemory: RefObject<RasterView | null>,
  slides: Slides,
) {
  const latest = useRef(slides)
  latest.current = slides

  useEffect(() => {
    const element = region.current
    if (!element) return
    const pointers = new Set<number>()
    let lastPage = -Infinity
    let paging = false
    // A trackpad swipe: its fine deltas add up until they turn one page; the swipe (with its
    // momentum) turns no other page until it pauses.
    let swipe = { distance: 0, turned: false, last: -Infinity }
    const down = (event: PointerEvent) => pointers.add(event.pointerId)
    const up = (event: PointerEvent) => pointers.delete(event.pointerId)
    const reset = () => pointers.clear()
    const wheel = (event: WheelEvent) => {
      const surface = event.target
      if (!(surface instanceof HTMLCanvasElement) || !surface.classList.contains('kui-canvas-surface')) return
      const view = viewMemory.current
      const { session, run } = latest.current
      const area = session.state?.area
      if (!view || !area || !canvas.current) return

      // Capture before KitCanvas's native wheel listener can freely pan the slide away (or zoom
      // 2.72 times a notch).
      event.preventDefault()
      event.stopPropagation()
      if (event.ctrlKey || event.metaKey) {
        const shown = canvas.current.view()?.zoom ?? view.zoom
        const notches = wheelNotches(event)
        let zoom = shown
        if (notches) for (let index = 0; index < Math.min(notches, 5); index += 1) zoom = notchZoom(zoom, event.deltaY < 0 ? 1 : -1)
        else zoom = pinchZoom(shown, event.deltaY)
        zoomAround(canvas.current, zoom, surface, event.clientX, event.clientY)
        return
      }
      const unit = event.deltaMode === WheelEvent.DOM_DELTA_LINE ? 16
        : event.deltaMode === WheelEvent.DOM_DELTA_PAGE ? surface.clientHeight : 1
      const dx = event.deltaX * unit
      const dy = event.deltaY * unit
      const overflowX = (area.x1 - area.x0) * view.zoom + CANVAS_MARGIN * 2 - surface.clientWidth
      const overflowY = (area.y1 - area.y0) * view.zoom + CANVAS_MARGIN * 2 - surface.clientHeight
      if (overflowX > 0.5 || overflowY > 0.5) {
        const pan = (value: number, delta: number, start: number, end: number, extent: number) => {
          if ((end - start) * view.zoom + CANVAS_MARGIN * 2 <= extent) return (start + end - extent / view.zoom) / 2
          const min = start - CANVAS_MARGIN / view.zoom
          const max = end - (extent - CANVAS_MARGIN) / view.zoom
          return Math.max(min, Math.min(max, value + delta / view.zoom))
        }
        canvas.current.panTo(
          pan(view.x, dx, area.x0, area.x1, surface.clientWidth),
          pan(view.y, dy, area.y0, area.y1, surface.clientHeight),
        )
      } else if (session.state?.target.kind !== 'slides') {
        return
      } else if (!pointers.size && wheelNotches(event) === 0) {
        // A trackpad: the swipe's distance, not each event's, turns the page; once.
        const now = performance.now()
        if (now - swipe.last > SWIPE_END_MS) swipe = { distance: 0, turned: false, last: now }
        swipe.last = now
        if (Math.abs(dy) < Math.abs(dx) || swipe.turned) return
        swipe.distance += dy
        if (Math.abs(swipe.distance) < SWIPE_DISTANCE || paging) return
        swipe.turned = true
        lastPage = now
        paging = true
        void run(swipe.distance > 0 ? 'slide.next' : 'slide.previous').finally(() => { paging = false })
      } else if (!pointers.size && !paging && Math.abs(dy) > PAGE_THRESHOLD && Math.abs(dy) >= Math.abs(dx)) {
        const now = performance.now()
        if (now - lastPage < PAGE_DELAY_MS) return
        lastPage = now
        paging = true
        void run(dy > 0 ? 'slide.next' : 'slide.previous').finally(() => { paging = false })
      }
    }
    element.addEventListener('wheel', wheel, { capture: true, passive: false })
    element.addEventListener('pointerdown', down, true)
    window.addEventListener('pointerup', up, true)
    window.addEventListener('pointercancel', up, true)
    window.addEventListener('blur', reset)
    return () => {
      element.removeEventListener('wheel', wheel, true)
      element.removeEventListener('pointerdown', down, true)
      window.removeEventListener('pointerup', up, true)
      window.removeEventListener('pointercancel', up, true)
      window.removeEventListener('blur', reset)
    }
  }, [canvas, region, viewMemory])
}

/**
 * The slide and `CANVAS_MARGIN` screen pixels of canvas around it, in document pixels at `zoom`:
 * where an enlarged slide's scrolling stops (the wheel's, a drag's and the scrollbars'), and what
 * the scrollbars measure. A view that holds it all keeps the slide centred.
 */
export function slideScrollArea(area: RasterRect | null | undefined, zoom: number): RasterRect | null {
  if (!area || !(zoom > 0)) return null
  const margin = CANVAS_MARGIN / zoom
  return { x0: area.x0 - margin, y0: area.y0 - margin, x1: area.x1 + margin, y1: area.y1 + margin }
}
