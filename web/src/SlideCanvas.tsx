import { useEffect, useMemo, useRef, useState, type MouseEvent, type PointerEvent, type RefObject } from 'react'
import { KitDocumentCanvas, drawPresence, isMac, useKitTools, type KitCanvasHandle, type RasterDecoration, type RasterView } from '@kition/sdk/editor'
import { useSlides } from './slides'
import { CanvasMarkers } from './HostComments'
import { CanvasScrollbars, createViewStore } from './CanvasScrollbars'
import { slideScrollArea } from './useSlideWheel'
import { CARET_BLINK_MS, isCaretShown, setCaretShown } from './handles'
import { useInputMethod } from './inputMethod'
import { useFileDrop } from './clipboard'
import type { OpenMenu } from './ContextMenu'
import type { ContextTarget } from './contextMenus'

export function SlideCanvas({ canvas, viewMemory, onZoom, onMenu }: { canvas: RefObject<KitCanvasHandle | null>; viewMemory: RefObject<RasterView | null>; onZoom: (zoom: number) => void; onMenu: (menu: OpenMenu) => void }) {
  const { session, view, threads, activeThread, remoteSelections, hostComments, commentPoint, runWhenEnabled, contextMenu } = useSlides()
  const tools = useKitTools()
  const [position, setPosition] = useState<RasterView>({ x: 0, y: 0, zoom: 1 })
  const area = session.state?.area
  // An enlarged slide scrolls only a margin past its edges, by a drag as by the wheel; the scrollbars follow the view.
  const views = useMemo(createViewStore, [])
  const panBounds = useMemo(() => slideScrollArea(area, position.zoom), [area?.x0, area?.y0, area?.x1, area?.y1, position.zoom])
  // The caret blinks while text is edited (walkthrough E13): each beat draws the overlays again over
  // the slide already drawn. It shows at once after every answer of the tool (a key, a click).
  const editing = session.state?.editingText === true
  const overlays = tools?.decorate
  // An input method composes in the text at the caret, its candidate window under the caret's line
  // (plan stage 4, walkthrough E10).
  const inputMethod = useInputMethod({
    doc: session.doc,
    inOrder: session.inOrder,
    canvas,
    refresh: () => tools?.refresh(),
    typing: editing || tools?.wantsText === true,
    answer: overlays,
  })
  // Picture files dropped on the slide are inserted where they land (plan stage 5, walkthrough F8).
  const region = useRef<HTMLDivElement>(null)
  const dropping = useFileDrop(region)
  const [, setBeat] = useState(0)
  useEffect(() => {
    setCaretShown(true)
    if (!editing) return
    const timer = window.setInterval(() => {
      setCaretShown(!isCaretShown())
      setBeat((beat) => beat + 1)
    }, CARET_BLINK_MS)
    return () => {
      window.clearInterval(timer)
      setCaretShown(true)
    }
  }, [editing, overlays])
  const decorate: RasterDecoration = (context, toScreen, raster) => {
    if (area) {
      const a = toScreen(area.x0, area.y0), b = toScreen(area.x1, area.y1)
      context.save(); context.beginPath(); context.rect(a.x, a.y, b.x - a.x, b.y - a.y); context.clip()
      context.strokeStyle = getComputedStyle(document.documentElement).getPropertyValue('--kit-text-muted')
      context.lineWidth = 0.5
      if (view.gridlines) {
        context.globalAlpha = 0.3; context.beginPath()
        for (let x = area.x0 + 36; x < area.x1; x += 36) { const p = toScreen(x, area.y0); context.moveTo(p.x, a.y); context.lineTo(p.x, b.y) }
        for (let y = area.y0 + 36; y < area.y1; y += 36) { const p = toScreen(area.x0, y); context.moveTo(a.x, p.y); context.lineTo(b.x, p.y) }
        context.stroke()
      }
      if (view.guides) {
        context.globalAlpha = 0.65; context.setLineDash([5, 5])
        const p = toScreen((area.x0 + area.x1) / 2, (area.y0 + area.y1) / 2)
        context.beginPath(); context.moveTo(p.x, a.y); context.lineTo(p.x, b.y); context.moveTo(a.x, p.y); context.lineTo(b.x, p.y); context.stroke()
      }
      context.restore()
    }
    // A flag at each open comment thread of the current slide (at its shape's corner, else its point).
    if (area) {
      const current = session.state?.current
      context.save()
      // The kit's brand (kit.json `ui.brand`), as the host's theme gives it.
      context.fillStyle = getComputedStyle(document.documentElement).getPropertyValue('--kit-primary').trim() || '#c0550d'
      for (const thread of threads) {
        if (thread.resolved || thread.slide !== current) continue
        const box = thread.anchor.box
        const at = toScreen(area.x0 + (box ? box.x + box.width : thread.anchor.x), area.y0 + (box ? box.y : thread.anchor.y))
        context.globalAlpha = thread.id === activeThread ? 1 : 0.75
        context.beginPath(); context.roundRect(at.x - 2, at.y - 18, 18, 14, 4); context.fill()
        context.beginPath(); context.moveTo(at.x, at.y - 5); context.lineTo(at.x, at.y + 1); context.lineTo(at.x + 6, at.y - 5); context.fill()
      }
      context.restore()
    }
    // The shapes other people selected, outlined in their colors with their names (Kit UI protocol UI-P2).
    if (area && remoteSelections.length) {
      const selections = remoteSelections.map((item) => ({ ...item, rect: { ...item.rect, x: area.x0 + item.rect.x, y: area.y0 + item.rect.y } }))
      drawPresence(context, { selections }, toScreen)
    }
    tools?.decorate?.(context, toScreen, raster)
    // The text sink follows the caret as this frame draws it (the view may have moved).
    inputMethod.place(toScreen)
  }
  const ruler = (axis: 'x' | 'y') => {
    if (!area) return null
    const start = axis === 'x' ? area.x0 : area.y0, end = axis === 'x' ? area.x1 : area.y1
    return <div className={`slides-ruler slides-ruler-${axis}`} data-testid={`slides-ruler-${axis}`}>
      {Array.from({ length: Math.ceil((end - start) / 36) + 1 }, (_, index) => <span key={index} style={{ [axis === 'x' ? 'left' : 'top']: (start + index * 36 - position[axis]) * position.zoom }}>{index % 2 === 0 ? index / 2 : ''}</span>)}
    </div>
  }
  // Where the person presses on the slide: a comment with nothing selected goes there (a host's thread).
  const remember = (event: { target: EventTarget; clientX: number; clientY: number }) => {
    const surface = event.target
    if (!hostComments || !area || !(surface instanceof HTMLCanvasElement) || !surface.classList.contains('kui-canvas-surface')) return
    const box = surface.getBoundingClientRect()
    const x = (event.clientX - box.left) / position.zoom + position.x - area.x0
    const y = (event.clientY - box.top) / position.zoom + position.y - area.y0
    const inside = x >= 0 && y >= 0 && x <= area.x1 - area.x0 && y <= area.y1 - area.y0
    commentPoint.current = inside ? { slide: session.state?.current ?? 0, x, y } : null
  }
  const pressed = (event: PointerEvent) => {
    // Ctrl+click on the Mac is a right-click (its context menu follows): no press of the tool.
    if (isMac() && event.button === 0 && event.ctrlKey && !event.metaKey && onSurface(event.target)) {
      event.stopPropagation()
      return
    }
    remember(event)
  }
  // The context menu (plan stage 6A, walkthrough C11): the canvas gives the tool no right-click,
  // so the engine is asked here, in the input order, what it is for (the object pressed becomes
  // the selection there, the text being edited keeps its caret); then the menu opens at the
  // pointer. With no point (Shift+F10) the selection stays and the menu opens at it.
  const latest = useRef({ session, position, tools, onMenu, canvas })
  latest.current = { session, position, tools, onMenu, canvas }
  const openMenu = (point: { x: number; y: number } | null, client: { x: number; y: number } | null) => {
    const { session: now, position: shown } = latest.current
    const engine = now.doc
    if (!engine) return
    void now
      .inOrder(async () => {
        const answer = await engine.invoke<string>('contextMenu', point?.x ?? Number.NaN, point?.y ?? Number.NaN, shown.zoom)
        // A settled tool call brings the editor's state up to date (the selection may have changed).
        await now.tool('tool', [], true)
        return answer
      })
      .then((answer) => {
        const target = JSON.parse(answer) as ContextTarget
        const { tools: engineTools, onMenu: open, canvas: view } = latest.current
        engineTools?.refresh()
        void view.current?.retryRender()
        let at = client
        if (!at) {
          const surface = region.current?.querySelector('.kui-canvas-surface')
          const box = surface?.getBoundingClientRect()
          const zoom = latest.current.position.zoom
          at = box ? { x: box.left + (target.at[0] - latest.current.position.x) * zoom, y: box.top + (target.at[1] - latest.current.position.y) * zoom } : { x: 0, y: 0 }
        }
        open({ ...target, client: at })
      })
      .catch(() => undefined)
  }
  const menuAt = (event: MouseEvent) => {
    if (!onSurface(event.target)) return
    event.preventDefault()
    const box = (event.target as HTMLElement).getBoundingClientRect()
    remember(event)
    openMenu({ x: (event.clientX - box.left) / position.zoom + position.x, y: (event.clientY - box.top) / position.zoom + position.y }, { x: event.clientX, y: event.clientY })
  }
  const opener = useRef(openMenu)
  opener.current = openMenu
  useEffect(() => {
    const open = () => opener.current(null, null)
    contextMenu.current = open
    return () => {
      if (contextMenu.current === open) contextMenu.current = null
    }
  }, [contextMenu])
  // A third click in text being edited selects its paragraph (plan 0B): the engine's command, in the
  // input order after the presses before it, which decide whether text is being edited by then. The
  // canvas gives the tool presses and double clicks only.
  const clicked = (event: MouseEvent) => {
    if (event.detail !== 3 || !onSurface(event.target)) return
    void runWhenEnabled('text.selectParagraph')
  }
  return <div ref={region} className="slides-canvas-view" data-dropping={dropping || undefined} data-ruler={view.ruler} data-grayscale={view.grayscale} onPointerDownCapture={pressed} onClick={clicked} onContextMenu={menuAt}>
    {view.ruler ? <>{ruler('x')}{ruler('y')}</> : null}
    <KitDocumentCanvas canvas={canvas} viewMemory={viewMemory} initialArea={area ?? null} panBounds={panBounds} framed={false} decorate={decorate} onView={(raster) => { setPosition(raster); views.set(raster); onZoom(raster.zoom) }} testId="slides-canvas-region">
      {hostComments ? <CanvasMarkers view={position} /> : null}
      {/* The scrollbars: there only while the slide and its margin are longer than the view. */}
      <CanvasScrollbars views={views} content={(shown) => slideScrollArea(area, shown.zoom)} canvas={canvas} />
    </KitDocumentCanvas>
  </div>
}

/** Whether `target` is the canvas's drawing surface (the slide and what is around it). */
function onSurface(target: EventTarget | null): target is HTMLCanvasElement {
  return target instanceof HTMLCanvasElement && target.classList.contains('kui-canvas-surface')
}
