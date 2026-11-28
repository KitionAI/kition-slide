/**
 * A modifier pressed or let go in the middle of a drag counts at once (plan stage 8; WPS W-B4,
 * W-B5, W-E8: Shift keeps the proportions or the axis, Cmd anchors at the centre, Alt stops the
 * snapping). The frame sends the engine's tool a pointer event only when the pointer moves, so
 * while a button is held on the canvas, a Shift, Alt or Cmd key sends the drag again where the
 * pointer is, with the keys now held. The engine computes a drag from where it began, so the
 * repeat changes only what the keys change.
 */
import { useEffect, type RefObject } from 'react'

const MODIFIERS = new Set(['Shift', 'Alt', 'Meta'])

type Held = { surface: HTMLCanvasElement; pointerId: number; pointerType: string; clientX: number; clientY: number }

export function useGestureModifiers(region: RefObject<HTMLElement | null>) {
  useEffect(() => {
    const root = region.current
    if (!root) return
    let held: Held | null = null
    const down = (event: PointerEvent) => {
      const surface = event.target
      if (!event.isTrusted || event.button !== 0 || !(surface instanceof HTMLCanvasElement) || !surface.classList.contains('kui-canvas-surface')) return
      held = { surface, pointerId: event.pointerId, pointerType: event.pointerType, clientX: event.clientX, clientY: event.clientY }
    }
    const move = (event: PointerEvent) => {
      if (!event.isTrusted || !held || event.pointerId !== held.pointerId) return
      held.clientX = event.clientX
      held.clientY = event.clientY
    }
    const up = (event: PointerEvent) => {
      if (held && event.pointerId === held.pointerId) held = null
    }
    const key = (event: KeyboardEvent) => {
      if (!held || !event.isTrusted || event.repeat || !MODIFIERS.has(event.key)) return
      const repeat = new PointerEvent('pointermove', {
        bubbles: true,
        cancelable: true,
        composed: true,
        pointerId: held.pointerId,
        pointerType: held.pointerType,
        isPrimary: true,
        clientX: held.clientX,
        clientY: held.clientY,
        buttons: 1,
        shiftKey: event.shiftKey,
        altKey: event.altKey,
        metaKey: event.metaKey,
        ctrlKey: event.ctrlKey,
      })
      // The frame reads a move's coalesced samples, which a made event does not carry.
      Object.defineProperty(repeat, 'getCoalescedEvents', { value: () => [repeat] })
      held.surface.dispatchEvent(repeat)
    }
    root.addEventListener('pointerdown', down, true)
    window.addEventListener('pointermove', move, true)
    window.addEventListener('pointerup', up, true)
    window.addEventListener('pointercancel', up, true)
    window.addEventListener('keydown', key, true)
    window.addEventListener('keyup', key, true)
    return () => {
      root.removeEventListener('pointerdown', down, true)
      window.removeEventListener('pointermove', move, true)
      window.removeEventListener('pointerup', up, true)
      window.removeEventListener('pointercancel', up, true)
      window.removeEventListener('keydown', key, true)
      window.removeEventListener('keyup', key, true)
    }
  }, [region])
}
