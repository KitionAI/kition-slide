import type { CSSProperties } from 'react'
import { useSlides, type HostMark } from './slides'

/**
 * Markers for the comment threads a host keeps (Kit UI protocol 2.6 section 7k): one per thread
 * of `comments/show`, at its shape's top-right corner, at the point it was left at, or at the
 * slide's corner; a click tells the host to open the thread (`comments/activated`).
 */

/** Where a thread's marker goes on its slide, in slide points. */
function markAt(mark: HostMark) {
  if (mark.rect) return { x: mark.rect.x + mark.rect.width, y: mark.rect.y }
  if (mark.point) return mark.point
  return { x: 0, y: 0 }
}

/** The slide's marks with where each goes, a mark that falls on another's moved along (`step`, screen pixels). */
function placed(marks: HostMark[], slide: number) {
  const taken = new Map<string, number>()
  return marks
    .filter((mark) => mark.slide === slide)
    .map((mark) => {
      const at = markAt(mark)
      const key = `${Math.round(at.x)},${Math.round(at.y)}`
      const stack = taken.get(key) ?? 0
      taken.set(key, stack + 1)
      return { mark, at, stack }
    })
}

function Marker({ mark, style, small = false }: { mark: HostMark; style: CSSProperties; small?: boolean }) {
  const { t, activeHostThread, openHostThread, session, run } = useSlides()
  return (
    <button
      type="button"
      className="slides-comment-marker"
      data-small={small}
      data-resolved={mark.resolved}
      aria-pressed={mark.id === activeHostThread}
      aria-label={t('comments.markerCount', { count: mark.count })}
      title={t('comments.markerCount', { count: mark.count })}
      style={style}
      data-testid={`slides-comment-marker${small ? '-thumb' : ''}-${mark.id}`}
      data-thread={mark.id}
      onPointerDown={(event) => event.stopPropagation()}
      onClick={(event) => {
        event.stopPropagation()
        if (mark.slide != null && mark.slide !== session.state?.current) void run('slide.go', { index: mark.slide })
        openHostThread(mark.id)
      }}
    >
      {mark.count > 0 ? mark.count : ''}
    </button>
  )
}

/** The current slide's markers over the canvas, placed by the canvas's view (`x`, `y`, `zoom`). */
export function CanvasMarkers({ view }: { view: { x: number; y: number; zoom: number } }) {
  const { session, hostMarks } = useSlides()
  const area = session.state?.area
  const current = session.state?.current ?? 0
  if (!area || !hostMarks.length) return null
  return (
    <div className="slides-comment-markers" data-testid="slides-comment-markers">
      {placed(hostMarks, current).map(({ mark, at, stack }) => (
        <Marker key={mark.id} mark={mark} style={{ left: (area.x0 + at.x - view.x) * view.zoom + stack * 22, top: (area.y0 + at.y - view.y) * view.zoom }} />
      ))}
    </div>
  )
}

/** A slide's markers over its thumbnail, placed in proportion to the slide. */
export function ThumbMarkers({ slide }: { slide: number }) {
  const { session, hostMarks } = useSlides()
  const area = session.state?.area
  if (!area || !hostMarks.length) return null
  const width = area.x1 - area.x0
  const height = area.y1 - area.y0
  if (!(width > 0 && height > 0)) return null
  return (
    <>
      {placed(hostMarks, slide).map(({ mark, at, stack }) => (
        <Marker
          key={mark.id}
          mark={mark}
          small
          style={{ left: `calc(${Math.min(100, Math.max(0, (at.x / width) * 100))}% + ${stack * 12}px)`, top: `${Math.min(100, Math.max(0, (at.y / height) * 100))}%` }}
        />
      ))}
    </>
  )
}
