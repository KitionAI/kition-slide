/**
 * The editor's own keys (plan 0A, walkthrough A12, C7, G6, I3, K4, K6, K7; WPS W-A11, W-H3, W-Q1),
 * beside the engine tool's keys and the commands' shortcuts. Two places, both in the frame's input
 * order (a command runs through `run`, after the tool events still on their way):
 *
 * - `beforeTool`: keys the tool would otherwise take: Cmd+Enter and F5 start the show with shapes
 *   selected. And the text keys that follow the platform (plan stage 3, walkthrough E3, E9; WPS
 *   W-F4): the engine's keys are the Mac's (Cmd+Left / Right to the line's ends, Alt+Left / Right
 *   by word), so on Windows and Linux Ctrl+Left / Right, Up / Down, Backspace and Delete go to it
 *   as Alt ones (by word, by paragraph), and on the Mac Ctrl+A / Ctrl+E as Home / End. F2 (which
 *   the frame does not hand the tool) edits the selected shape's text with all of it selected.
 * - `afterTool`: keys the tool did not take: with nothing selected, Home and End go to the first
 *   and last slide, the arrows to the next and previous one, Tab selects the first object (Shift+Tab
 *   the last). Delete goes on to the engine's Delete, which deletes nothing then (only the slide
 *   list deletes slides, WPS W-Q1). Cmd+Shift+Z redoes as Cmd+Y does, Cmd+K opens the link dialog,
 *   Cmd+Alt+M a new comment, Cmd+1 shows 100%.
 */
import { isMac, type KitCanvasHandle } from '@kition/sdk/editor'
import { useEffect, type RefObject } from 'react'
import type { Slides } from './slides'
import { editorRoot } from './focus'

type Key = { key: string; code?: string; shiftKey: boolean; altKey: boolean; metaKey: boolean; ctrlKey: boolean; target?: EventTarget | null }

/** A menu, a group's panel or a popup is open: Escape is theirs. */
export function popupOpen() {
  return document.querySelector('.kui-popover') != null
}

/**
 * Escape while a menu or a group's panel is open closes it and does nothing else (walkthrough K4:
 * it also let go of the selection). The key stops before the editor's handlers and is handed to
 * the window's listeners alone, where menus and panels close.
 */
export function usePopupEscape() {
  useEffect(() => {
    const keyDown = (event: KeyboardEvent) => {
      if (!event.isTrusted || event.key !== 'Escape' || event.shiftKey || event.altKey || event.metaKey || event.ctrlKey || event.isComposing) return
      if (!popupOpen() || document.querySelector('.kui-dialog-backdrop')) return
      event.stopPropagation()
      window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', code: 'Escape', cancelable: true }))
    }
    document.addEventListener('keydown', keyDown, true)
    return () => document.removeEventListener('keydown', keyDown, true)
  }, [])
}

/** Whether the key was pressed on the slide (the frame itself or the canvas), not on a control. */
function onSlide(target: EventTarget | null | undefined) {
  if (!(target instanceof Element)) return true
  return target === editorRoot() || target === document.body || target.classList.contains('kui-canvas-surface') || target.classList.contains('kui-text-sink')
}

export function slideKeys(slides: Slides, canvas: RefObject<KitCanvasHandle | null>) {
  const { session, view, run, query } = slides
  const command = (event: Key) => event.metaKey || event.ctrlKey
  const plain = (event: Key) => !event.metaKey && !event.ctrlKey && !event.altKey && !event.shiftKey
  const show = (fromStart: boolean) => {
    if (!session.doc || !session.state?.slides) return
    view.present(fromStart ? 0 : session.state.current)
  }

  /** The tool's own handling of a key (the frame's input order). */
  type Forward = (event: Key) => boolean
  const textKey = (event: Key, forward: Forward) => {
    if (!session.state?.editingText || !event.ctrlKey || event.metaKey || event.altKey) return false
    const as = (key: string, alt: boolean) => forward({ key, code: key, shiftKey: event.shiftKey, altKey: alt, metaKey: false, ctrlKey: false, target: event.target })
    if (isMac()) {
      const key = event.key.toLowerCase()
      if (key === 'a' || event.code === 'KeyA') return as('Home', false)
      if (key === 'e' || event.code === 'KeyE') return as('End', false)
      return false
    }
    if (['ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown'].includes(event.key)) return as(event.key, true)
    if ((event.key === 'Backspace' || event.key === 'Delete') && !event.shiftKey) return as(event.key, true)
    return false
  }

  const beforeTool = (event: Key, forward: Forward) => {
    if (textKey(event, forward)) return true
    if (event.key === 'F2' && plain(event)) {
      if (session.state && session.state.selected > 0 && !session.state.editingText) void run('text.edit', { all: true })
      return session.state != null && session.state.selected > 0
    }
    if (command(event) && !event.altKey && event.key === 'Enter' && !session.state?.editingText) {
      show(event.shiftKey)
      return true
    }
    if (event.key === 'F5' && !command(event) && !event.altKey) {
      show(!event.shiftKey)
      return true
    }
    return false
  }

  const afterTool = (event: Key) => {
    const code = event.code ?? ''
    if (command(event) && event.shiftKey && !event.altKey && event.key.toLowerCase() === 'z') {
      if (slides.enabled('edit.redo')) void run('edit.redo', {})
      return true
    }
    if (command(event) && !event.shiftKey && !event.altKey && (code === 'KeyK' || event.key.toLowerCase() === 'k')) {
      if (session.state && (session.state.selected > 0 || session.state.editingText)) slides.setLinking(true)
      return true
    }
    if (command(event) && event.altKey && !event.shiftKey && (code === 'KeyM' || event.key.toLowerCase() === 'm')) {
      if (slides.canNewComment) void slides.newComment()
      return true
    }
    if (command(event) && !event.shiftKey && !event.altKey && (event.key === '1' || code === 'Digit1')) {
      canvas.current?.zoomTo(1)
      return true
    }
    const state = session.state
    if (!state || state.selected > 0 || state.editingText || state.target.kind !== 'slides' || !onSlide(event.target)) return false
    if (!plain(event) && !(event.key === 'Tab' && event.shiftKey && !command(event) && !event.altKey)) return false
    switch (event.key) {
      case 'Home':
        void run('slide.first', {})
        return true
      case 'End':
        void run('slide.last', {})
        return true
      case 'ArrowDown':
      case 'ArrowRight':
        void run('slide.next', {})
        return true
      case 'ArrowUp':
      case 'ArrowLeft':
        void run('slide.previous', {})
        return true
      case 'Tab': {
        const backwards = event.shiftKey
        void query<{ shapes?: Array<{ id: number; hidden?: boolean }> }>('slide.inspect', { index: state.current, select: false }).then((slide) => {
          const shapes = (slide?.shapes ?? []).filter((shape) => !shape.hidden)
          const shape = backwards ? shapes.at(-1) : shapes[0]
          if (shape) void run('edit.select', { ids: [shape.id] })
        })
        return true
      }
      default:
        return false
    }
  }

  return { beforeTool, afterTool }
}
