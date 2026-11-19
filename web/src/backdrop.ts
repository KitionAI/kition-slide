/**
 * The canvas around the slides follows the host's theme. The engine paints that pasteboard itself
 * (`setBackdrop`, `#rrggbb`), so CSS cannot reach it: the editor gives it the theme's
 * `--kit-canvas` when the document opens (the manifest's setup) and again whenever the host
 * changes the theme (`ui/theme`, applied at once, no reload), and the canvas repaints. Only the
 * on-screen editor does this; previews, export and print never call `setBackdrop`.
 */
import { useEffect, type RefObject } from 'react'
import type { KitCanvasHandle, KitEngine } from '@kition/kit-editor'

const HEX = /^#[0-9a-f]{6}$/i

/**
 * The theme's `--kit-canvas` as `#rrggbb`: the token is any CSS color (Kition sends `hsl(...)`),
 * which the browser resolves. `null` when it is unset or not an opaque color.
 */
export function canvasColor(root: HTMLElement = document.documentElement): string | null {
  const token = getComputedStyle(root).getPropertyValue('--kit-canvas').trim()
  if (HEX.test(token)) return token.toLowerCase()
  if (!token || typeof CSS === 'undefined' || !CSS.supports('color', token)) return null
  const context = document.createElement('canvas').getContext('2d')
  if (!context) return null
  context.fillStyle = token
  const resolved = String(context.fillStyle)
  return HEX.test(resolved) ? resolved.toLowerCase() : null
}

/** Keeps the engine's pasteboard the theme's canvas color while `doc` is open, repainting `canvas` on a change. */
export function useEngineBackdrop(doc: KitEngine | null, canvas: RefObject<KitCanvasHandle | null>) {
  useEffect(() => {
    if (!doc) return undefined
    let open = true
    let given: string | null = null
    const follow = () => {
      const color = canvasColor()
      if (!color || color === given) return
      given = color
      void doc
        .invoke('setBackdrop', color)
        .then(() => (open ? canvas.current?.retryRender() : undefined))
        .catch(() => undefined)
    }
    follow()
    // kit-editor's `applyTheme` writes the tokens on the root's style and marks its data-color-scheme.
    const observer = typeof MutationObserver === 'undefined' ? null : new MutationObserver(follow)
    observer?.observe(document.documentElement, { attributes: true, attributeFilter: ['style', 'data-color-scheme', 'class'] })
    return () => {
      open = false
      observer?.disconnect()
    }
  }, [doc, canvas])
}
