import { useMemo, useState, type PointerEvent, type RefObject } from 'react'
import { KitDocumentCanvas, drawPresence, useKitTools, type KitCanvasHandle, type RasterDecoration, type RasterView } from '@kition/sdk/editor'
import { useSlides } from './slides'
import { CanvasMarkers } from './HostComments'
import { CanvasScrollbars, createViewStore } from './CanvasScrollbars'
import { slideScrollArea } from './useSlideWheel'

export function SlideCanvas({ canvas, viewMemory, onZoom }: { canvas: RefObject<KitCanvasHandle | null>; viewMemory: RefObject<RasterView | null>; onZoom: (zoom: number) => void }) {
  const { session, view, threads, activeThread, remoteSelections, hostComments, commentPoint } = useSlides()
  const tools = useKitTools()
  const [position, setPosition] = useState<RasterView>({ x: 0, y: 0, zoom: 1 })
  const area = session.state?.area
  // An enlarged slide scrolls only a margin past its edges, by a drag as by the wheel; the scrollbars follow the view.
  const views = useMemo(createViewStore, [])
  const panBounds = useMemo(() => slideScrollArea(area, position.zoom), [area?.x0, area?.y0, area?.x1, area?.y1, position.zoom])
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
  }
  const ruler = (axis: 'x' | 'y') => {
    if (!area) return null
    const start = axis === 'x' ? area.x0 : area.y0, end = axis === 'x' ? area.x1 : area.y1
    return <div className={`slides-ruler slides-ruler-${axis}`} data-testid={`slides-ruler-${axis}`}>
      {Array.from({ length: Math.ceil((end - start) / 36) + 1 }, (_, index) => <span key={index} style={{ [axis === 'x' ? 'left' : 'top']: (start + index * 36 - position[axis]) * position.zoom }}>{index % 2 === 0 ? index / 2 : ''}</span>)}
    </div>
  }
  // Where the person presses on the slide: a comment with nothing selected goes there (a host's thread).
  const pressed = (event: PointerEvent) => {
    const surface = event.target
    if (!hostComments || !area || !(surface instanceof HTMLCanvasElement) || !surface.classList.contains('kui-canvas-surface')) return
    const box = surface.getBoundingClientRect()
    const x = (event.clientX - box.left) / position.zoom + position.x - area.x0
    const y = (event.clientY - box.top) / position.zoom + position.y - area.y0
    const inside = x >= 0 && y >= 0 && x <= area.x1 - area.x0 && y <= area.y1 - area.y0
    commentPoint.current = inside ? { slide: session.state?.current ?? 0, x, y } : null
  }
  return <div className="slides-canvas-view" data-ruler={view.ruler} data-grayscale={view.grayscale} onPointerDownCapture={pressed}>
    {view.ruler ? <>{ruler('x')}{ruler('y')}</> : null}
    <KitDocumentCanvas canvas={canvas} viewMemory={viewMemory} initialArea={area ?? null} panBounds={panBounds} framed={false} decorate={decorate} onView={(raster) => { setPosition(raster); views.set(raster); onZoom(raster.zoom) }} testId="slides-canvas-region">
      {hostComments ? <CanvasMarkers view={position} /> : null}
      {/* The scrollbars: there only while the slide and its margin are longer than the view. */}
      <CanvasScrollbars views={views} content={(shown) => slideScrollArea(area, shown.zoom)} canvas={canvas} />
    </KitDocumentCanvas>
  </div>
}
