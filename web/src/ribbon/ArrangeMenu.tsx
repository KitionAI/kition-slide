/**
 * Arrange in place of the parameter forms (plan stage 1; walkthrough G5, D13): align, distribute,
 * order, group, rotate and flip run straight on the selection, as WPS and PowerPoint do. One
 * object aligns to the slide, several to their selection, unless the person chose otherwise for
 * this selection (the choice lasts while the selection does).
 */
import {
  AlignCenterHorizontalIcon,
  AlignCenterVerticalIcon,
  AlignEndHorizontalIcon,
  AlignEndVerticalIcon,
  AlignHorizontalJustifyCenterIcon,
  AlignStartHorizontalIcon,
  AlignStartVerticalIcon,
  AlignVerticalJustifyCenterIcon,
  ArrowDownIcon,
  ArrowUpIcon,
  BringToFrontIcon,
  FlipHorizontalIcon,
  GroupIcon,
  LayersIcon,
  PanelRightIcon,
  RotateCcwIcon,
  RotateCwIcon,
  SendToBackIcon,
  UngroupIcon,
  type IconComponent,
} from '@kition/tessera/icons/react'
import { MenuItem, MenuSeparator, RibbonButton, RibbonMenu } from '@kition/sdk/editor'
import { useState } from 'react'
import { useSlides } from '../slides'

/** Flip Vertical: the horizontal flip's mark turned a quarter. */
export const FlipVerticalIcon: IconComponent = (props) => <FlipHorizontalIcon {...props} style={{ ...props.style, transform: 'rotate(90deg)' }} />

export const ALIGN_EDGES: Array<{ edge: string; icon: IconComponent }> = [
  { edge: 'left', icon: AlignStartVerticalIcon },
  { edge: 'center', icon: AlignCenterVerticalIcon },
  { edge: 'right', icon: AlignEndVerticalIcon },
  { edge: 'top', icon: AlignStartHorizontalIcon },
  { edge: 'middle', icon: AlignCenterHorizontalIcon },
  { edge: 'bottom', icon: AlignEndHorizontalIcon },
]
const DISTRIBUTE: Array<{ dir: string; icon: IconComponent }> = [
  { dir: 'horizontal', icon: AlignHorizontalJustifyCenterIcon },
  { dir: 'vertical', icon: AlignVerticalJustifyCenterIcon },
]
const ORDER: Array<{ command: string; key: string; icon: IconComponent }> = [
  { command: 'arrange.bringToFront', key: 'bringToFront', icon: BringToFrontIcon },
  { command: 'arrange.bringForward', key: 'bringForward', icon: ArrowUpIcon },
  { command: 'arrange.sendBackward', key: 'sendBackward', icon: ArrowDownIcon },
  { command: 'arrange.sendToBack', key: 'sendToBack', icon: SendToBackIcon },
]
const TURNS: Array<{ command: string; key: string; icon: IconComponent }> = [
  { command: 'arrange.rotateRight', key: 'rotateRight', icon: RotateCwIcon },
  { command: 'arrange.rotateLeft', key: 'rotateLeft', icon: RotateCcwIcon },
  { command: 'arrange.flipVertical', key: 'flipVertical', icon: FlipVerticalIcon },
  { command: 'arrange.flipHorizontal', key: 'flipHorizontal', icon: FlipHorizontalIcon },
]

/** What the person chose to align to, and for which selection (it lapses when the selection changes). */
let alignChoice: { selection: string; to: 'slide' | 'selection' } | null = null

/** The arrange actions on the current selection, each one engine command with its parameters. */
export function useArrange() {
  const { run, enabled, facts, session } = useSlides()
  const ids = facts.selection?.shapes ?? []
  const count = ids.length || (session.state?.selected ?? 0)
  const key = ids.join(',')
  const [, redraw] = useState(0)
  const chosen = alignChoice && alignChoice.selection === key ? alignChoice.to : null
  /** One object goes to the slide; several to their selection (R-D41), unless chosen otherwise. */
  const to: 'slide' | 'selection' = chosen ?? (count < 2 ? 'slide' : 'selection')
  const kinds = session.state?.selectedKinds ?? []
  return {
    count,
    to,
    can: enabled('arrange.align'),
    canGroup: enabled('arrange.group') && count >= 2,
    canUngroup: enabled('arrange.ungroup') && kinds.includes('group'),
    choose: (value: 'slide' | 'selection') => {
      alignChoice = { selection: key, to: value }
      redraw((n) => n + 1)
    },
    align: (edge: string) => void run('arrange.align', { edge, to }),
    // Distribute spreads three or more over their own span; fewer, or to the slide, over the slide.
    distribute: (dir: string) => void run('arrange.distribute', { dir, to }),
    order: (command: string) => void run(command, {}),
    group: () => void run('arrange.group', {}),
    ungroup: () => void run('arrange.ungroup', {}),
    turn: (command: string) => void run(command, {}),
  }
}

/** The align and distribute rows, with the two "align to" choices under them. */
function AlignRows({ prefix }: { prefix: string }) {
  const { t } = useSlides()
  const arrange = useArrange()
  return (
    <>
      {ALIGN_EDGES.map(({ edge, icon }) => (
        <MenuItem key={edge} icon={icon} label={t(`align.${edge}`)} disabled={!arrange.can} onSelect={() => arrange.align(edge)} testId={`${prefix}-align-${edge}`} />
      ))}
      <MenuSeparator />
      {DISTRIBUTE.map(({ dir, icon }) => (
        <MenuItem key={dir} icon={icon} label={t(`distribute.${dir}`)} disabled={!arrange.can} onSelect={() => arrange.distribute(dir)} testId={`${prefix}-distribute-${dir}`} />
      ))}
      <MenuSeparator />
      <MenuItem label={t('align.toSlide')} checked={arrange.to === 'slide'} disabled={!arrange.can} onSelect={() => arrange.choose('slide')} testId={`${prefix}-align-to-slide`} />
      <MenuItem label={t('align.toSelection')} checked={arrange.to === 'selection'} disabled={!arrange.can || arrange.count < 2} onSelect={() => arrange.choose('selection')} testId={`${prefix}-align-to-selection`} />
    </>
  )
}

/** Shape Format's and Picture Format's Align button. */
export function AlignMenu({ primary }: { primary?: boolean }) {
  const { t } = useSlides()
  const arrange = useArrange()
  return (
    <RibbonMenu icon={AlignStartVerticalIcon} label={t('align')} size="small" disabled={!arrange.can} testId="slides-align" primary={primary}>
      <AlignRows prefix="slides" />
    </RibbonMenu>
  )
}

/** Bring Forward and Send Backward as buttons, Bring to Front and Send to Back beside them. */
export function OrderButtons({ primary }: { primary?: boolean }) {
  const { t, enabled } = useSlides()
  const arrange = useArrange()
  return (
    <>
      {ORDER.map(({ command, key, icon }) => {
        const main = command === 'arrange.bringForward' || command === 'arrange.sendBackward'
        return <RibbonButton key={command} icon={icon} label={t(key)} size="small" primary={primary && main} disabled={!enabled(command)} onClick={() => arrange.order(command)} data-testid={`slides-${key.replace(/[A-Z]/g, (c) => `-${c.toLowerCase()}`)}`} />
      })}
    </>
  )
}

/** Group, Ungroup. */
export function GroupMenu({ primary }: { primary?: boolean }) {
  const { t } = useSlides()
  const arrange = useArrange()
  return (
    <RibbonMenu icon={GroupIcon} label={t('group')} size="small" disabled={!arrange.canGroup && !arrange.canUngroup} testId="slides-group-menu" primary={primary}>
      <MenuItem icon={GroupIcon} label={t('group')} shortcut="⌘G" disabled={!arrange.canGroup} onSelect={arrange.group} testId="slides-group" />
      <MenuItem icon={UngroupIcon} label={t('ungroup')} shortcut="⇧⌘G" disabled={!arrange.canUngroup} onSelect={arrange.ungroup} testId="slides-ungroup" />
    </RibbonMenu>
  )
}

/** Rotate Right 90°, Rotate Left 90°, Flip Vertical, Flip Horizontal. */
export function RotateMenu({ primary }: { primary?: boolean }) {
  const { t, enabled } = useSlides()
  const arrange = useArrange()
  return (
    <RibbonMenu icon={RotateCwIcon} label={t('rotate')} size="small" disabled={!enabled('arrange.rotateRight')} testId="slides-rotate" primary={primary}>
      {TURNS.map(({ command, key, icon }) => (
        <MenuItem key={command} icon={icon} label={t(key)} disabled={!enabled(command)} onSelect={() => arrange.turn(command)} testId={`slides-${key.replace(/[A-Z]/g, (c) => `-${c.toLowerCase()}`)}`} />
      ))}
    </RibbonMenu>
  )
}

/** Home › Arrange: every arrange action in one menu, as WPS's and PowerPoint's Home tab has it. */
export function ArrangeMenu({ primary }: { primary?: boolean }) {
  const { t, enabled, view } = useSlides()
  const arrange = useArrange()
  return (
    <RibbonMenu icon={LayersIcon} label={t('arrange')} size="large" disabled={!arrange.can} testId="slides-arrange" primary={primary}>
      <div className="slides-menu-heading">{t('arrange.order')}</div>
      {ORDER.map(({ command, key, icon }) => (
        <MenuItem key={command} icon={icon} label={t(key)} disabled={!enabled(command)} onSelect={() => arrange.order(command)} testId={`slides-arrange-${key}`} />
      ))}
      <MenuSeparator />
      <div className="slides-menu-heading">{t('arrange.groupObjects')}</div>
      <MenuItem icon={GroupIcon} label={t('group')} shortcut="⌘G" disabled={!arrange.canGroup} onSelect={arrange.group} testId="slides-arrange-group" />
      <MenuItem icon={UngroupIcon} label={t('ungroup')} shortcut="⇧⌘G" disabled={!arrange.canUngroup} onSelect={arrange.ungroup} testId="slides-arrange-ungroup" />
      <MenuSeparator />
      <div className="slides-menu-heading">{t('align')}</div>
      <AlignRows prefix="slides-arrange" />
      <MenuSeparator />
      <div className="slides-menu-heading">{t('rotate')}</div>
      {TURNS.map(({ command, key, icon }) => (
        <MenuItem key={command} icon={icon} label={t(key)} disabled={!enabled(command)} onSelect={() => arrange.turn(command)} testId={`slides-arrange-${key}`} />
      ))}
      <MenuSeparator />
      <MenuItem icon={PanelRightIcon} label={t('selectionPane')} onSelect={() => view.setPane('selection')} testId="slides-arrange-selection-pane" />
    </RibbonMenu>
  )
}
