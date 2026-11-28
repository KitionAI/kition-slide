/**
 * The floating toolbars (plan stage 6B; walkthrough G9; WPS W-J1, W-A13). Words selected with the
 * mouse (a drag through them, a double click on a word, a triple click) bring a small two-row bar
 * up just above them: font, size, bold, italic, underline, font color, left, centre and right
 * alignment, bullets. Two or more objects selected bring the align bar above them: the six aligns,
 * the two distributes and Group. A single object has none (WPS's column of buttons beside one
 * object is left out).
 *
 * The engine says which bar the selection has and where it is (`floatingBar`), in the input order
 * after the press that made the selection. A bar never takes the keyboard: a press on it keeps the
 * focus where it was (the text being edited keeps its caret and its selection), so the next key
 * types into the text. It goes away on a key that types or moves (not on a shortcut with Cmd or
 * Ctrl), an input method starting, a scroll or zoom of the slide, a press elsewhere, and Escape
 * (which goes on to the slide, as without the bar); and when the selection no longer has it.
 */
import { useCallback, useEffect, useLayoutEffect, useRef, useState, type ReactNode, type RefObject } from 'react'
import {
  BaselineIcon,
  BoldIcon,
  ChevronDownIcon,
  GroupIcon,
  ItalicIcon,
  ListIcon,
  TextAlignCenterIcon,
  TextAlignEndIcon,
  TextAlignStartIcon,
  UnderlineIcon,
  type IconComponent,
} from '@kition/tessera/icons/react'
import type { RasterView } from '@kition/sdk/editor'
import { useSlides } from './slides'
import { focusEditor } from './focus'
import { ColorPicker } from './ribbon/common'
import { ALIGN_EDGES, DISTRIBUTE, useArrange } from './ribbon/ArrangeMenu'
import { SIZES, fontChoices } from './ribbon/HomeTab'

/** What `floatingBar` answers: the bar's kind and the box it floats over, in canvas points. */
export type FloatingBar = { kind: 'text' | 'objects'; box: [number, number, number, number]; ids?: number[] }

/**
 * The gap between a bar and what it floats over, in pixels: the align bar keeps clear of the
 * rotation handles above the objects (18 px over their top edge).
 */
const GAP = { text: 8, objects: 30 }

/** Keys that only change another key: they leave the bar up. */
const MODIFIERS = new Set(['Shift', 'Control', 'Alt', 'Meta', 'CapsLock', 'Fn', 'OS', 'Hyper', 'Super'])

/** Whether `target` is inside the bar `bar`. */
function inside(bar: RefObject<HTMLElement | null>, target: EventTarget | null) {
  return target instanceof Node && bar.current?.contains(target) === true
}

/** Whether `target` is the canvas's drawing surface (the slide and what is around it). */
function onSurface(target: EventTarget | null) {
  return target instanceof HTMLCanvasElement && target.classList.contains('kui-canvas-surface')
}

/**
 * Which bar shows, kept in step with the selection: `region` is the canvas's region (its presses
 * and double clicks), `view` the view shown (a scroll or zoom puts the bar away). Answers the bar
 * shown, if any, and the ref its element takes (presses on it are not presses elsewhere).
 */
export function useFloatingBar(region: RefObject<HTMLElement | null>, view: RasterView) {
  const { session } = useSlides()
  const [bar, setBar] = useState<FloatingBar | null>(null)
  const element = useRef<HTMLDivElement | null>(null)
  const latest = useRef({ session, bar })
  latest.current = { session, bar }
  /** A press on the slide is down: the selection it makes is asked for when it is let go. */
  const pressing = useRef(false)
  /** Counts the asks: an answer to an older one, or one after the bar went away, is dropped. */
  const asked = useRef(0)
  /**
   * The objects whose align bar a key or a scroll put away: it stays away for them (a nudge of
   * them changes the slides) until another selection, or a press on the slide.
   */
  const dismissed = useRef<string | null>(null)

  /**
   * Asks the engine which bar the selection has, after the input before it. `text`: words just
   * selected with the mouse may bring the text bar up; otherwise only the align bar comes up, and
   * a bar already up stays only while it is the same kind.
   */
  const ask = useCallback((text: boolean) => {
    const { session: now } = latest.current
    const engine = now.doc
    if (!engine) return
    const ticket = ++asked.current
    if (text) dismissed.current = null
    void now
      .inOrder(() => engine.invoke<string>('floatingBar'))
      .then((answer) => {
        if (ticket !== asked.current) return
        const found = JSON.parse(answer) as FloatingBar | null
        const shown = latest.current.bar
        const allowed = found && (found.kind === 'objects' ? String(found.ids) !== dismissed.current : text || shown?.kind === 'text')
        setBar(allowed ? found : null)
      })
      .catch(() => undefined)
  }, [])
  const hide = useCallback(() => {
    asked.current += 1
    setBar(null)
  }, [])
  /** A key, an input method or a scroll puts the bar away (and the align bar for these objects). */
  const dismiss = useCallback(() => {
    const shown = latest.current.bar
    if (shown?.kind === 'objects') dismissed.current = String(shown.ids)
    hide()
  }, [hide])

  // A press on the slide puts the bar away; letting it go asks for the bar the selection has now
  // (after a drag through words, a double or triple click, a Shift+click or a marquee). The canvas
  // answers the press first: the ask waits for the events before it.
  useEffect(() => {
    const down = (event: PointerEvent) => {
      if (inside(element, event.target)) return
      hide()
      pressing.current = event.button === 0 && onSurface(event.target) && region.current?.contains(event.target as Node) === true
    }
    const up = (event: PointerEvent) => {
      if (!pressing.current || event.button !== 0) return
      pressing.current = false
      window.setTimeout(() => ask(true), 0)
    }
    const doubled = (event: MouseEvent) => {
      if (onSurface(event.target) && region.current?.contains(event.target as Node)) window.setTimeout(() => ask(true), 0)
    }
    const key = (event: KeyboardEvent) => {
      if (!latest.current.bar || inside(element, event.target)) return
      if (MODIFIERS.has(event.key) || event.metaKey || event.ctrlKey) return
      dismiss()
    }
    const scroll = (event: Event) => {
      if (latest.current.bar && !inside(element, event.target)) dismiss()
    }
    window.addEventListener('pointerdown', down, true)
    window.addEventListener('pointerup', up, true)
    window.addEventListener('dblclick', doubled, true)
    document.addEventListener('keydown', key, true)
    document.addEventListener('compositionstart', dismiss, true)
    window.addEventListener('wheel', scroll, true)
    window.addEventListener('resize', dismiss)
    return () => {
      window.removeEventListener('pointerdown', down, true)
      window.removeEventListener('pointerup', up, true)
      window.removeEventListener('dblclick', doubled, true)
      document.removeEventListener('keydown', key, true)
      document.removeEventListener('compositionstart', dismiss, true)
      window.removeEventListener('wheel', scroll, true)
      window.removeEventListener('resize', dismiss)
    }
  }, [ask, hide, dismiss, region])

  // The view moved (a scroll, a zoom): the bar goes away.
  const shownView = useRef(view)
  useEffect(() => {
    const before = shownView.current
    shownView.current = view
    if (before.x !== view.x || before.y !== view.y || before.zoom !== view.zoom) dismiss()
  }, [view.x, view.y, view.zoom, dismiss])

  // The selection or the slides changed: a bar up follows them (its box moves, or it goes when the
  // selection no longer has it); several objects selected by other means (Select All, the
  // selection pane) bring the align bar up. Not while a press is down.
  const state = session.state
  const revision = session.change.revision
  useEffect(() => {
    if (pressing.current || !state) return
    if (latest.current.bar || (state.selected >= 2 && !state.editingText)) ask(false)
  }, [state, revision, ask])

  return { bar, element, hide }
}

/** A bar's button: pressed or not, with its name as tooltip. */
function BarButton({ icon: Icon, label, pressed, disabled, onClick, testId, children }: { icon: IconComponent; label: string; pressed?: boolean; disabled?: boolean; onClick: () => void; testId: string; children?: ReactNode }) {
  return (
    <button type="button" className="slides-mini-button" title={label} aria-label={label} aria-pressed={pressed} disabled={disabled} onClick={onClick} data-testid={testId}>
      <Icon size={16} strokeWidth={1.75} aria-hidden="true" />
      {children}
    </button>
  )
}

/** A bar's list (the fonts, the sizes, the colors): open under its button while `open`. */
function BarList({ label, value, width, open, onOpen, disabled, testId, children }: { label: string; value: string; width: number; open: boolean; onOpen: (open: boolean) => void; disabled?: boolean; testId: string; children: ReactNode }) {
  return (
    <div className="slides-mini-field" style={{ width }}>
      <button type="button" className="slides-mini-select" title={label} aria-label={label} aria-haspopup="listbox" aria-expanded={open} disabled={disabled} onClick={() => onOpen(!open)} data-testid={testId}>
        <span>{value}</span>
        <ChevronDownIcon size={12} strokeWidth={1.75} aria-hidden="true" />
      </button>
      {open ? <div className="slides-mini-list" role="listbox" aria-label={label} data-testid={`${testId}-list`}>{children}</div> : null}
    </div>
  )
}

/** A row of a bar's list. */
function ListRow({ label, chosen, onPick, testId }: { label: string; chosen: boolean; onPick: () => void; testId?: string }) {
  return (
    <button type="button" role="option" aria-selected={chosen} className="slides-mini-option" onClick={onPick} data-testid={testId}>
      {label}
    </button>
  )
}

/** The text bar's buttons (WPS W-J1, the plan's set). */
function TextBar({ list, setList }: { list: string | null; setList: (list: string | null) => void }) {
  const { t, run, enabled, format, facts, session } = useSlides()
  const on = enabled('format.bold')
  const align = format.align ?? ''
  const pick = (id: string, params: Record<string, unknown>) => {
    // A color typed in the hex field: the keyboard goes back to the text from there.
    if (document.activeElement instanceof HTMLInputElement && document.activeElement.closest('.slides-mini')) focusEditor(session.state?.editingText === true)
    setList(null)
    void run(id, params)
  }
  const command = (id: string, icon: IconComponent, label: string, pressed?: boolean) => (
    <BarButton key={id} icon={icon} label={t(label)} pressed={pressed} disabled={!enabled(id)} onClick={() => pick(id, {})} testId={`slides-mini-${id.slice('format.'.length)}`} />
  )
  return (
    <>
      <div className="slides-mini-row">
        <BarList label={t('font')} value={format.font ?? ''} width={132} open={list === 'font'} onOpen={(open) => setList(open ? 'font' : null)} disabled={!on} testId="slides-mini-font">
          {fontChoices(facts, format).map((font) => <ListRow key={font} label={font} chosen={font === format.font} onPick={() => pick('format.fontName', { family: font })} />)}
        </BarList>
        <BarList label={t('fontSize')} value={format.size == null ? '' : String(format.size)} width={58} open={list === 'size'} onOpen={(open) => setList(open ? 'size' : null)} disabled={!on} testId="slides-mini-size">
          {SIZES.map((size) => <ListRow key={size} label={String(size)} chosen={size === format.size} onPick={() => pick('format.fontSize', { size })} testId={`slides-mini-size-${size}`} />)}
        </BarList>
      </div>
      <div className="slides-mini-row">
        {command('format.bold', BoldIcon, 'bold', format.bold)}
        {command('format.italic', ItalicIcon, 'italic', format.italic)}
        {command('format.underline', UnderlineIcon, 'underline', format.underline)}
        <div className="slides-mini-field">
          <BarButton icon={BaselineIcon} label={t('fontColor')} disabled={!on} onClick={() => setList(list === 'color' ? null : 'color')} testId="slides-mini-color">
            <ChevronDownIcon size={10} strokeWidth={1.75} aria-hidden="true" />
          </BarButton>
          {list === 'color' ? (
            <div className="slides-mini-list slides-mini-colors" data-testid="slides-mini-color-list">
              <ColorPicker onPick={(color) => pick('format.color', { color })} testId="slides-mini-colors" />
            </div>
          ) : null}
        </div>
        <span className="slides-mini-separator" />
        {command('format.alignLeft', TextAlignStartIcon, 'alignLeft', align === 'l')}
        {command('format.alignCenter', TextAlignCenterIcon, 'alignCenter', align === 'ctr')}
        {command('format.alignRight', TextAlignEndIcon, 'alignRight', align === 'r')}
        <span className="slides-mini-separator" />
        {command('format.bullets', ListIcon, 'bullets', format.bullets)}
      </div>
    </>
  )
}

/** The align bar's buttons (WPS W-A13): six aligns, two distributes, Group. */
function ObjectsBar() {
  const { t } = useSlides()
  const arrange = useArrange()
  return (
    <div className="slides-mini-row">
      {ALIGN_EDGES.map(({ edge, icon }) => <BarButton key={edge} icon={icon} label={t(`align.${edge}`)} disabled={!arrange.can} onClick={() => arrange.align(edge)} testId={`slides-align-bar-${edge}`} />)}
      <span className="slides-mini-separator" />
      {DISTRIBUTE.map(({ dir, icon }) => <BarButton key={dir} icon={icon} label={t(`distribute.${dir}`)} disabled={!arrange.can} onClick={() => arrange.distribute(dir)} testId={`slides-align-bar-distribute-${dir}`} />)}
      <span className="slides-mini-separator" />
      <BarButton icon={GroupIcon} label={t('group')} disabled={!arrange.canGroup} onClick={arrange.group} testId="slides-align-bar-group" />
    </div>
  )
}

/**
 * The bar `bar` over its box, `toClient` placing canvas points on the page: the text bar above
 * the selected words, from their middle rightwards (WPS shows it up and to the right); the align
 * bar centred above the objects. Below them when there is no room above; always inside `region`.
 */
export function MiniToolbar({ bar, element, toClient, region }: { bar: FloatingBar; element: RefObject<HTMLDivElement | null>; toClient: (x: number, y: number) => { x: number; y: number } | null; region: RefObject<HTMLElement | null> }) {
  const { t } = useSlides()
  const [list, setList] = useState<string | null>(null)
  const [place, setPlace] = useState<{ left: number; top: number } | null>(null)
  const [x0, y0, x1, y1] = bar.box
  const a = toClient(x0, y0)
  const b = toClient(x1, y1)
  useLayoutEffect(() => {
    const own = element.current
    const area = region.current?.getBoundingClientRect()
    if (!own || !a || !b || !area) {
      setPlace(null)
      return
    }
    const width = own.offsetWidth, height = own.offsetHeight
    const left = bar.kind === 'text' ? (a.x + b.x) / 2 - 24 : (a.x + b.x) / 2 - width / 2
    const gap = GAP[bar.kind]
    let top = Math.min(a.y, b.y) - gap - height
    if (top < area.top + 4) top = Math.max(a.y, b.y) + gap
    setPlace({
      left: Math.round(Math.max(area.left + 4, Math.min(left, area.right - width - 4))),
      top: Math.round(Math.max(area.top + 4, Math.min(top, area.bottom - height - 4))),
    })
  }, [bar.kind, a?.x, a?.y, b?.x, b?.y])
  useEffect(() => setList(null), [bar.kind])
  // Escape with a list open closes the list alone; otherwise it goes on to the slide.
  useEffect(() => {
    if (!list) return
    const key = (event: KeyboardEvent) => {
      if (event.key !== 'Escape') return
      event.preventDefault()
      event.stopImmediatePropagation()
      setList(null)
    }
    document.addEventListener('keydown', key, true)
    return () => document.removeEventListener('keydown', key, true)
  }, [list])
  const text = bar.kind === 'text'
  return (
    <div
      ref={element}
      className="slides-mini"
      role="toolbar"
      aria-label={t(text ? 'mini.text' : 'mini.objects')}
      data-testid={text ? 'slides-mini' : 'slides-align-bar'}
      style={place ?? { left: 0, top: 0, visibility: 'hidden' }}
      // A press on the bar keeps the keyboard where it is (the caret in the text), except in a field.
      onMouseDown={(event) => {
        if (!(event.target instanceof HTMLInputElement)) event.preventDefault()
      }}
      onContextMenu={(event) => event.preventDefault()}
    >
      {text ? <TextBar list={list} setList={setList} /> : <ObjectsBar />}
    </div>
  )
}
