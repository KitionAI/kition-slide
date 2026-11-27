/**
 * How a selected object's handles look (plan 0A, walkthrough C2; WPS W-B1): the engine places them
 * (`bindings/wasm` `overlays`, anchors in its own colours) and this draws them so they show on any
 * slide: the eight resize handles and a line's ends as white circles 9 px across with a dark ring,
 * the rotation handle as a grey circle with a turning arrow, a shape's adjust handles as yellow
 * diamonds. They sit on the slide's own colours, not the editor's, so they keep fixed colours that
 * read on white and on black alike, as the selection outline does. Everything else is kit-editor's
 * default drawing.
 */
import type { OverlayDrawer } from '@kition/sdk/editor'

/** The ring around every handle: dark enough on white, framed by the white fill on black. */
const RING = 'rgba(64, 64, 64, 1)'
const WHITE = 'rgba(255, 255, 255, 1)'
const ROTATE_FILL = 'rgba(112, 112, 112, 1)'
const ADJUST_FILL = 'rgba(255, 210, 63, 1)'
/** Diameter of a resize handle and of an adjust diamond, in CSS pixels. */
const HANDLE = 9
/** Diameter of the rotation handle, in CSS pixels. */
const ROTATE = 13

type Rgba = [number, number, number, number]
const isColor = (color: Rgba, r: number, g: number, b: number) => color[0] === r && color[1] === g && color[2] === b

export const drawSlideHandle: OverlayDrawer = (overlay, { context, toScreen }) => {
  if (overlay.kind !== 'anchor') return false
  const { x, y } = toScreen(overlay.x, overlay.y)
  context.save()
  context.lineWidth = 1
  context.strokeStyle = RING
  if (overlay.filled && isColor(overlay.color, 255, 255, 255)) {
    // A resize handle, or a line's end.
    context.beginPath()
    context.arc(x, y, HANDLE / 2, 0, Math.PI * 2)
    context.fillStyle = WHITE
    context.fill()
    context.stroke()
  } else if (overlay.filled) {
    // An adjust handle (the engine's yellow).
    const r = HANDLE / 2 + 0.5
    context.beginPath()
    context.moveTo(x, y - r)
    context.lineTo(x + r, y)
    context.lineTo(x, y + r)
    context.lineTo(x - r, y)
    context.closePath()
    context.fillStyle = ADJUST_FILL
    context.fill()
    context.stroke()
  } else {
    // The rotation handle: a grey disc with a turning arrow in white.
    const r = ROTATE / 2
    context.beginPath()
    context.arc(x, y, r, 0, Math.PI * 2)
    context.fillStyle = ROTATE_FILL
    context.fill()
    context.strokeStyle = WHITE
    context.stroke()
    context.lineWidth = 1.25
    context.beginPath()
    context.arc(x, y, r - 3, -Math.PI * 0.35, Math.PI * 1.25)
    context.stroke()
    const tip = { x: x + (r - 3) * Math.cos(-Math.PI * 0.35), y: y + (r - 3) * Math.sin(-Math.PI * 0.35) }
    context.beginPath()
    context.moveTo(tip.x - 2.5, tip.y - 1)
    context.lineTo(tip.x, tip.y)
    context.lineTo(tip.x - 0.5, tip.y + 2.5)
    context.stroke()
  }
  context.restore()
  return true
}
