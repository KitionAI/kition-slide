import { useEffect, useRef, useState, type DragEvent, type KeyboardEvent } from 'react'
import { CopyIcon as Copy, EyeOffIcon as EyeOff, PlusIcon as Plus, TrashIcon as Trash2 } from '@kition/tessera/icons/react'
import { MenuItem, MenuSeparator, useReadOnly } from '@kition/sdk/editor'
import { useSlides } from './slides'
import { ThumbMarkers } from './HostComments'

const THUMB_WIDTH = 176
const REDRAW_MS = 250

type Menu = { index: number; x: number; y: number }

/** The slide list: a numbered thumbnail per slide, under its section, the current one marked. */
export function SlidesPanel() {
  const { session, facts, run, query, t, view } = useSlides()
  const { doc, change, state } = session
  const [thumbs, setThumbs] = useState<Map<number, ImageData>>(new Map())
  const [menu, setMenu] = useState<Menu | null>(null)
  const [dragged, setDragged] = useState<number | null>(null)
  const current = state?.current ?? 0
  const count = state?.slides ?? 0
  const list = useRef<HTMLDivElement>(null)
  const [outline, setOutline] = useState<string[][]>([])
  useEffect(() => {
    if (view.mode !== 'outline') return
    let stale = false
    void Promise.all(Array.from({ length: count }, (_, index) => query<{ shapes: Array<{ text?: string }> }>('slide.inspect', { index, select: false }))).then((slides) => {
      if (!stale) setOutline(slides.map((slide) => (slide?.shapes ?? []).map((shape) => shape.text ?? '').filter(Boolean)))
    })
    return () => { stale = true }
  }, [view.mode, count, change.revision, query])

  // Thumbnails follow every change, a moment after it: the current slide first, then the rest,
  // each shown as soon as it is drawn (the old one stays until then).
  useEffect(() => {
    if (!doc || !count) return undefined
    let stale = false
    const timer = window.setTimeout(async () => {
      const order = [current, ...Array.from({ length: count }, (_, index) => index).filter((index) => index !== current)]
      for (const index of order) {
        if (stale) return
        try {
          const rgba = await doc.invoke<Uint8Array>('renderSlide', index, THUMB_WIDTH)
          const height = Math.round(rgba.length / 4 / THUMB_WIDTH)
          const image = new ImageData(new Uint8ClampedArray(rgba.buffer as ArrayBuffer, rgba.byteOffset, rgba.byteLength), THUMB_WIDTH, height)
          if (!stale) setThumbs((previous) => new Map(previous).set(index, image))
        } catch {
          // A slide that cannot be drawn keeps its last thumbnail.
        }
      }
    }, REDRAW_MS)
    return () => {
      stale = true
      window.clearTimeout(timer)
    }
  }, [change.revision, count, current, doc])

  // The current slide stays in view.
  useEffect(() => {
    list.current?.querySelector(`[data-index="${current}"]`)?.scrollIntoView({ block: 'nearest' })
  }, [current])

  // A viewer goes from slide to slide; adding, moving, hiding, and deleting slides edit the slide file.
  const readOnly = useReadOnly()
  const go = (index: number) => void run('slide.go', { index })
  const keyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key === 'ArrowDown' || event.key === 'ArrowRight') go(Math.min(count - 1, current + 1))
    else if (event.key === 'ArrowUp' || event.key === 'ArrowLeft') go(Math.max(0, current - 1))
    else if (!readOnly && (event.key === 'Delete' || event.key === 'Backspace')) void run('slide.delete', { index: current })
    else if (!readOnly && event.key === 'Enter') void run('slide.new', { at: current + 1 })
    else return
    event.preventDefault()
    event.stopPropagation()
  }
  const drop = (event: DragEvent, to: number) => {
    event.preventDefault()
    if (dragged != null && dragged !== to) void run('slide.move', { from: dragged, to })
    setDragged(null)
  }

  const sectionAt = new Map<number, string>()
  for (const section of facts.sections) {
    const start = section.start ?? section.slides?.[0]
    if (start != null) sectionAt.set(start, section.name)
  }

  return (
    <aside className="kui-panel slides-slides" data-side="left" data-mode={view.mode} data-testid={view.mode === 'sorter' ? 'slides-sorter' : 'slides-slides'}>
      <div className="slides-slides-list" ref={list} tabIndex={0} onKeyDown={keyDown} role="listbox" aria-label={t('group.slides')}>
        {Array.from({ length: count }, (_, index) => (
          <div key={index}>
            {sectionAt.has(index) ? <div className="slides-section">{sectionAt.get(index)}</div> : null}
            <div
              className="slides-slide"
              data-index={index}
              data-current={index === current}
              data-hidden={facts.slides[index]?.hidden === true}
              role="option"
              aria-selected={index === current}
              draggable={!readOnly}
              onDragStart={() => setDragged(index)}
              onDragOver={(event) => event.preventDefault()}
              onDrop={(event) => drop(event, index)}
              onClick={() => go(index)}
              onContextMenu={(event) => {
                event.preventDefault()
                go(index)
                setMenu({ index, x: event.clientX, y: event.clientY })
              }}
              data-testid={`slides-slide-${index}`}
            >
              <span className="slides-slide-number">{index + 1}</span>
              {view.mode === 'outline' ? <div className="slides-outline-text">{(outline[index]?.length ? outline[index] : [facts.slides[index]?.title ?? t('slideOf', { slide: index + 1, slides: count })]).map((text, line) => <p key={line}>{text}</p>)}</div> : <div className="slides-thumb-frame"><Thumb image={thumbs.get(index) ?? null} /><ThumbMarkers slide={index} /></div>}
            </div>
          </div>
        ))}
      </div>
      {menu ? (
        <div className="kui-popover" role="menu" style={{ left: menu.x, top: menu.y }} onPointerLeave={() => setMenu(null)}>
          <MenuItem icon={Plus} label={t('newSlide')} disabled={readOnly} onSelect={() => { setMenu(null); void run('slide.new', { at: menu.index + 1 }) }} />
          <MenuItem icon={Copy} label={t('duplicateSlide')} disabled={readOnly} onSelect={() => { setMenu(null); void run('slide.duplicate', { index: menu.index }) }} />
          <MenuItem icon={EyeOff} label={t('hideSlide')} disabled={readOnly} onSelect={() => { setMenu(null); void run('slide.hide', { index: menu.index, hidden: !facts.slides[menu.index]?.hidden }) }} />
          <MenuSeparator />
          <MenuItem icon={Trash2} label={t('deleteSlide')} disabled={readOnly} onSelect={() => { setMenu(null); void run('slide.delete', { index: menu.index }) }} />
        </div>
      ) : null}
    </aside>
  )
}

export function Thumb({ image }: { image: ImageData | null }) {
  const canvas = useRef<HTMLCanvasElement>(null)
  useEffect(() => {
    if (!image || !canvas.current) return
    canvas.current.width = image.width
    canvas.current.height = image.height
    canvas.current.getContext('2d')?.putImageData(image, 0, 0)
  }, [image])
  return <canvas ref={canvas} className="slides-thumb" width={THUMB_WIDTH} height={Math.round((THUMB_WIDTH * 9) / 16)} />
}
