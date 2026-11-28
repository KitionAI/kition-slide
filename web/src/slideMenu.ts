/**
 * The slide panel's context menu (plan stage 7, walkthrough B6; WPS W-J7): Copy, Cut and Paste on
 * top as WPS's menus have them (W-J3), then New Slide, Duplicate Slide, Delete Slide, Hide Slide,
 * Add Section and Rename Section, Layout ▸ and Reset Slide. Every row acts on the slides selected
 * in the panel (the slide right-clicked, unless it is one of several selected), each one command
 * and one undo step. A row WPS has and this editor has no command for is left out.
 */
import { ClipboardPasteIcon, CopyIcon, EyeOffIcon, FolderPlusIcon, LayoutTemplateIcon, PencilIcon, PlusIcon, RotateCcwIcon, ScissorsIcon, TrashIcon } from '@kition/tessera/icons/react'
import type { MenuAction, MenuEntry, MenuModel } from './contextMenus'

export type SlideMenuSources = {
  t: (key: string, values?: Record<string, string | number>) => string
  enabled: (id: string) => boolean
  /** Runs a command with these parameters, or without any (the clipboard's commands). */
  run: (id: string, params?: Record<string, unknown>) => void
  readOnly: boolean
  /** The slide the menu is for, and whether it is one of several selected (the rows act on all of them). */
  index: number
  several: boolean
  /** Whether every slide the rows act on is hidden. */
  hidden: boolean
  /** The section the slide is in (its index), or null when the deck has none. */
  section: number | null
  layouts: Array<{ id: number; name: string }>
  /** A new section starts at the slide: the panel names it next. */
  addSection: () => void
  /** The slide's section gets a new name in the panel. */
  renameSection: (section: number) => void
}

export function slideMenuModel(from: SlideMenuSources): MenuModel {
  const { t, enabled, run, readOnly } = from
  /** The rows' parameters: the slide right-clicked, or the selected slides when it is one of several. */
  const which = from.several ? {} : { index: from.index }
  const action = (id: string, key: string, icon: MenuAction['icon'], command: string, params: Record<string, unknown> | undefined, edits = true): MenuAction => ({
    id,
    label: t(key),
    icon,
    disabled: (edits && readOnly) || !enabled(command),
    run: () => run(command, params),
  })
  const item = (a: MenuAction): MenuEntry => ({ kind: 'item', ...a })
  const bar: MenuAction[] = [
    action('copy', 'cm.copy', CopyIcon, 'edit.copy', undefined, false),
    action('cut', 'cm.cut', ScissorsIcon, 'edit.cut', undefined),
    action('paste', 'cm.paste', ClipboardPasteIcon, 'edit.paste', undefined),
  ]
  const entries: MenuEntry[] = [
    item(action('new', 'sp.newSlide', PlusIcon, 'slide.new', { at: from.index + 1 })),
    item(action('duplicate', 'sp.duplicate', CopyIcon, 'slide.duplicate', which)),
    item(action('delete', 'sp.delete', TrashIcon, 'slide.delete', which)),
    { kind: 'item', ...action('hide', 'sp.hide', EyeOffIcon, 'slide.hide', { ...which, hidden: !from.hidden }), checked: from.hidden },
    { kind: 'separator' },
    { kind: 'item', id: 'addSection', label: t('sp.addSection'), icon: FolderPlusIcon, disabled: readOnly || !enabled('section.add'), run: from.addSection },
  ]
  if (from.section != null) {
    const section = from.section
    entries.push({ kind: 'item', id: 'renameSection', label: t('sp.renameSection'), icon: PencilIcon, disabled: readOnly || !enabled('section.rename'), run: () => from.renameSection(section) })
  }
  entries.push(
    { kind: 'separator' },
    {
      kind: 'submenu',
      id: 'layout',
      label: t('cm.layout'),
      icon: LayoutTemplateIcon,
      disabled: readOnly || !enabled('slide.layout') || from.layouts.length === 0,
      entries: from.layouts.map((layout) => ({ kind: 'item', id: `layout-${layout.id}`, label: layout.name, run: () => run('slide.layout', { layout: layout.name, index: from.index }) })),
    },
    item(action('reset', 'cm.reset', RotateCcwIcon, 'slide.reset', { index: from.index })),
  )
  return { bar, entries }
}
