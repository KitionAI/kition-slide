/**
 * What the slide's context menus hold (plan stage 6A, walkthrough C11, G9): the rows WPS shows for
 * a shape, a picture, text being edited, a table's cell and the empty slide (W-J4, W-J9, W-J5,
 * W-J8, W-J6), in its order, each one of the engine's commands or one of the editor's own views.
 * A row WPS has and this editor has no command for is left out, not shown greyed. Every menu
 * starts with a row of icons: Copy, Cut, Paste and Paste Text Only (W-J3). The engine decides what
 * the menu is for (`contextMenu`, the right-click's own selection); this file only lists the rows.
 */
import {
  ArrowDownIcon,
  ArrowUpIcon,
  BetweenHorizontalEndIcon,
  BetweenHorizontalStartIcon,
  BetweenVerticalEndIcon,
  BetweenVerticalStartIcon,
  BringToFrontIcon,
  ClipboardPasteIcon,
  CopyIcon,
  Grid3x3Icon,
  GroupIcon,
  ImageUpIcon,
  LayoutTemplateIcon,
  LinkIcon,
  ListIcon,
  LogOutIcon,
  MessageSquarePlusIcon,
  PaintBucketIcon,
  PinIcon,
  RotateCcwIcon,
  RulerIcon,
  ScissorsIcon,
  SendToBackIcon,
  ShapesIcon,
  SlidersHorizontalIcon,
  SquareDashedMousePointerIcon,
  TableCellsMergeIcon,
  TableCellsSplitIcon,
  TextCursorInputIcon,
  TrashIcon,
  TypeIcon,
  UngroupIcon,
} from '@kition/tessera/icons/react'
import type { IconComponent } from '@kition/sdk/editor'

/** What the engine said the menu is for (`contextMenu`): `at` is a canvas point. */
export type ContextTarget = {
  menu: 'text' | 'cell' | 'objects' | 'slide'
  ids: number[]
  kinds: string[]
  at: [number, number]
}

/** One choice. `pair` is a second, smaller choice on the same row (Group with Ungroup beside it). */
export type MenuAction = {
  id: string
  label: string
  icon?: IconComponent
  shortcut?: string | null
  checked?: boolean
  disabled?: boolean
  run: () => void
}

export type MenuEntry =
  | ({ kind: 'item'; pair?: MenuAction } & MenuAction)
  | { kind: 'submenu'; id: string; label: string; icon?: IconComponent; disabled?: boolean; entries: MenuEntry[] }
  | { kind: 'separator' }
  /** Something other than rows (a grid of colors); `pick` closes the menu, then acts. */
  | { kind: 'panel'; id: string; render: (pick: (act: () => void) => void) => unknown }

export type MenuModel = { bar: MenuAction[]; entries: MenuEntry[] }

/** What the rows need from the editor. */
export type MenuSources = {
  t: (key: string, values?: Record<string, string | number>) => string
  enabled: (id: string) => boolean
  /** Runs a command with these parameters (never through a parameter form). */
  run: (id: string, params?: Record<string, unknown>) => void
  /** `Cmd+K` as the platform writes it. */
  linkShortcut: string
  link: () => void
  formatObject: () => void
  changePicture: (() => void) | null
  newComment: (() => void) | null
  layouts: Array<{ id: number; name: string }>
  format: { bullets?: boolean; numbering?: boolean }
  view: {
    ruler: boolean
    gridlines: boolean
    guides: boolean
    setRuler: (on: boolean) => void
    setGridlines: (on: boolean) => void
    setGuides: (on: boolean) => void
    grid: { snapToGrid: boolean; smartGuides: boolean }
    setGrid: (change: { snapToGrid?: boolean; smartGuides?: boolean }) => void
    setGridDialog: (open: boolean) => void
  }
  /** The table whose cell the menu is for. */
  table: { id: number; canMerge: boolean; canSplit: boolean } | null
  /** The slide's backgrounds (a color grid), for Background's submenu. */
  backgrounds: (pick: (color: string) => void) => unknown
}

/** The kinds a shape's own rows (Edit Text, Change Shape, Set as Default Shape) are for. */
const TEXT_SHAPES = ['shape', 'textBox', 'placeholder']

/** The rows of the menu for `target`. */
export function menuModel(target: ContextTarget, from: MenuSources): MenuModel {
  const { t, enabled, run } = from
  const action = (id: string, key: string, icon: IconComponent | undefined, command: string, params: Record<string, unknown> = {}, disabled = false): MenuAction => ({
    id,
    label: t(key),
    icon,
    disabled: disabled || !enabled(command),
    run: () => run(command, params),
  })
  const item = (a: MenuAction, pair?: MenuAction): MenuEntry => ({ kind: 'item', ...a, ...(pair ? { pair } : {}) })
  const separator: MenuEntry = { kind: 'separator' }
  const link: MenuEntry = { kind: 'item', id: 'link', label: t('cm.link'), icon: LinkIcon, shortcut: from.linkShortcut, disabled: !enabled('link.insert'), run: from.link }
  const formatObject: MenuEntry = { kind: 'item', id: 'formatObject', label: t('cm.formatObject'), icon: SlidersHorizontalIcon, run: from.formatObject }

  // On the empty slide there is nothing to copy or cut.
  const nothing = target.menu === 'slide'
  const bar: MenuAction[] = [
    action('copy', 'cm.copy', CopyIcon, 'edit.copy', {}, nothing),
    action('cut', 'cm.cut', ScissorsIcon, 'edit.cut', {}, nothing),
    action('paste', 'cm.paste', ClipboardPasteIcon, 'edit.paste'),
    action('pasteText', 'cm.pasteText', TypeIcon, 'edit.pasteText'),
  ]

  if (target.menu === 'text') {
    return {
      bar,
      entries: [
        {
          kind: 'submenu',
          id: 'bulletsNumbering',
          label: t('cm.bulletsNumbering'),
          icon: ListIcon,
          entries: [
            { ...action('bullets', 'cm.bullets', undefined, 'format.bullets'), kind: 'item', checked: from.format.bullets === true },
            { ...action('numbering', 'cm.numbering', undefined, 'format.numbering'), kind: 'item', checked: from.format.numbering === true },
          ],
        },
        item(action('exitText', 'cm.exitText', LogOutIcon, 'text.exit')),
        separator,
        link,
        formatObject,
      ],
    }
  }

  if (target.menu === 'cell') {
    const table = from.table
    const id = table ? { id: table.id } : {}
    const cell = (name: string, key: string, icon: IconComponent, command: string, disabled = false) => action(name, key, icon, command, id, disabled || !table)
    return {
      bar,
      entries: [
        {
          kind: 'submenu',
          id: 'insert',
          label: t('cm.insert'),
          entries: [
            item(cell('insertRowAbove', 'cm.insertRowAbove', BetweenHorizontalStartIcon, 'table.insertRowAbove')),
            item(cell('insertRowBelow', 'cm.insertRowBelow', BetweenHorizontalEndIcon, 'table.insertRowBelow')),
            item(cell('insertColumnLeft', 'cm.insertColumnLeft', BetweenVerticalStartIcon, 'table.insertColumnLeft')),
            item(cell('insertColumnRight', 'cm.insertColumnRight', BetweenVerticalEndIcon, 'table.insertColumnRight')),
          ],
        },
        item(cell('deleteRows', 'cm.deleteRows', TrashIcon, 'table.deleteRow')),
        item(cell('deleteColumns', 'cm.deleteColumns', TrashIcon, 'table.deleteColumn')),
        item(cell('mergeCells', 'cm.mergeCells', TableCellsMergeIcon, 'table.merge', !table?.canMerge)),
        item(cell('splitCells', 'cm.splitCells', TableCellsSplitIcon, 'table.split', !table?.canSplit)),
        separator,
        link,
        item({ ...action('selectTable', 'cm.selectTable', SquareDashedMousePointerIcon, 'edit.select', table ? { ids: [table.id] } : {}, !table) }),
      ],
    }
  }

  if (target.menu === 'objects') {
    const kinds = target.kinds
    const single = target.ids.length === 1
    const several = target.ids.length > 1
    const pictures = kinds.length > 0 && kinds.every((kind) => kind === 'picture')
    const shapes = kinds.length > 0 && kinds.every((kind) => TEXT_SHAPES.includes(kind))
    const group = action('group', 'cm.group', GroupIcon, 'arrange.group', {}, !several)
    const ungroup = action('ungroup', 'cm.ungroup', UngroupIcon, 'arrange.ungroup', {}, !kinds.includes('group'))
    const front = action('bringToFront', 'cm.bringToFront', BringToFrontIcon, 'arrange.bringToFront')
    const forward = action('bringForward', 'cm.bringForward', ArrowUpIcon, 'arrange.bringForward')
    const back = action('sendToBack', 'cm.sendToBack', SendToBackIcon, 'arrange.sendToBack')
    const backward = action('sendBackward', 'cm.sendBackward', ArrowDownIcon, 'arrange.sendBackward')
    const entries: MenuEntry[] = [item(action('delete', 'cm.delete', TrashIcon, 'edit.delete'))]
    // A picture's menu has the one-step rows alone (W-J9); a shape's has the other beside each (W-J4).
    if (pictures) entries.push(item(group), item(front), item(back))
    else entries.push(item(group, ungroup), item(front, forward), item(back, backward))
    entries.push(separator)
    if (shapes && single) entries.push(item(action('editText', 'cm.editText', TextCursorInputIcon, 'text.edit', { all: true })))
    if (shapes) {
      entries.push({
        kind: 'submenu',
        id: 'changeShape',
        label: t('cm.changeShape'),
        icon: ShapesIcon,
        disabled: !enabled('shape.change'),
        entries: CHANGE_TO.map((preset) => item(action(`changeShape-${preset}`, `shape.${preset}`, undefined, 'shape.change', { preset }))),
      })
    }
    if (pictures && single && from.changePicture) {
      entries.push({ kind: 'item', id: 'changePicture', label: t('cm.changePicture'), icon: ImageUpIcon, disabled: !enabled('image.change'), run: from.changePicture })
    }
    entries.push(link)
    if (shapes && single) entries.push(item(action('setDefault', 'cm.setDefault', PinIcon, 'shape.setDefault')))
    entries.push(formatObject)
    return { bar, entries }
  }

  const view = from.view
  const entries: MenuEntry[] = [
    {
      kind: 'submenu',
      id: 'background',
      label: t('cm.background'),
      icon: PaintBucketIcon,
      disabled: !enabled('slide.background'),
      entries: [
        item(action('resetBackground', 'cm.resetBackground', RotateCcwIcon, 'slide.background', { reset: true })),
        { kind: 'panel', id: 'backgrounds', render: (pick) => from.backgrounds((color) => pick(() => run('slide.background', { color }))) },
      ],
    },
    {
      kind: 'submenu',
      id: 'layout',
      label: t('cm.layout'),
      icon: LayoutTemplateIcon,
      disabled: !enabled('slide.layout') || from.layouts.length === 0,
      entries: from.layouts.map((layout) => ({ kind: 'item', id: `layout-${layout.id}`, label: layout.name, run: () => run('slide.layout', { layout: layout.name }) })),
    },
    item(action('reset', 'cm.reset', RotateCcwIcon, 'slide.reset')),
    separator,
    {
      kind: 'submenu',
      id: 'gridGuides',
      label: t('cm.gridGuides'),
      icon: Grid3x3Icon,
      entries: [
        { kind: 'item', id: 'gridlines', label: t('cm.gridlines'), checked: view.gridlines, run: () => view.setGridlines(!view.gridlines) },
        { kind: 'item', id: 'guides', label: t('cm.guides'), checked: view.guides, run: () => view.setGuides(!view.guides) },
        { kind: 'item', id: 'smartGuides', label: t('cm.smartGuides'), checked: view.grid.smartGuides, run: () => view.setGrid({ smartGuides: !view.grid.smartGuides }) },
        { kind: 'item', id: 'snapToGrid', label: t('cm.snapToGrid'), checked: view.grid.snapToGrid, run: () => view.setGrid({ snapToGrid: !view.grid.snapToGrid }) },
        separator,
        { kind: 'item', id: 'gridSettings', label: t('cm.gridSettings'), run: () => view.setGridDialog(true) },
      ],
    },
    { kind: 'item', id: 'ruler', label: t('cm.ruler'), icon: RulerIcon, checked: view.ruler, run: () => view.setRuler(!view.ruler) },
  ]
  if (from.newComment) entries.push(separator, { kind: 'item', id: 'newComment', label: t('cm.newComment'), icon: MessageSquarePlusIcon, run: from.newComment })
  return { bar, entries }
}

/** The shapes Change Shape offers, as Shape Format's Change Shape does. */
export const CHANGE_TO = ['rect', 'roundRect', 'ellipse', 'triangle', 'rtTriangle', 'parallelogram', 'trapezoid', 'diamond', 'pentagon', 'hexagon', 'octagon', 'star5', 'rightArrow', 'leftArrow', 'upArrow', 'downArrow', 'heart', 'cloud']
