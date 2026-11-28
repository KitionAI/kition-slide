/**
 * Dragging thumbnails, as WPS does it (plan stage 7, walkthrough B4; WPS W-K5): pointer events,
 * not the browser's drag and drop. After a press moves more than 4 px, the pressed thumbnail
 * follows the pointer (with the other selected ones stacked under it), the other thumbnails make
 * room for them as it goes, and the release puts them where the room was; there is no insertion
 * line. Near the panel's top or bottom it scrolls. Escape, or the pointer taken away, puts
 * everything back. One drop is one `slide.move`, one undo step.
 *
 * The thumbnails are moved with transforms on their elements (`.slides-slide[data-index]`), not
 * through React, so a drag costs no render of the panel. The slots are where the thumbnails are
 * laid out (in the list's own coordinates), so the same code serves the panel's column and the
 * sorter's grid.
 */
import { useEffect, useRef, type RefObject } from 'react'

/** How far a press moves before it is a drag (WPS W-A2), in CSS pixels. */
export const DRAG_START = 4
/** The band at the list's top and bottom that scrolls it while dragging, in CSS pixels. */
const EDGE = 32
/** The most the list scrolls per frame while dragging at its very edge, in CSS pixels. */
const MOST_SCROLL = 18
/** How far each slide under the pressed one is stacked, in CSS pixels. */
const STACK = 6

type Point = { x: number; y: number }

type Drag = {
  pointer: number
  /** Where the press was, on the page. */
  start: Point
  /** The pointer now, on the page. */
  at: Point
  /** The press's slide, and what a drag takes (in deck order) when it starts. */
  index: number
  dragged: number[]
  /** What a press that never became a drag does at its release. */
  click: (() => void) | null
  /** Set when the drag started: the thumbnails, where they are laid out, and the scroll then. */
  started: null | {
    elements: HTMLElement[]
    slots: Array<Point & { width: number; height: number }>
    /** The list's scroll when the drag started, and the most it scrolls (the dragged thumbnails' transforms widen what it could). */
    scrollLeft: number
    scrollTop: number
    most: number
  }
}

/** The thumbnails' elements, in deck order. */
function thumbnails(list: HTMLElement) {
  const found = [...list.querySelectorAll<HTMLElement>('.slides-slide[data-index]')]
  return found.sort((a, b) => Number(a.dataset.index) - Number(b.dataset.index))
}

/** Where `element` is laid out in `list`'s content (transforms aside). */
function slotOf(element: HTMLElement, list: HTMLElement) {
  const box = element.getBoundingClientRect()
  const frame = list.getBoundingClientRect()
  const transform = getComputedStyle(element).transform
  const matrix = new DOMMatrixReadOnly(transform && transform !== 'none' ? transform : undefined)
  return {
    x: box.left - frame.left + list.scrollLeft - matrix.e,
    y: box.top - frame.top + list.scrollTop - matrix.f,
    width: box.width,
    height: box.height,
  }
}

/**
 * Where the block of `dragged` goes among the thumbnails laid out in `slots`: the gap before the
 * `gap`-th of those that stay, which is how many of them come before the pressed thumbnail's
 * centre `centre` in reading order. In a column a thumbnail comes before it once the centre is
 * past the thumbnail's middle; in the sorter's grid, once the centre is below its row, or in its
 * row and past its middle.
 */
export function gapFor(slots: Array<Point & { width: number; height: number }>, dragged: number[], centre: Point) {
  const grid = new Set(slots.map((slot) => Math.round(slot.x))).size > 1
  let gap = 0
  slots.forEach((slot, index) => {
    if (dragged.includes(index)) return
    const before = grid
      ? centre.y > slot.y + slot.height || (centre.y >= slot.y && centre.x > slot.x + slot.width / 2)
      : centre.y > slot.y + slot.height / 2
    if (before) gap += 1
  })
  return gap
}

/** The deck order after the block of `dragged` (in deck order) goes into gap `gap` of `count` slides. */
export function orderAfter(count: number, dragged: number[], gap: number) {
  const stay = Array.from({ length: count }, (_, at) => at).filter((at) => !dragged.includes(at))
  return [...stay.slice(0, gap), ...dragged, ...stay.slice(gap)]
}

export type SlideDrag = {
  /**
   * A press on thumbnail `index`: `dragged` is what a drag takes (null: no drag, as for a viewer),
   * `click` what the release does when the pointer never moved far.
   */
  press: (event: PointerEvent | React.PointerEvent, index: number, dragged: number[] | null, click: (() => void) | null) => void
  /** Whether a drag is under way. */
  dragging: () => boolean
}

/**
 * Drags in `list`. `drop` gets the slides dragged (in deck order) and the gap among those that
 * stay where they go; it is not called when the order would not change.
 */
export function useSlideDrag(list: RefObject<HTMLElement | null>, drop: (dragged: number[], gap: number) => void): SlideDrag {
  const drag = useRef<Drag | null>(null)
  const latestDrop = useRef(drop)
  latestDrop.current = drop

  /** How far the pressed thumbnail has moved in the list's content (the pointer and the scroll). */
  const moved = (now: Drag, element: HTMLElement) => {
    const started = now.started
    if (!started) return { x: 0, y: 0 }
    return { x: now.at.x - now.start.x + element.scrollLeft - started.scrollLeft, y: now.at.y - now.start.y + element.scrollTop - started.scrollTop }
  }
  /** The gap the pressed thumbnail is over now, or -1. */
  const gapNow = (now: Drag, element: HTMLElement) => {
    const pressed = now.started?.slots[now.index]
    if (!now.started || !pressed) return -1
    const by = moved(now, element)
    return gapFor(now.started.slots, now.dragged, { x: pressed.x + pressed.width / 2 + by.x, y: pressed.y + pressed.height / 2 + by.y })
  }

  const layout = () => {
    const now = drag.current
    const element = list.current
    if (!now?.started || !element) return
    const { elements, slots } = now.started
    const pressed = slots[now.index]
    if (!pressed) return
    const by = moved(now, element)
    const gap = gapNow(now, element)
    const order = orderAfter(slots.length, now.dragged, gap)
    order.forEach((index, slot) => {
      const thumbnail = elements[index]
      const from = slots[index]
      const to = slots[slot]
      if (!thumbnail || !from || !to) return
      if (now.dragged.includes(index)) return
      thumbnail.style.transform = to.x === from.x && to.y === from.y ? '' : `translate(${to.x - from.x}px, ${to.y - from.y}px)`
    })
    // The pressed thumbnail follows the pointer; the other dragged ones are stacked under it.
    let rank = 0
    for (const index of now.dragged) {
      const thumbnail = elements[index]
      const from = slots[index]
      if (!thumbnail || !from) continue
      const stack = index === now.index ? 0 : (rank += 1) * STACK
      thumbnail.style.transform = `translate(${pressed.x + by.x + stack - from.x}px, ${pressed.y + by.y + stack - from.y}px)`
    }
  }

  /** Puts every thumbnail back where it is laid out; `animate` lets them slide there. */
  const settle = (animate: boolean) => {
    const element = list.current
    const now = drag.current
    drag.current = null
    if (!element || !now?.started) return
    if (!animate) element.dataset.settling = 'true'
    for (const thumbnail of now.started.elements) {
      thumbnail.style.transform = ''
      delete thumbnail.dataset.dragged
      delete thumbnail.dataset.dragCount
    }
    delete element.dataset.dragging
    if (!animate) requestAnimationFrame(() => delete element.dataset.settling)
  }

  useEffect(() => {
    let frame = 0
    const scroll = () => {
      frame = 0
      const now = drag.current
      const element = list.current
      if (!now?.started || !element) return
      const box = element.getBoundingClientRect()
      const above = now.at.y - box.top
      const below = box.bottom - now.at.y
      const step = above < EDGE ? -Math.ceil(((EDGE - above) / EDGE) * MOST_SCROLL) : below < EDGE ? Math.ceil(((EDGE - below) / EDGE) * MOST_SCROLL) : 0
      if (!step) return
      const before = element.scrollTop
      element.scrollTop = Math.max(0, Math.min(now.started.most, before + step))
      if (element.scrollTop !== before) layout()
      frame = requestAnimationFrame(scroll)
    }
    const move = (event: PointerEvent) => {
      const now = drag.current
      const element = list.current
      if (!now || event.pointerId !== now.pointer || !element) return
      now.at = { x: event.clientX, y: event.clientY }
      if (!now.started) {
        if (!now.dragged.length) return
        if (Math.hypot(now.at.x - now.start.x, now.at.y - now.start.y) <= DRAG_START) return
        const elements = thumbnails(element)
        now.started = {
          elements,
          slots: elements.map((thumbnail) => slotOf(thumbnail, element)),
          scrollLeft: element.scrollLeft,
          scrollTop: element.scrollTop,
          most: Math.max(0, element.scrollHeight - element.clientHeight),
        }
        element.dataset.dragging = 'true'
        for (const index of now.dragged) {
          const thumbnail = elements[index]
          if (thumbnail) thumbnail.dataset.dragged = index === now.index ? 'pressed' : 'stacked'
        }
        const pressed = elements[now.index]
        if (pressed && now.dragged.length > 1) pressed.dataset.dragCount = String(now.dragged.length)
      }
      event.preventDefault()
      layout()
      if (!frame) frame = requestAnimationFrame(scroll)
    }
    const up = (event: PointerEvent) => {
      const now = drag.current
      if (!now || event.pointerId !== now.pointer) return
      if (!now.started) {
        drag.current = null
        now.click?.()
        return
      }
      const element = list.current
      const gap = element ? gapNow(now, element) : -1
      const dragged = now.dragged
      const order = gap >= 0 ? orderAfter(now.started.slots.length, dragged, gap) : null
      const changed = order != null && order.some((index, at) => index !== at)
      settle(!changed)
      if (changed) latestDrop.current(dragged, gap)
    }
    const cancel = (event: PointerEvent) => {
      if (drag.current && event.pointerId === drag.current.pointer) settle(true)
    }
    const key = (event: KeyboardEvent) => {
      if (!drag.current?.started || event.key !== 'Escape') return
      event.preventDefault()
      event.stopPropagation()
      settle(true)
    }
    window.addEventListener('pointermove', move, true)
    window.addEventListener('pointerup', up, true)
    window.addEventListener('pointercancel', cancel, true)
    window.addEventListener('keydown', key, true)
    const away = () => settle(true)
    window.addEventListener('blur', away)
    return () => {
      window.removeEventListener('blur', away)
      if (frame) cancelAnimationFrame(frame)
      window.removeEventListener('pointermove', move, true)
      window.removeEventListener('pointerup', up, true)
      window.removeEventListener('pointercancel', cancel, true)
      window.removeEventListener('keydown', key, true)
    }
    // `layout` and `settle` read only refs.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [list])

  return {
    press: (event, index, dragged, click) => {
      drag.current = {
        pointer: event.pointerId,
        start: { x: event.clientX, y: event.clientY },
        at: { x: event.clientX, y: event.clientY },
        index,
        // Nothing to drag (a viewer): only the click.
        dragged: dragged && dragged.includes(index) ? [...dragged].sort((a, b) => a - b) : dragged ? [index] : [],
        click,
        started: null,
      }
    },
    dragging: () => drag.current?.started != null,
  }
}
