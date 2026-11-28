import { useCallback, useEffect, useRef, useState } from 'react'
import type { KitEngine } from '@kition/sdk/editor'
import { ChevronLeftIcon, ChevronRightIcon, XIcon } from '@kition/tessera/icons/react'
import type { SlideShowOptions } from './useSlideView'
import { useSlides } from './slides'
import { CanvasMenu } from './ContextMenu'
import type { MenuEntry, MenuModel } from './contextMenus'
import { wheelNotches } from './zoom'
import { PAGE_DELAY_MS, PAGE_THRESHOLD, SWIPE_DISTANCE, SWIPE_END_MS } from './useSlideWheel'

/** How long the pointer rests before it and the control bar hide (plan stage 11, W-M6). */
export const CONTROLS_HIDE_MS = 2500
/** How long a typed slide number waits for its Enter. */
const TYPED_CLEAR_MS = 3000

type Screen = 'black' | 'white' | null

/**
 * The slide show (plan stage 11, walkthrough L1–L4, WPS W-M2–W-M6): one slide at a time, full
 * screen, rendered at the screen's resolution. A click, Space, Right, Down, Return, Page Down and N
 * go on; Left, Up, Backspace, Page Up and P go back; Home and End go to the first and the last
 * slide; a typed number and Return go to that slide; B and W black or white the screen until a key
 * or a click; the wheel turns pages as in the editor. After the last slide a black end screen says
 * so, and one more step ends the show; Esc ends it at once. The right button opens a menu (Next,
 * Previous, First and Last Slide, Go to Slide, Screen, End Show). The pointer and a control bar at
 * the bottom left (Previous, Next, End Show) show while the pointer moves and hide after it rested
 * 2.5 s (not while it is on the bar). `onEnd` gets the slide shown last, where the editor goes.
 * Transitions and animations are not played.
 */
export function SlideShow({ doc, from, slides, onEnd, hint, options }: { doc: KitEngine; from: number; slides: number; onEnd: (last: number) => void; hint: string; options: SlideShowOptions }) {
  const { query, run, t, facts } = useSlides()
  /** The slide shown, from 0; `slides` is the end screen. */
  const [index, setIndex] = useState(from)
  const [screen, setScreen] = useState<Screen>(null)
  const [typed, setTyped] = useState('')
  const [controls, setControls] = useState(false)
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null)
  const stage = useRef<HTMLDivElement>(null)
  const canvas = useRef<HTMLCanvasElement>(null)
  const nextCanvas = useRef<HTMLCanvasElement>(null)
  const [notes, setNotes] = useState('')
  const entered = useRef(performance.now())

  // What the listeners read: the latest values, so they are added once.
  const latest = useRef({ index, screen, typed, slides, onEnd, menu })
  latest.current = { index, screen, typed, slides, onEnd, menu }
  const menuOpen = useRef(false)
  /** When the menu last closed or an Escape took back a typed number (an Escape then does not end the show). */
  const layerEscape = useRef(-Infinity)
  const closeMenu = useCallback(() => {
    // Leaving full screen closes the menu too (the window resizes) before the browser says so.
    if (menuOpen.current) layerEscape.current = performance.now()
    menuOpen.current = false
    setMenu(null)
  }, [])
  const ended = useRef(false)
  /** Ends the show once, on the last slide shown (the last slide after the end screen). */
  const end = useCallback(() => {
    if (ended.current) return
    ended.current = true
    const { index: at, slides: count, onEnd: done } = latest.current
    done(Math.max(0, Math.min(at, count - 1)))
  }, [])
  const go = useCallback((to: number) => {
    setScreen(null)
    setIndex(Math.max(0, Math.min(to, latest.current.slides - 1)))
  }, [])
  const advance = useCallback(() => {
    const { index: at, slides: count, screen: blank } = latest.current
    if (blank) return setScreen(null)
    if (at >= count) return end()
    if (options.rehearse) void run('transition.timing', { index: at, after: Math.round(performance.now() - entered.current) })
    entered.current = performance.now()
    setIndex(at + 1)
  }, [end, options.rehearse, run])
  const back = useCallback(() => {
    const { index: at, screen: blank } = latest.current
    if (blank) return setScreen(null)
    entered.current = performance.now()
    setIndex(Math.max(0, at - 1))
  }, [])
  const blank = useCallback((which: Exclude<Screen, null>) => setScreen((now) => (now === which ? null : which)), [])

  useEffect(() => {
    if (options.reading) return
    void stage.current?.requestFullscreen?.().catch(() => undefined)
    // The browser leaves full screen on Escape whatever the page does. An Escape that closed the
    // menu or took back a typed number does not end the show: it goes on in the window.
    const onFullscreen = () => {
      if (document.fullscreenElement) return
      if (menuOpen.current) return closeMenu()
      if (latest.current.typed) return setTyped('')
      if (performance.now() - layerEscape.current < 1000) return
      end()
    }
    document.addEventListener('fullscreenchange', onFullscreen)
    return () => {
      document.removeEventListener('fullscreenchange', onFullscreen)
      if (document.fullscreenElement) void document.exitFullscreen().catch(() => undefined)
    }
  }, [closeMenu, end, options.reading])

  useEffect(() => {
    if (!options.presenter) return
    let stale = false
    if (index >= slides) {
      setNotes('')
      return
    }
    void query<{ notes?: string }>('slide.inspect', { index, select: false }).then((slide) => { if (!stale) setNotes(slide?.notes ?? '') })
    if (index + 1 < slides) void doc.invoke<Uint8Array>('renderSlide', index + 1, 480).then((rgba) => {
      if (!stale && nextCanvas.current) {
        nextCanvas.current.width = 480; nextCanvas.current.height = rgba.length / 4 / 480
        nextCanvas.current.getContext('2d')?.putImageData(new ImageData(new Uint8ClampedArray(rgba), 480, rgba.length / 4 / 480), 0, 0)
      }
    })
    return () => { stale = true }
  }, [doc, index, options.presenter, query, slides])

  useEffect(() => {
    if (index >= slides) return
    let stale = false
    const width = Math.round(Math.min(window.screen.width, 3840) * Math.min(window.devicePixelRatio || 1, 2))
    void doc.invoke<Uint8Array>('renderSlide', index, width).then((rgba) => {
      if (stale || !canvas.current) return
      const height = Math.round(rgba.length / 4 / width)
      canvas.current.width = width
      canvas.current.height = height
      canvas.current.getContext('2d')?.putImageData(new ImageData(new Uint8ClampedArray(rgba.buffer as ArrayBuffer, rgba.byteOffset, rgba.byteLength), width, height), 0, 0)
    })
    return () => {
      stale = true
    }
  }, [doc, index, slides])

  // A slide deleted by someone else while the show runs: the show stays inside the deck.
  useEffect(() => {
    if (index > slides) setIndex(slides)
  }, [index, slides])

  // A typed slide number waits 3 s for its Return.
  useEffect(() => {
    if (!typed) return
    const timer = window.setTimeout(() => setTyped(''), TYPED_CLEAR_MS)
    return () => window.clearTimeout(timer)
  }, [typed])

  // The keys are the show's while it runs: none reaches the editor behind it. An open menu has them.
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.isComposing) return
      if (menuOpen.current) {
        if (event.key === 'Escape') layerEscape.current = performance.now()
        return
      }
      event.stopPropagation()
      if (event.metaKey || event.ctrlKey || event.altKey) return
      const { typed: number, slides: count } = latest.current
      const key = event.key
      if (/^[0-9]$/.test(key)) setTyped((value) => (value + key).slice(-4))
      else if (key === 'Enter' && number) {
        const to = Number(number)
        if (to >= 1 && to <= count) go(to - 1)
        setTyped('')
      } else if (key === 'Backspace' && number) setTyped(number.slice(0, -1))
      else if (key === 'Escape' && number) {
        layerEscape.current = performance.now()
        setTyped('')
      }
      else if (key === 'Escape') end()
      else if (['ArrowRight', 'ArrowDown', 'PageDown', ' ', 'Enter', 'n', 'N'].includes(key)) advance()
      else if (['ArrowLeft', 'ArrowUp', 'PageUp', 'Backspace', 'p', 'P'].includes(key)) back()
      else if (key === 'Home') go(0)
      else if (key === 'End') go(count - 1)
      else if (key === 'b' || key === 'B' || key === '.') blank('black')
      else if (key === 'w' || key === 'W' || key === ',') blank('white')
      else if (key !== 'Tab') return
      event.preventDefault()
      if (key !== 'Enter' && key !== 'Backspace' && !/^[0-9]$/.test(key)) setTyped('')
    }
    window.addEventListener('keydown', onKey, true)
    return () => window.removeEventListener('keydown', onKey, true)
  }, [advance, back, blank, end, go])

  // The wheel turns pages as in the editor (`useSlideWheel`): a mouse notch past 18 px, once per
  // 350 ms; a trackpad swipe once, after 60 px. It stops at the end screen: leaving takes a click or a key.
  useEffect(() => {
    const element = stage.current
    if (!element) return
    let lastPage = -Infinity
    let swipe = { distance: 0, turned: false, last: -Infinity }
    const turn = (forward: boolean) => {
      if (forward && latest.current.index >= latest.current.slides && !latest.current.screen) return
      if (forward) advance()
      else back()
    }
    const wheel = (event: WheelEvent) => {
      if (menuOpen.current || event.ctrlKey || event.metaKey) return
      event.preventDefault()
      const unit = event.deltaMode === WheelEvent.DOM_DELTA_LINE ? 16 : event.deltaMode === WheelEvent.DOM_DELTA_PAGE ? element.clientHeight : 1
      const dx = event.deltaX * unit
      const dy = event.deltaY * unit
      const now = performance.now()
      if (wheelNotches(event) === 0) {
        if (now - swipe.last > SWIPE_END_MS) swipe = { distance: 0, turned: false, last: now }
        swipe.last = now
        if (Math.abs(dy) < Math.abs(dx) || swipe.turned) return
        swipe.distance += dy
        if (Math.abs(swipe.distance) < SWIPE_DISTANCE) return
        swipe.turned = true
        lastPage = now
        turn(swipe.distance > 0)
      } else if (Math.abs(dy) > PAGE_THRESHOLD && Math.abs(dy) >= Math.abs(dx) && now - lastPage >= PAGE_DELAY_MS) {
        lastPage = now
        turn(dy > 0)
      }
    }
    element.addEventListener('wheel', wheel, { passive: false })
    return () => element.removeEventListener('wheel', wheel)
  }, [advance, back])

  // The pointer and the control bar show while the pointer moves (not in Reading View, where they
  // always show) and hide after it rested 2.5 s, unless it rests on the bar.
  const always = options.reading
  const hideTimer = useRef(0)
  const onBar = useRef(false)
  const lastPoint = useRef<{ x: number; y: number } | null>(null)
  const restartHide = useCallback(() => {
    window.clearTimeout(hideTimer.current)
    hideTimer.current = window.setTimeout(() => {
      if (!onBar.current) setControls(false)
    }, CONTROLS_HIDE_MS)
  }, [])
  useEffect(() => () => window.clearTimeout(hideTimer.current), [])
  const onPointerMove = (event: React.PointerEvent) => {
    const last = lastPoint.current
    lastPoint.current = { x: event.clientX, y: event.clientY }
    // A browser sends a move that is none as the show opens under a still pointer.
    const moved = event.movementX !== 0 || event.movementY !== 0 || (last != null && (last.x !== event.clientX || last.y !== event.clientY))
    if (!moved) return
    setControls(true)
    restartHide()
  }

  // A press that closes the menu does not also go on.
  const swallowClick = useRef(false)
  useEffect(() => {
    const down = () => {
      swallowClick.current = menuOpen.current
    }
    window.addEventListener('pointerdown', down, true)
    return () => window.removeEventListener('pointerdown', down, true)
  }, [])
  const openMenu = (event: React.MouseEvent) => {
    event.preventDefault()
    menuOpen.current = true
    setMenu({ x: event.clientX, y: event.clientY })
  }
  const onClick = (event: React.MouseEvent) => {
    if (event.button !== 0) return
    if (swallowClick.current) {
      swallowClick.current = false
      return
    }
    advance()
  }

  const atEnd = index >= slides
  const model: MenuModel = {
    bar: [],
    entries: [
      { kind: 'item', id: 'next', label: t('show.next'), icon: ChevronRightIcon, shortcut: 'N', disabled: atEnd, run: advance },
      { kind: 'item', id: 'previous', label: t('show.previous'), icon: ChevronLeftIcon, shortcut: 'P', disabled: index === 0 && !screen, run: back },
      { kind: 'item', id: 'first', label: t('show.first'), shortcut: 'Home', run: () => go(0) },
      { kind: 'item', id: 'last', label: t('show.last'), shortcut: 'End', run: () => go(slides - 1) },
      { kind: 'separator' },
      {
        kind: 'submenu',
        id: 'goTo',
        label: t('show.goTo'),
        entries: Array.from({ length: slides }, (_, at): MenuEntry => ({
          kind: 'item',
          id: `goTo-${at}`,
          label: t('link.slideItem', { slide: at + 1, title: facts.slides[at]?.title?.trim() || t('link.untitled') }),
          checked: at === index ? true : undefined,
          run: () => go(at),
        })),
      },
      {
        kind: 'submenu',
        id: 'screen',
        label: t('show.screen'),
        entries: [
          { kind: 'item', id: 'black', label: t('show.black'), shortcut: 'B', checked: screen === 'black' ? true : undefined, run: () => blank('black') },
          { kind: 'item', id: 'white', label: t('show.white'), shortcut: 'W', checked: screen === 'white' ? true : undefined, run: () => blank('white') },
        ],
      },
      { kind: 'separator' },
      { kind: 'item', id: 'end', label: t('show.end'), shortcut: 'Esc', run: end },
    ],
  }

  const shown = always || controls
  return (
    <div
      ref={stage}
      className="slides-show"
      data-reading={options.reading}
      data-presenter={options.presenter}
      data-slide={atEnd ? 'end' : index}
      data-screen={screen ?? undefined}
      data-pointer={shown || menu ? 'shown' : 'hidden'}
      onClick={onClick}
      onContextMenu={openMenu}
      onPointerMove={onPointerMove}
      data-testid="slides-show"
      title={hint}
    >
      <canvas ref={canvas} className="slides-show-slide" hidden={atEnd} />
      {atEnd ? <div className="slides-show-ended" data-testid="slides-show-ended">{t('show.ended')}</div> : null}
      {options.presenter ? <aside className="slides-presenter"><h2>{t('nextSlide')}</h2><canvas ref={nextCanvas} hidden={index + 1 >= slides} /><h2>{t('speakerNotes')}</h2><p>{notes || t('noNotes')}</p></aside> : null}
      {screen ? <div className="slides-show-screen" data-testid="slides-show-screen" /> : null}
      {typed ? <div className="slides-show-typed" data-testid="slides-show-typed">{t('show.typed', { slide: typed })}</div> : null}
      <div
        className="slides-show-controls"
        data-shown={shown}
        data-testid="slides-show-controls"
        onClick={(event) => event.stopPropagation()}
        onContextMenu={(event) => event.stopPropagation()}
        onPointerEnter={() => { onBar.current = true }}
        onPointerLeave={() => {
          onBar.current = false
          if (!always) restartHide()
        }}
      >
        <span>{atEnd ? t('show.endedShort') : t('slideOf', { slide: index + 1, slides })}</span>
        <button type="button" className="slides-show-control" aria-label={t('show.previous')} title={t('show.previous')} data-testid="slides-show-previous" onClick={back} disabled={index === 0 && !screen}><ChevronLeftIcon size={16} aria-hidden="true" /></button>
        <button type="button" className="slides-show-control" aria-label={t('show.next')} title={t('show.next')} data-testid="slides-show-next" onClick={advance}><ChevronRightIcon size={16} aria-hidden="true" /></button>
        <button type="button" className="slides-show-control" aria-label={t('show.end')} title={t('show.end')} data-testid="slides-show-end" onClick={end}><XIcon size={16} aria-hidden="true" /></button>
      </div>
      {menu ? (
        <div onClick={(event) => event.stopPropagation()} onContextMenu={(event) => { event.preventDefault(); event.stopPropagation() }}>
          <CanvasMenu at={menu} model={model} label={t('show.menu')} onClose={closeMenu} testId="slides-show-menu" rowIds="slides-show-cm-" />
        </div>
      ) : null}
    </div>
  )
}
