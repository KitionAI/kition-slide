/**
 * The slide's context menu (plan stage 6A, walkthrough C11, G9): opened by a right-click on the
 * slide (Ctrl+click on the Mac), Shift+F10 or the menu key, at the pointer (else at the selection).
 * The engine has chosen what it is for by then; its rows are `contextMenus.ts`'s. It stays while the
 * pointer leaves it, closes on Escape, a press outside it, a scroll, the window resizing or losing
 * the focus, and on a choice, and keeps inside the window. A submenu opens on hover, a click, Right,
 * Enter or Space, and closes on Left. When it closes the slide has the keyboard again: the text
 * being edited, else the editor's frame (`onClose`), before a choice runs.
 */
import { createContext, useContext, useEffect, useLayoutEffect, useRef, useState, type KeyboardEvent, type ReactNode } from 'react'
import { CheckIcon, ChevronRightIcon } from '@kition/tessera/icons/react'
import { isMac, menuKeyDown, type IconComponent } from '@kition/sdk/editor'
import { useSlides } from './slides'
import { usePicturePick } from './ribbon/HomeTab'
import { ColorGrid } from './ribbon/common'
import { menuModel, type ContextTarget, type MenuAction, type MenuEntry, type MenuModel } from './contextMenus'

/** An open menu: what the engine said it is for, and where on the page it opens. */
export type OpenMenu = ContextTarget & { client: { x: number; y: number } }

/** Where a menu at `x`, `y` of `width` × `height` stays inside the window. */
function inside(x: number, y: number, width: number, height: number) {
  return { left: Math.max(4, Math.min(x, window.innerWidth - width - 4)), top: Math.max(4, Math.min(y, window.innerHeight - height - 4)) }
}

function Glyph({ icon: Icon, checked }: { icon?: IconComponent; checked?: boolean }) {
  if (checked) return <CheckIcon size={15} strokeWidth={1.75} aria-hidden="true" />
  if (Icon) return <Icon size={15} strokeWidth={1.75} aria-hidden="true" />
  return <span style={{ width: 15 }} />
}

type Pick = (act: () => void) => void

/** The start of a menu's row test ids: the slide's menu has `slides-cm-`, the slide panel's its own. */
const RowIds = createContext('slides-cm-')

/** One row's button: a choice. */
function Row({ action, pick, onEnter, compact = false }: { action: MenuAction; pick: Pick; onEnter: () => void; compact?: boolean }) {
  const ids = useContext(RowIds)
  return (
    <button
      type="button"
      role={action.checked === undefined ? 'menuitem' : 'menuitemcheckbox'}
      aria-checked={action.checked}
      aria-label={compact ? action.label : undefined}
      title={compact ? action.label : undefined}
      className={compact ? 'kui-menu-item slides-cm-icon' : 'kui-menu-item'}
      disabled={action.disabled}
      data-testid={`${ids}${action.id}`}
      onPointerEnter={onEnter}
      onClick={() => pick(action.run)}
    >
      <Glyph icon={action.icon} checked={action.checked} />
      {compact ? null : <span>{action.label}</span>}
      {!compact && action.shortcut ? <kbd>{action.shortcut}</kbd> : null}
    </button>
  )
}

/** One level of the menu: its rows, and the submenu open from one of them. */
function Level({ entries, bar, pick, label, testId, style, levelRef }: { entries: MenuEntry[]; bar?: MenuAction[]; pick: Pick; label?: string; testId?: string; style: React.CSSProperties; levelRef?: React.Ref<HTMLDivElement> }) {
  const ids = useContext(RowIds)
  const [open, setOpen] = useState<string | null>(null)
  const rows = useRef(new Map<string, HTMLButtonElement>())
  const [place, setPlace] = useState<{ left: number; top: number } | null>(null)
  const sub = useRef<HTMLDivElement>(null)
  const focusSub = useRef(false)
  // A submenu opens beside its row, inside the window (on the left when the right has no room).
  useLayoutEffect(() => {
    const row = open ? rows.current.get(open) : null
    const element = sub.current
    if (!row || !element) {
      setPlace(null)
      return
    }
    const box = row.getBoundingClientRect()
    const width = element.offsetWidth, height = element.offsetHeight
    const left = box.right + 2 + width > window.innerWidth - 4 ? box.left - width - 2 : box.right + 2
    setPlace(inside(left, box.top - 4, width, height))
  }, [open])
  // Its first row takes the keyboard once it shows (a hidden row cannot be focused).
  useLayoutEffect(() => {
    if (!place || !focusSub.current) return
    focusSub.current = false
    sub.current?.querySelector<HTMLElement>('[role^="menuitem"]:not(:disabled)')?.focus({ preventScroll: true })
  }, [place])
  const openSub = (id: string, focus: boolean) => {
    focusSub.current = focus
    setOpen(id)
  }
  const keyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const target = event.target as HTMLElement
    const id = target.dataset.submenu
    if (id && (event.key === 'ArrowRight' || event.key === 'Enter' || event.key === ' ')) {
      event.preventDefault()
      event.stopPropagation()
      openSub(id, true)
      return
    }
    // Left in a submenu goes on to the level above, which has its row (`onSubKey`).
    menuKeyDown(event)
  }
  const onSubKey = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key !== 'ArrowLeft' || !open) return
    event.preventDefault()
    event.stopPropagation()
    const row = rows.current.get(open)
    setOpen(null)
    row?.focus({ preventScroll: true })
  }
  const enter = () => setOpen(null)
  const entry = (item: MenuEntry, index: number): ReactNode => {
    switch (item.kind) {
      case 'separator':
        return <div key={`separator-${index}`} className="kui-menu-separator" role="separator" />
      case 'panel':
        return <div key={item.id} onPointerEnter={enter}>{item.render(pick) as ReactNode}</div>
      case 'submenu':
        return (
          <button
            key={item.id}
            ref={(element) => {
              if (element) rows.current.set(item.id, element)
              else rows.current.delete(item.id)
            }}
            type="button"
            role="menuitem"
            aria-haspopup="menu"
            aria-expanded={open === item.id}
            className="kui-menu-item"
            disabled={item.disabled || item.entries.length === 0}
            data-submenu={item.id}
            data-testid={`${ids}${item.id}`}
            onPointerEnter={() => openSub(item.id, false)}
            onClick={() => (open === item.id ? setOpen(null) : openSub(item.id, true))}
          >
            <Glyph icon={item.icon} />
            <span>{item.label}</span>
            <ChevronRightIcon size={14} strokeWidth={1.75} aria-hidden="true" style={{ marginLeft: 'auto' }} />
          </button>
        )
      case 'item':
        if (!item.pair) return <Row key={item.id} action={item} pick={pick} onEnter={enter} />
        return (
          <div key={item.id} className="slides-cm-pair">
            <Row action={item} pick={pick} onEnter={enter} />
            <Row action={item.pair} pick={pick} onEnter={enter} compact />
          </div>
        )
    }
  }
  const opened = entries.find((item): item is Extract<MenuEntry, { kind: 'submenu' }> => item.kind === 'submenu' && item.id === open)
  return (
    <div ref={levelRef} className="kui-popover slides-cm" role="menu" aria-label={label} data-testid={testId} style={style} onKeyDown={keyDown}>
      {bar ? (
        <div className="slides-cm-bar" onPointerEnter={enter}>
          {bar.map((action) => <Row key={action.id} action={action} pick={pick} onEnter={enter} compact />)}
        </div>
      ) : null}
      {bar ? <div className="kui-menu-separator" role="separator" /> : null}
      {entries.map(entry)}
      {opened ? (
        <div onKeyDown={onSubKey}>
          <Level entries={opened.entries} pick={pick} testId={`${ids}${opened.id}-menu`} style={place ? { left: place.left, top: place.top } : { left: 0, top: 0, visibility: 'hidden' }} levelRef={sub} />
        </div>
      ) : null}
    </div>
  )
}

/**
 * A menu of `model`'s rows at page point `at`. `testId` names it and `rowIds` starts its rows' test
 * ids (the slide's menu by default; the slide panel's menu has its own).
 */
export function CanvasMenu({ at, model, label, onClose, testId = 'slides-context-menu', rowIds = 'slides-cm-' }: { at: { x: number; y: number }; model: MenuModel; label: string; onClose: () => void; testId?: string; rowIds?: string }) {
  const box = useRef<HTMLDivElement>(null)
  const [place, setPlace] = useState<{ left: number; top: number } | null>(null)
  const latest = useRef(onClose)
  latest.current = onClose
  useLayoutEffect(() => {
    const element = box.current
    if (!element) return
    setPlace(inside(at.x, at.y, element.offsetWidth, element.offsetHeight))
  }, [at.x, at.y])
  // The first row takes the keyboard once the menu shows (a hidden row cannot be focused).
  const shown = place !== null
  useLayoutEffect(() => {
    if (shown) box.current?.querySelector<HTMLElement>('[role^="menuitem"]:not(:disabled)')?.focus({ preventScroll: true })
  }, [shown])
  // Once closing, the keyboard goes back to the slide and the menu no longer keeps it.
  const closed = useRef(false)
  const lastRow = useRef<HTMLElement | null>(null)
  useLayoutEffect(() => {
    const close = () => {
      closed.current = true
      latest.current()
    }
    const within = (target: EventTarget | null) => target instanceof Node && box.current?.contains(target) === true
    const down = (event: PointerEvent) => {
      if (!within(event.target)) close()
    }
    const key = (event: globalThis.KeyboardEvent) => {
      if (event.key !== 'Escape') return
      event.preventDefault()
      event.stopPropagation()
      close()
    }
    const scroll = (event: Event) => {
      if (!within(event.target)) close()
    }
    // While a text shape is selected the editor's text field takes the keyboard back after each
    // pointer release; the menu keeps it while open, so its keys are not typed on the slide.
    const focusIn = (event: FocusEvent) => {
      if (closed.current) return
      if (within(event.target)) {
        lastRow.current = event.target as HTMLElement
        return
      }
      if (!(event.target instanceof Element) || !event.target.classList.contains('kui-text-sink')) return
      const back = lastRow.current?.isConnected ? lastRow.current : box.current?.querySelector<HTMLElement>('[role^="menuitem"]:not(:disabled)')
      back?.focus({ preventScroll: true })
    }
    document.addEventListener('focusin', focusIn, true)
    window.addEventListener('pointerdown', down, true)
    window.addEventListener('keydown', key, true)
    window.addEventListener('wheel', scroll, true)
    window.addEventListener('resize', close)
    window.addEventListener('blur', close)
    return () => {
      document.removeEventListener('focusin', focusIn, true)
      window.removeEventListener('pointerdown', down, true)
      window.removeEventListener('keydown', key, true)
      window.removeEventListener('wheel', scroll, true)
      window.removeEventListener('resize', close)
      window.removeEventListener('blur', close)
    }
  }, [])
  // A choice closes the menu first (the slide has the keyboard again), then acts.
  const pick: Pick = (act) => {
    closed.current = true
    latest.current()
    act()
  }
  return (
    <RowIds.Provider value={rowIds}>
    <div
      onContextMenu={(event) => event.preventDefault()}
      // Keys in the menu are the menu's, never the editor's shortcuts or the tool's.
      onKeyDown={(event) => event.stopPropagation()}
    >
      <Level
        levelRef={box}
        entries={model.entries}
        bar={model.bar}
        pick={pick}
        label={label}
        testId={testId}
        style={place ? { left: place.left, top: place.top } : { left: at.x, top: at.y, visibility: 'hidden' }}
      />
    </div>
    </RowIds.Provider>
  )
}

type TableFacts = { id: number; canMerge: boolean; canSplit: boolean }

/** The context menu for `menu`, with the editor's commands and views behind its rows. */
export function SlideContextMenu({ menu, onClose, onFormat }: { menu: OpenMenu; onClose: () => void; onFormat: (target: ContextTarget) => void }) {
  const slides = useSlides()
  const { t, enabled, query, facts, format, view } = slides
  const picture = usePicturePick()
  const [table, setTable] = useState<TableFacts | null>(null)
  useEffect(() => {
    if (menu.menu !== 'cell') return
    let stale = false
    void query<TableFacts>('table.state').then((state) => {
      if (!stale && state && typeof state.id === 'number') setTable({ id: state.id, canMerge: state.canMerge === true, canSplit: state.canSplit === true })
    })
    return () => {
      stale = true
    }
  }, [menu, query])
  const model = menuModel(menu, {
    t,
    enabled,
    run: (id, params) => void slides.run(id, params ?? {}),
    linkShortcut: isMac() ? '⌘K' : 'Ctrl+K',
    link: () => slides.setLinking(true),
    formatObject: () => onFormat(menu),
    changePicture: picture.can
      ? () => {
          void picture.pick().then((picked) => {
            if (picked) void slides.run('image.change', picked)
          })
        }
      : null,
    newComment: slides.canNewComment ? () => void slides.newComment() : null,
    layouts: facts.layouts,
    format,
    view,
    table,
    backgrounds: (pick) => <ColorGrid onPick={pick} testId="slides-cm-backgrounds" />,
  })
  return <CanvasMenu at={menu.client} model={model} label={t('cm.label')} onClose={onClose} />
}
