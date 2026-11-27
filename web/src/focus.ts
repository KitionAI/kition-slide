/**
 * Where the keyboard goes when nothing in the editor holds it (plan 0A, walkthrough K1, K2, K4).
 * The editor's keys (the engine's tool, the shortcuts, the commands) belong to kit-editor's frame
 * (`.kui-app`) and run only while the focus is inside it. A page just opened, a slide show that
 * ended, a menu row that went away with its menu, a closed dialog or a field's Enter all used to
 * leave the focus on the page's `<body>`, and then no key worked until the slide was clicked.
 */
import { useEffect } from 'react'

/** kit-editor's frame: it holds the keyboard when no control in it does. */
export function editorRoot(): HTMLElement | null {
  return document.querySelector<HTMLElement>('.kui-app')
}

/** Whether no element of the editor holds the keyboard (the page's body has it). */
export function keyboardLost() {
  const active = document.activeElement
  return !active || active === document.body || active === document.documentElement || !active.isConnected
}

/**
 * Gives the keyboard to the editor's frame, or to its text field while text on the slide is edited
 * (so an input method keeps composing there); answers whether it has it now.
 */
export function focusEditor(editingText = false) {
  const target = (editingText ? document.querySelector<HTMLElement>('.kui-text-sink') : null) ?? editorRoot()
  if (!target) return false
  target.focus({ preventScroll: true })
  return document.activeElement === target
}

/** After something closed: once the focus settled, the editor has the keyboard if nothing else took it. */
export function returnKeyboardLater() {
  window.setTimeout(() => {
    if (keyboardLost() && document.hasFocus()) focusEditor()
  }, 0)
}

/** A field let go of the keyboard (its Enter, a choice made): the editor has it next. */
export function releaseField(field: HTMLElement, editingText = false) {
  if (!focusEditor(editingText)) field.blur()
}

/** A control a person types into: the keyboard is theirs there, never taken from it. */
function typingIn(element: Element | null) {
  if (!element) return false
  if (element instanceof HTMLTextAreaElement || element instanceof HTMLSelectElement) return true
  if (element instanceof HTMLElement && element.isContentEditable) return true
  return element instanceof HTMLInputElement && !['button', 'submit', 'reset', 'checkbox', 'radio', 'file', 'range', 'color', 'image'].includes(element.type)
}

/**
 * The slides just opened: the person means to work in them, so the editor takes the keyboard, as a
 * desktop application does with the window it opened. Not while the host has someone typing in one
 * of its fields (its search box): the host's own page, when the editor can see it, must have the
 * keyboard on nothing, a button or the editor's frame; a host on another origin is asked through
 * the focus of this page only.
 */
export function takeKeyboardOnOpen() {
  if (!keyboardLost()) return
  let hostIdle = document.hasFocus()
  if (!hostIdle) {
    try {
      const host = window.parent !== window ? window.parent.document : null
      hostIdle = host !== null && !typingIn(host.activeElement)
    } catch {
      hostIdle = false
    }
  }
  if (hostIdle) focusEditor()
}

/**
 * The safety net: a key pressed while the body has the keyboard gives the frame the keyboard and
 * the key itself, once (a capture listener on the document, before any other). A blur with nowhere
 * to go (a dialog closed, a field left) returns the keyboard to the frame as well.
 */
export function useKeyboardFallback() {
  useEffect(() => {
    const keyDown = (event: KeyboardEvent) => {
      if (event.defaultPrevented || event.isComposing || event.keyCode === 229 || !keyboardLost()) return
      if (event.target !== document.body && event.target !== document.documentElement) return
      if (!focusEditor()) return
      const root = editorRoot()
      if (!root) return
      event.preventDefault()
      event.stopImmediatePropagation()
      root.dispatchEvent(
        new KeyboardEvent('keydown', {
          key: event.key,
          code: event.code,
          location: event.location,
          repeat: event.repeat,
          shiftKey: event.shiftKey,
          altKey: event.altKey,
          ctrlKey: event.ctrlKey,
          metaKey: event.metaKey,
          bubbles: true,
          cancelable: true,
          composed: true,
        }),
      )
    }
    const focusOut = (event: FocusEvent) => {
      if (event.relatedTarget === null) returnKeyboardLater()
    }
    document.addEventListener('keydown', keyDown, true)
    document.addEventListener('focusout', focusOut)
    return () => {
      document.removeEventListener('keydown', keyDown, true)
      document.removeEventListener('focusout', focusOut)
    }
  }, [])
}
