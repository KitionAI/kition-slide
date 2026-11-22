import { useSyncExternalStore, type RefObject } from 'react'
import { useKitEditor, type KitCanvasHandle, type KitCanvasView, type RasterRect } from '@kition/sdk/editor'
import { Scrollbar } from '@kition/tessera/ui'

/** The canvas's view, for the scrollbars only: a scroll renders them, not the editor around them. */
export function createViewStore() {
  let view: KitCanvasView | null = null
  const listeners = new Set<() => void>()
  return {
    get: () => view,
    set: (next: KitCanvasView) => {
      view = next
      for (const listener of listeners) listener()
    },
    subscribe: (listener: () => void) => {
      listeners.add(listener)
      return () => listeners.delete(listener)
    },
  }
}
export type ViewStore = ReturnType<typeof createViewStore>

/**
 * The canvas's scrollbars, over its right and bottom edges. Each is there only while the content
 * (`content`, in document pixels at the view's zoom) is longer than the view along its axis; it
 * shows where the view is in the content and moves it. Rendered inside the canvas (its children).
 */
export function CanvasScrollbars({ views, content, canvas }: {
  views: ViewStore
  content: (view: KitCanvasView) => RasterRect | null
  canvas: RefObject<KitCanvasHandle | null>
}) {
  const { t } = useKitEditor()
  const view = useSyncExternalStore(views.subscribe, views.get)
  const area = view && view.zoom > 0 ? content(view) : null
  if (!view || !area) return null
  const pageX = view.width / view.zoom
  const pageY = view.height / view.zoom
  const maxX = area.x1 - area.x0 - pageX
  const maxY = area.y1 - area.y0 - pageY
  // Longer than the view by more than a screen pixel.
  const horizontal = maxX * view.zoom > 1
  const vertical = maxY * view.zoom > 1
  if (!horizontal && !vertical) return null
  return (
    <div className="slides-scrollbars" data-both={horizontal && vertical ? true : undefined} data-testid="slides-scrollbars">
      {vertical ? <Scrollbar label={t('verticalScroll')} value={view.y - area.y0} max={maxY} page={pageY} onChange={(value) => canvas.current?.panTo(view.x, area.y0 + value)} /> : null}
      {horizontal ? <Scrollbar label={t('horizontalScroll')} orientation="horizontal" value={view.x - area.x0} max={maxX} page={pageX} onChange={(value) => canvas.current?.panTo(area.x0 + value, view.y)} /> : null}
    </div>
  )
}
