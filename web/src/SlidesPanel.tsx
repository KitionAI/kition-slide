/**
 * The slide panel, and the sorter, which is the same list laid out as a grid (plan stage 7,
 * walkthrough B2, B4, B6, B9, B12, A14; WPS W-K2 to W-K7). It is a second place to work on the
 * slides, as in WPS:
 *
 * - A click goes to a slide; Shift+click selects the run from the last slide clicked, Cmd+click
 *   (Ctrl+click off the Mac) adds a slide or takes it away; Shift+Up and Shift+Down extend the run,
 *   Cmd+A selects every slide. The selection is the engine's (`slide.selectSlides`, its
 *   `selectedSlides`), and the current slide is the one last chosen.
 * - Delete and Backspace, Cmd+C, Cmd+X and Cmd+V, Cmd+D and Cmd+Shift+D, the menu and a drag act on
 *   every selected slide, each one undo step. Return adds a slide after the current one (its
 *   layout following the current slide's); in the sorter Return and a double-click open the slide.
 * - A drag moves the slides live (`slideDrag.ts`), and the right-click menu is `slideMenu.ts`'s.
 * - Pictures are drawn again only for slides that changed (`thumbnails.tsx`).
 */
import { useEffect, useRef, useState, type KeyboardEvent, type PointerEvent as ReactPointerEvent } from 'react'
import { isMac, useReadOnly } from '@kition/sdk/editor'
import { useSlides } from './slides'
import { ThumbMarkers } from './HostComments'
import { CanvasMenu } from './ContextMenu'
import { slideMenuModel } from './slideMenu'
import { useSlideDrag, orderAfter } from './slideDrag'
import { Thumb, useThumbnails } from './thumbnails'

type Menu = { index: number; x: number; y: number }

/** The slides from `from` to `to`, `to` last (the one that becomes current). */
function span(from: number, to: number) {
  const step = to >= from ? 1 : -1
  const out: number[] = []
  for (let at = from; at !== to + step; at += step) out.push(at)
  return out
}

/** Cmd on the Mac, Ctrl elsewhere: the key that adds a slide to the selection or takes it away. */
const toggles = (event: { metaKey: boolean; ctrlKey: boolean }) => (isMac() ? event.metaKey && !event.ctrlKey : event.ctrlKey && !event.metaKey)

export function SlidesPanel() {
  const slides = useSlides()
  const { session, facts, query, t, view, enabled } = slides
  const { doc, change, state } = session
  const current = state?.current ?? 0
  const count = state?.slides ?? 0
  const selected = state?.selectedSlides?.length ? state.selectedSlides : [current]
  const list = useRef<HTMLDivElement>(null)
  const readOnly = useReadOnly()
  const sorter = view.mode === 'sorter'
  const thumbs = useThumbnails(doc, change.revision, count, current, list, view.mode !== 'outline')
  const [menu, setMenu] = useState<Menu | null>(null)
  /** The section being named (its index), while its name is a text field. */
  const [naming, setNaming] = useState<{ section: number; at: number } | null>(null)
  /** Where a Shift+click or Shift+arrow run starts: the slide last clicked. */
  const anchor = useRef(current)
  if (selected.length <= 1) anchor.current = current

  const [outline, setOutline] = useState<string[][]>([])
  useEffect(() => {
    if (view.mode !== 'outline') return
    let stale = false
    void Promise.all(Array.from({ length: count }, (_, index) => query<{ shapes: Array<{ text?: string }> }>('slide.inspect', { index, select: false }))).then((all) => {
      if (!stale) setOutline(all.map((slide) => (slide?.shapes ?? []).map((shape) => shape.text ?? '').filter(Boolean)))
    })
    return () => {
      stale = true
    }
  }, [view.mode, count, change.revision, query])

  // The current slide stays in view.
  useEffect(() => {
    list.current?.querySelector(`[data-index="${current}"]`)?.scrollIntoView({ block: 'nearest' })
  }, [current])

  const act = (id: string, params: Record<string, unknown> = {}) => slides.run(id, params)
  const go = (index: number) => {
    anchor.current = index
    void act('slide.go', { index })
  }
  /** Selects `indices`, the last one becoming current. */
  const choose = (indices: number[]) => {
    const valid = indices.filter((at) => at >= 0 && at < count)
    if (!valid.length) return
    if (valid.length === 1) return void act('slide.go', { index: valid[0] })
    void act('slide.selectSlides', { indices: valid })
  }
  const extendTo = (index: number) => choose(span(anchor.current, Math.max(0, Math.min(count - 1, index))))
  const toggle = (index: number) => {
    anchor.current = index
    if (selected.includes(index)) {
      if (selected.length === 1) return
      const rest = selected.filter((at) => at !== index)
      // The current slide stays current when it stays selected.
      choose(rest.includes(current) ? [...rest.filter((at) => at !== current), current] : rest)
    } else choose([...selected.filter((at) => at !== current), current, index].filter((at, k, all) => all.indexOf(at) === k))
  }
  /** Opens slide `index` in the normal view (the sorter's double-click and Return). */
  const open = (index: number) => {
    go(index)
    view.setMode('normal')
  }

  // Sections: which slide each starts at (they are in deck order), and each slide's section.
  const starts = new Map<number, number>()
  const sectionOf: number[] = []
  {
    let at = 0
    facts.sections.forEach((section, index) => {
      const size = typeof section.slides === 'number' ? section.slides : 0
      if (size > 0 || index === 0) starts.set(at, index)
      for (let k = 0; k < size; k += 1) sectionOf[at + k] = index
      at += size
    })
  }

  // A drop moves the dragged slides into the gap they were dropped in: one command, one undo step.
  const drag = useSlideDrag(list, (dragged, gap) => {
    if (readOnly) return
    const ids = thumbs.ids.length === count ? thumbs.ids : null
    if (ids) thumbs.reorder(orderAfter(count, dragged, gap).map((at) => ids[at] ?? -1))
    const moved = dragged.length === 1 ? act('slide.move', { from: dragged[0], to: gap }) : ids ? act('slide.move', { ids: dragged.map((at) => ids[at]), to: gap }) : Promise.resolve(null)
    void moved.finally(() => thumbs.refresh())
  })

  const pointerDown = (event: ReactPointerEvent<HTMLDivElement>, index: number) => {
    if (event.button !== 0 || naming) return
    // Ctrl+click on the Mac is the right-click (the menu), not a selection.
    if (isMac() && event.ctrlKey) return
    const canDrag = !readOnly && view.mode !== 'outline'
    if (event.shiftKey) {
      const indices = span(anchor.current, index)
      choose(indices)
      drag.press(event, index, canDrag ? indices : null, null)
      return
    }
    if (toggles(event)) {
      toggle(index)
      const now = selected.includes(index) ? selected.filter((at) => at !== index) : [...selected, index]
      drag.press(event, index, canDrag && now.includes(index) ? now : null, null)
      return
    }
    if (selected.includes(index) && selected.length > 1) {
      // A press on one of several selected slides drags them all; a click alone keeps only it.
      drag.press(event, index, canDrag ? selected : null, () => go(index))
      return
    }
    // The pressed slide is chosen at once, the slide's objects let go: it is the one a drag moves (walkthrough B4).
    go(index)
    drag.press(event, index, canDrag ? [index] : null, null)
  }

  const keyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (naming || event.altKey) return
    const command = isMac() ? event.metaKey : event.ctrlKey
    const key = event.key
    // In the sorter Up and Down go a row at a time.
    const columns = sorter ? Math.max(1, columnsOf(list.current)) : 1
    const step = key === 'ArrowDown' ? columns : key === 'ArrowUp' ? -columns : key === 'ArrowRight' ? 1 : key === 'ArrowLeft' ? -1 : 0
    if (step && !command) {
      const to = Math.max(0, Math.min(count - 1, current + step))
      if (event.shiftKey) extendTo(to)
      else go(to)
    } else if ((key === 'Home' || key === 'End') && !command) {
      const to = key === 'Home' ? 0 : Math.max(0, count - 1)
      if (event.shiftKey) extendTo(to)
      else go(to)
    } else if (command && !event.shiftKey && (key.toLowerCase() === 'a' || event.code === 'KeyA')) {
      choose([...Array.from({ length: count }, (_, at) => at).filter((at) => at !== current), current])
    } else if (command && (key.toLowerCase() === 'd' || event.code === 'KeyD')) {
      // Cmd+D and Cmd+Shift+D duplicate the selected slides (WPS W-K7).
      if (!readOnly) void act('slide.duplicate')
    } else if (!command && !event.shiftKey && (key === 'Delete' || key === 'Backspace')) {
      if (!readOnly) void act('slide.delete')
    } else if (!command && !event.shiftKey && key === 'Enter') {
      if (sorter) open(current)
      else if (!readOnly) void act('slide.new', { at: current + 1 })
    } else return
    event.preventDefault()
    event.stopPropagation()
  }

  const closeMenu = () => {
    setMenu(null)
    list.current?.focus({ preventScroll: true })
  }
  const nameSection = (section: number, name: string) => {
    setNaming(null)
    list.current?.focus({ preventScroll: true })
    const was = facts.sections[section]?.name
    if (name.trim() && name !== was) void act('section.rename', { index: section, name: name.trim() })
  }

  // A new section named by the person: its name field opens once the section is there.
  const [addedAt, setAddedAt] = useState<number | null>(null)
  useEffect(() => {
    if (addedAt == null) return
    const section = starts.get(addedAt)
    if (section == null) return
    setAddedAt(null)
    setNaming({ section, at: addedAt })
    // `starts` is derived from `facts`.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [addedAt, facts.sections])

  const several = (index: number) => selected.length > 1 && selected.includes(index)

  return (
    <aside className="kui-panel slides-slides" data-side="left" data-mode={view.mode} data-testid={sorter ? 'slides-sorter' : 'slides-slides'}>
      <div
        className="slides-slides-list"
        ref={list}
        tabIndex={0}
        onKeyDown={keyDown}
        role="listbox"
        aria-multiselectable="true"
        aria-label={t('group.slides')}
        data-drawn={thumbs.drawn}
      >
        {Array.from({ length: count }, (_, index) => {
          const section = starts.get(index)
          const id = thumbs.ids[index]
          return (
            <div key={index} className="slides-slide-row">
              {section != null && facts.sections[section] ? (
                naming?.section === section ? (
                  <input
                    className="slides-section-name"
                    data-testid="slides-section-name"
                    aria-label={t('sp.sectionName')}
                    defaultValue={facts.sections[section]?.name ?? ''}
                    autoFocus
                    onFocus={(event) => event.currentTarget.select()}
                    onKeyDown={(event) => {
                      event.stopPropagation()
                      if (event.key === 'Enter') nameSection(section, event.currentTarget.value)
                      else if (event.key === 'Escape') {
                        event.preventDefault()
                        setNaming(null)
                        list.current?.focus({ preventScroll: true })
                      }
                    }}
                    onBlur={(event) => naming && nameSection(section, event.currentTarget.value)}
                  />
                ) : (
                  <div className="slides-section" data-testid={`slides-section-${section}`} onDoubleClick={() => !readOnly && setNaming({ section, at: index })}>
                    {facts.sections[section]?.name}
                  </div>
                )
              ) : null}
              <div
                className="slides-slide"
                data-index={index}
                data-current={index === current}
                data-selected={selected.includes(index)}
                data-hidden={facts.slides[index]?.hidden === true}
                role="option"
                aria-selected={selected.includes(index)}
                onPointerDown={(event) => pointerDown(event, index)}
                onDoubleClick={() => sorter && open(index)}
                onContextMenu={(event) => {
                  event.preventDefault()
                  if (!selected.includes(index)) go(index)
                  setMenu({ index, x: event.clientX, y: event.clientY })
                }}
                data-testid={`slides-slide-${index}`}
              >
                <span className="slides-slide-number">{index + 1}</span>
                {view.mode === 'outline' ? (
                  <div className="slides-outline-text">{(outline[index]?.length ? outline[index] : [facts.slides[index]?.title ?? t('slideOf', { slide: index + 1, slides: count })]).map((text, line) => <p key={line}>{text}</p>)}</div>
                ) : (
                  <div className="slides-thumb-frame">
                    <Thumb image={id != null ? thumbs.images.get(id) ?? null : null} />
                    <ThumbMarkers slide={index} />
                  </div>
                )}
              </div>
            </div>
          )
        })}
      </div>
      {menu ? (
        <CanvasMenu
          at={{ x: menu.x, y: menu.y }}
          label={t('sp.label')}
          testId="slides-slide-menu"
          rowIds="slides-menu-"
          onClose={closeMenu}
          model={slideMenuModel({
            t,
            enabled,
            run: (id, params) => void slides.run(id, params),
            readOnly,
            index: menu.index,
            several: several(menu.index),
            hidden: (several(menu.index) ? selected : [menu.index]).every((at) => facts.slides[at]?.hidden === true),
            section: facts.sections.length ? (sectionOf[menu.index] ?? null) : null,
            layouts: facts.layouts,
            addSection: () => {
              setAddedAt(menu.index)
              void act('section.add', { at: menu.index, name: t('sp.newSection') })
            },
            renameSection: (section) => {
              const at = [...starts].find(([, index]) => index === section)?.[0] ?? menu.index
              setNaming({ section, at })
            },
          })}
        />
      ) : null}
    </aside>
  )
}

/** How many thumbnails the sorter's first row holds. */
function columnsOf(list: HTMLElement | null) {
  const items = [...(list?.querySelectorAll<HTMLElement>('.slides-slide') ?? [])]
  const top = items[0]?.offsetTop
  if (top === undefined) return 1
  let columns = 0
  for (const item of items) {
    if (item.offsetTop !== top) break
    columns += 1
  }
  return columns
}
