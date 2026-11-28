/**
 * Input methods (Chinese, Japanese, Korean) in the text being edited on a slide (plan stage 4,
 * walkthrough E10; WPS W-R1, W-R2). kit-editor's text sink, a hidden text field, is where the
 * input method composes; the sink hands the engine the finished text only. Here:
 *
 * - what is being composed shows in the text at the caret, underlined and laid out with it
 *   (`setPreedit`, in the input order), without changing the slides; an empty composition (the
 *   input method's Escape) takes it away, and the finished text the sink types ends it, as one undo
 *   step of its own;
 * - the text sink sits on the caret's line (`textCaret`; at the composition's start while
 *   composing), so the candidate window opens just under that line, lined up with the composition;
 * - while composing, every key is the input method's (Enter picks, Escape cancels, the arrows and
 *   Backspace move in the composition): none reaches the tool, the editor's keys or the commands.
 */
import { useEffect, useRef, type RefObject } from 'react'
import type { KitCanvasHandle, KitEngine } from '@kition/sdk/editor'

/** A point of the canvas (points) on the canvas surface (CSS pixels), as the frame drawn has it. */
export type ToScreen = (x: number, y: number) => { x: number; y: number }

/** The text sink an event came from, if it did. */
function sinkOf(target: EventTarget | null): HTMLTextAreaElement | null {
  return target instanceof HTMLTextAreaElement && target.classList.contains('kui-text-sink') ? target : null
}

/** Whether a key belongs to an input method's composition (its first one says so only by its code). */
export function composingKey(event: KeyboardEvent) {
  return event.isComposing || event.keyCode === 229 || event.key === 'Process'
}

/**
 * The input method's cursor in the composition `text`, in characters (the engine's unit): where
 * `field` has it once its value is the composition, else at its end (a composition event can come
 * before the field shows what it composes).
 */
function cursorIn(field: HTMLTextAreaElement, text: string) {
  const length = Array.from(text).length
  if (field.value !== text) return length
  const end = Math.min(field.selectionEnd ?? text.length, text.length)
  return Math.min(Array.from(text.slice(0, end)).length, length)
}

type Inputs = {
  doc: KitEngine | null
  inOrder: <R>(work: () => Promise<R>) => Promise<R>
  canvas: RefObject<KitCanvasHandle | null>
  /** Asks the tool for its overlays again (the underline, the caret). */
  refresh: () => void
  /** Text on the slide is edited, or typing goes into the one selected shape. */
  typing: boolean
  /** Moves with every answer of the tool (its overlays): the caret may have moved. */
  answer: unknown
}

/**
 * Shows compositions in the text being edited, keeps their keys from the editor, and keeps the
 * text sink on the caret's line. Answers `place`, for the canvas to call with each frame's mapping.
 */
export function useInputMethod({ doc, inOrder, canvas, refresh, typing, answer }: Inputs) {
  const latest = useRef({ doc, inOrder, canvas, refresh, typing })
  latest.current = { doc, inOrder, canvas, refresh, typing }
  /** The caret's line in canvas points, `[x0, y0, x1, y1]`, while text is edited. */
  const caret = useRef<number[] | null>(null)
  const mapping = useRef<ToScreen | null>(null)

  const place = (toScreen?: ToScreen) => {
    if (toScreen) mapping.current = toScreen
    const line = caret.current
    const map = mapping.current
    if (!line || !map || !latest.current.typing) return
    const field = document.querySelector<HTMLTextAreaElement>('textarea.kui-text-sink')
    const surface = document.querySelector<HTMLElement>('.slides-canvas-view .kui-canvas-surface')
    if (!field || !surface) return
    const a = map(line[0], line[1])
    const b = map(line[2], line[3])
    const box = surface.getBoundingClientRect()
    const holder = (field.offsetParent ?? document.body).getBoundingClientRect()
    const left = Math.round(box.left + Math.min(a.x, b.x) - holder.left)
    const top = Math.round(box.top + Math.min(a.y, b.y) - holder.top)
    const height = Math.max(1, Math.round(Math.abs(b.y - a.y)))
    if (![left, top, height].every(Number.isFinite)) return
    const style = field.style
    const next = { left: `${left}px`, top: `${top}px`, height: `${height}px` }
    if (style.left === next.left && style.top === next.top && style.height === next.height) return
    Object.assign(style, next)
    // The composition stays on one line, as tall as the caret's, so the candidate window opens
    // under the line and starts where the composition does.
    style.lineHeight = `${height}px`
    style.fontSize = `${Math.max(1, Math.round(height * 0.8))}px`
    style.whiteSpace = 'pre'
  }

  /** Asks the engine where the caret's line is now, then puts the text sink there. */
  const follow = () => {
    const engine = latest.current.doc
    if (!engine || !latest.current.typing) return
    void engine
      .invoke<ArrayLike<number> | null | undefined>('textCaret')
      .then((line) => {
        caret.current = line && line.length === 4 ? Array.from(line) : null
        place()
      })
      .catch(() => undefined)
  }
  const followRef = useRef(follow)
  followRef.current = follow

  useEffect(() => {
    /** One composition update to the engine, after the input before it. */
    const send = (text: string, cursor: number, repaint: boolean) => {
      const { doc: engine, inOrder: ordered } = latest.current
      if (!engine) return
      void ordered(() => engine.invoke<boolean>('setPreedit', text, cursor))
        .then(() => {
          if (!repaint) return
          void latest.current.canvas.current?.retryRender()
          latest.current.refresh()
          followRef.current()
        })
        .catch(() => undefined)
    }
    const update = (event: CompositionEvent) => {
      const field = sinkOf(event.target)
      if (!field || !latest.current.typing) return
      const text = event.data ?? ''
      // A composition starts empty: nothing to show yet.
      if (event.type === 'compositionstart' && !text) return
      send(text, cursorIn(field, text), true)
    }
    const end = (event: CompositionEvent) => {
      if (!sinkOf(event.target) || !latest.current.typing) return
      // A cancelled composition ends with nothing to type: it goes away now. A committed one is
      // typed by the text sink (after this listener), which ends it in the engine; the empty one
      // sent after that only makes sure no composition is left showing.
      if (!event.data) send('', 0, true)
      else window.setTimeout(() => send('', 0, false), 0)
    }
    const key = (event: KeyboardEvent) => {
      if (sinkOf(event.target) && composingKey(event)) event.stopPropagation()
    }
    window.addEventListener('compositionstart', update, true)
    window.addEventListener('compositionupdate', update, true)
    window.addEventListener('compositionend', end, true)
    window.addEventListener('keydown', key, true)
    window.addEventListener('keyup', key, true)
    return () => {
      window.removeEventListener('compositionstart', update, true)
      window.removeEventListener('compositionupdate', update, true)
      window.removeEventListener('compositionend', end, true)
      window.removeEventListener('keydown', key, true)
      window.removeEventListener('keyup', key, true)
    }
  }, [])

  // After every answer of the tool while typing goes to the slide: the caret may have moved.
  useEffect(() => {
    if (!typing) {
      caret.current = null
      return
    }
    followRef.current()
  }, [typing, answer, doc])

  return { place }
}
