/**
 * The canvas's zoom steps (plan 0A, walkthrough A2, A3, A5, A7; WPS W-L1, W-L2, W-L4). Buttons
 * and Cmd+− / Cmd+= go 10% at a time to the next multiple of 10 (132% → 140% or 130%); a mouse
 * wheel notch with Cmd or Ctrl goes to the first multiple of 10 at least 10 points away (132% →
 * 150%), about the pointer; a trackpad pinch stays continuous. Every way of zooming stays within
 * 10%–400%, and every Fit (the slide as it opens, the status bar's, View › Fit, Cmd+0) fits the
 * slide with the same margin.
 */
import type { RefObject } from 'react'
import type { KitCanvasHandle } from '@kition/sdk/editor'
import type { SlideArea } from './slides'

export const MIN_ZOOM = 0.1
export const MAX_ZOOM = 4
/** A pinch's zoom per pixel of wheel delta (the shared canvas's continuous zoom). */
const PINCH_SPEED = 0.01
const EPSILON = 1e-6

export const clampZoom = (zoom: number) => Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, zoom))

/** The next multiple of 10% past `zoom`, up (`1`) or down (`-1`). */
export function stepZoom(zoom: number, direction: 1 | -1) {
  const tens = (zoom * 100) / 10
  const next = direction > 0 ? Math.floor(tens + EPSILON) + 1 : Math.ceil(tens - EPSILON) - 1
  return clampZoom((next * 10) / 100)
}

/** One mouse wheel notch from `zoom`: the first multiple of 10% at least 10 points away. */
export function notchZoom(zoom: number, direction: 1 | -1) {
  const percent = zoom * 100
  const next = direction > 0 ? Math.ceil((percent + 10) / 10 - EPSILON) : Math.floor((percent - 10) / 10 + EPSILON)
  return clampZoom((next * 10) / 100)
}

/** A pinch (or a wheel too fine to be notches) zooms continuously, by its delta. */
export const pinchZoom = (zoom: number, deltaY: number) => clampZoom(zoom * Math.exp(-deltaY * PINCH_SPEED))

/**
 * How many mouse wheel notches a wheel event is, or 0 for a trackpad's scroll or pinch: a wheel
 * counted in lines, or in whole multiples of about 100 pixels (a browser's notch).
 */
export function wheelNotches(event: Pick<WheelEvent, 'deltaMode' | 'deltaY'>) {
  const delta = Math.abs(event.deltaY)
  if (event.deltaMode === 1) return delta > 0 ? Math.max(1, Math.round(delta / 3)) : 0
  if (event.deltaMode === 2) return delta > 0 ? 1 : 0
  if (delta < 50 || !Number.isInteger(event.deltaY)) return 0
  const notches = Math.round(delta / 100)
  return notches >= 1 && Math.abs(delta - notches * 100) <= 10 * notches ? notches : 0
}

/** Zooms to `zoom`, keeping the document point under (`clientX`, `clientY`) still; the canvas's centre without one. */
export function zoomAround(canvas: KitCanvasHandle, zoom: number, surface?: Element | null, clientX?: number, clientY?: number) {
  const next = clampZoom(zoom)
  const view = canvas.view()
  if (!view || !surface || clientX === undefined || clientY === undefined) {
    canvas.zoomTo(next)
    return
  }
  const box = surface.getBoundingClientRect()
  const sx = clientX - box.left
  const sy = clientY - box.top
  const x = view.x + sx / view.zoom
  const y = view.y + sy / view.zoom
  canvas.zoomTo(next)
  canvas.panTo(x - sx / next, y - sy / next)
}

/** Fits the slide in view (the margin the canvas fits it with as it opens). */
export function fitSlide(canvas: KitCanvasHandle, area: SlideArea | null | undefined) {
  if (area) canvas.show(area)
  else canvas.fit()
}

/**
 * The canvas as kit-editor's frame sees it (its Cmd+0, Cmd+= and Cmd+−): the same canvas, with Fit
 * fitting the slide and each zoom step going to the next multiple of 10%.
 */
export function steppedCanvas(canvas: RefObject<KitCanvasHandle | null>, area: () => SlideArea | null | undefined): RefObject<KitCanvasHandle | null> {
  return {
    get current() {
      const inner = canvas.current
      if (!inner) return null
      return {
        ...inner,
        fit: () => fitSlide(inner, area()),
        zoomTo: (zoom: number) => inner.zoomTo(clampZoom(zoom)),
        zoomBy: (factor: number) => {
          const view = inner.view()
          if (view) inner.zoomTo(stepZoom(view.zoom, factor >= 1 ? 1 : -1))
        },
      }
    },
  } as RefObject<KitCanvasHandle | null>
}
