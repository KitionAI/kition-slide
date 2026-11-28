/**
 * The slide panel's pictures (plan stage 7, walkthrough B11, B12; A14): each thumbnail is drawn
 * again only when its slide's picture changed. After a change the engine says, per slide, a stamp
 * of what its picture is drawn from (`slideStamps`: the slide, its layout, master and theme, its
 * number, the footers); only the slides whose stamp moved are drawn again, the current one first.
 * Pictures are kept by slide id, so a slide moved keeps its picture at its new place. They are
 * drawn at the size they are shown (times the device pixel ratio), so the sorter's larger ones are
 * sharp, and as the slide shows: no placeholder prompts.
 */
import { useEffect, useRef, useState, type RefObject } from 'react'
import type { KitEngine } from '@kition/sdk/editor'

/** The width a thumbnail is drawn at before its frame has been measured, in pixels. */
const FIRST_WIDTH = 176
/** A moment after a change, so a burst of keys draws once. */
const AFTER_MS = 120
/** The widest picture drawn, in pixels. */
const MOST_WIDTH = 960

export type Thumbnails = {
  /** The slides' ids in deck order, as the engine last said (moved at once by `reorder`). */
  ids: number[]
  /** Each slide's picture, by slide id. */
  images: Map<number, ImageData>
  /** How many pictures were drawn so far (the hand tests count them). */
  drawn: number
  /** The order the panel shows until the engine's next answer (a drop, before the move is back). */
  reorder: (ids: number[]) => void
  /** Asks the engine again (after a move that may have been refused). */
  refresh: () => void
}

/** The pixel width the thumbnails of `list` are shown at, in steps of 16 (a few pixels of resizing draws nothing again). */
function shownWidth(list: HTMLElement | null) {
  const frame = list?.querySelector<HTMLElement>('.slides-thumb-frame')
  if (!frame || !frame.clientWidth) return FIRST_WIDTH
  const pixels = frame.clientWidth * (window.devicePixelRatio || 1)
  return Math.min(MOST_WIDTH, Math.max(32, Math.ceil(pixels / 16) * 16))
}

/** Pairs `[id, stamp, id, stamp, …]` as ids and stamps. */
function pairs(flat: ArrayLike<number> | null | undefined) {
  const ids: number[] = []
  const stamps: number[] = []
  for (let at = 0; flat && at + 1 < flat.length; at += 2) {
    ids.push(Number(flat[at]))
    stamps.push(Number(flat[at + 1]))
  }
  return { ids, stamps }
}

export function useThumbnails(doc: KitEngine | null, revision: number, count: number, current: number, list: RefObject<HTMLElement | null>, active = true): Thumbnails {
  const [ids, setIds] = useState<number[]>([])
  const [images, setImages] = useState<Map<number, ImageData>>(() => new Map())
  const [width, setWidth] = useState(FIRST_WIDTH)
  const [fonts, setFonts] = useState(0)
  const [drawn, setDrawn] = useState(0)
  /** What each slide's picture was drawn from: its stamp and width. */
  const made = useRef(new Map<number, { stamp: number; width: number }>())

  // The shown size, as the panel or the sorter is resized.
  useEffect(() => {
    const element = list.current
    if (!element || typeof ResizeObserver === 'undefined') return
    const measure = () => setWidth(shownWidth(element))
    measure()
    const observer = new ResizeObserver(measure)
    observer.observe(element)
    return () => observer.disconnect()
  }, [list, count])

  // Fonts that came later change what text looks like: every picture is drawn again.
  useEffect(() => {
    if (!doc) return
    return doc.onFonts(() => {
      made.current.clear()
      setFonts((value) => value + 1)
    })
  }, [doc])

  useEffect(() => {
    if (!doc || !count || !active) return
    let stale = false
    const timer = window.setTimeout(async () => {
      let flat: ArrayLike<number> | null = null
      try {
        flat = await doc.invoke<Uint32Array>('slideStamps')
      } catch {
        return
      }
      if (stale) return
      const now = pairs(flat)
      setIds((previous) => (previous.length === now.ids.length && previous.every((id, at) => id === now.ids[at]) ? previous : now.ids))
      for (const id of [...made.current.keys()]) if (!now.ids.includes(id)) made.current.delete(id)
      const order = [current, ...now.ids.map((_, at) => at).filter((at) => at !== current)]
      for (const at of order) {
        const id = now.ids[at]
        const stamp = now.stamps[at]
        if (id === undefined || stamp === undefined) continue
        const was = made.current.get(id)
        if (was && was.stamp === stamp && was.width === width) continue
        if (stale) return
        try {
          const rgba = await doc.invoke<Uint8Array>('renderSlide', at, width)
          if (stale) return
          const height = Math.round(rgba.length / 4 / width)
          if (!height) continue
          const image = new ImageData(new Uint8ClampedArray(rgba.buffer as ArrayBuffer, rgba.byteOffset, width * height * 4), width, height)
          made.current.set(id, { stamp, width })
          setImages((previous) => new Map(previous).set(id, image))
          setDrawn((value) => value + 1)
        } catch {
          // A slide that cannot be drawn keeps its last picture.
        }
      }
    }, AFTER_MS)
    return () => {
      stale = true
      window.clearTimeout(timer)
    }
  }, [doc, revision, count, current, width, fonts, active])

  return { ids, images, drawn, reorder: setIds, refresh: () => setFonts((value) => value + 1) }
}

/** A picture of a slide on a canvas (the slide panel's, the notes page's, a design idea's). */
export function Thumb({ image }: { image: ImageData | null }) {
  const canvas = useRef<HTMLCanvasElement>(null)
  useEffect(() => {
    if (!image || !canvas.current) return
    canvas.current.width = image.width
    canvas.current.height = image.height
    canvas.current.getContext('2d')?.putImageData(image, 0, 0)
  }, [image])
  return <canvas ref={canvas} className="slides-thumb" width={FIRST_WIDTH} height={Math.round((FIRST_WIDTH * 9) / 16)} />
}
