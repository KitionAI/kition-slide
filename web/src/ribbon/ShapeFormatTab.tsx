/**
 * Shape Format and Picture Format, the editor's own (plan stage 1; walkthrough G5, G2, G11, G8):
 * fill and outline from color menus, arrange from the selection, size typed in. Every control runs
 * one engine command on the selection with its parameters; none opens a parameter form. The few
 * rare commands left (alt text, margins…) stay in More with their forms.
 */
import {
  BlendIcon,
  BoldIcon,
  CombineIcon,
  ContrastIcon,
  CropIcon,
  ImageUpIcon,
  ItalicIcon,
  LayersIcon,
  LockIcon,
  LockOpenIcon,
  MoveHorizontalIcon,
  MoveVerticalIcon,
  PaintBucketIcon,
  PaletteIcon,
  PanelRightIcon,
  PenLineIcon,
  RotateCcwIcon,
  RotateCwIcon,
  RulerIcon,
  ShapesIcon,
  SparklesIcon,
  SquaresExcludeIcon,
  SquaresIntersectIcon,
  SquaresSubtractIcon,
  SquaresUniteIcon,
  SunIcon,
  TextCursorInputIcon,
  TextWrapIcon,
  TypeIcon,
  UnderlineIcon,
  BaselineIcon,
  ScalingIcon,
  EllipsisIcon,
  type IconComponent,
} from '@kition/tessera/icons/react'
import { CommandButton, CommandItem, DrawingWarpMenu, MenuItem, MenuSeparator, RibbonButton, RibbonGroup, RibbonItem, RibbonMenu, RibbonRow, RibbonStack, useKitEditor } from '@kition/sdk/editor'
import { useEffect, useRef, useState, type KeyboardEvent } from 'react'
import { useSlides } from '../slides'
import { useTools } from '../tools'
import { releaseField } from '../focus'
import { AlignMenu, GroupMenu, OrderButtons, RotateMenu } from './ArrangeMenu'
import { ColorPicker, ShapesMenu } from './common'
import { usePicturePick } from './HomeTab'
import { CHANGE_TO } from '../contextMenus'

/** Points in a centimetre: sizes show in centimetres, as WPS shows them. */
const CM = 72 / 2.54

/** A typed length in points: a number in centimetres, or with its unit (cm, mm, in, ", pt, 厘米, 毫米, 英寸, 磅). */
export function parseLength(text: string): number | null {
  const match = /^\s*(-?\d+(?:[.,]\d+)?|-?[.,]\d+)\s*(cm|厘米|mm|毫米|in|"|英寸|pt|磅)?\s*$/i.exec(text)
  if (!match) return null
  const value = Number(match[1].replace(',', '.'))
  if (!Number.isFinite(value) || value <= 0) return null
  const unit = (match[2] ?? 'cm').toLowerCase()
  const pt = unit === 'mm' || unit === '毫米' ? (value / 10) * CM : unit === 'in' || unit === '"' || unit === '英寸' ? value * 72 : unit === 'pt' || unit === '磅' ? value : value * CM
  return Math.min(pt, 15840)
}

/** A typed angle in degrees (the ° optional), any sign; null when it is not a number. */
export function parseAngle(text: string): number | null {
  const match = /^\s*(-?\d+(?:[.,]\d+)?)\s*°?\s*$/.exec(text)
  if (!match) return null
  const value = Number(match[1].replace(',', '.'))
  return Number.isFinite(value) ? (((value % 360) + 360) % 360) : null
}

type Box = { x: number; y: number; w: number; h: number; rot?: number }

/** The selected objects' boxes (slide points), read again after every change. */
function useSelectedBoxes() {
  const { query, facts, session } = useSlides()
  const ids = facts.selection?.shapes ?? []
  const key = ids.join(',')
  const revision = session.change.revision
  const [boxes, setBoxes] = useState<Box[]>([])
  useEffect(() => {
    let stale = false
    const list = key ? key.split(',').map(Number) : []
    void Promise.all(list.map((id) => query<{ box?: Box }>('shape.inspect', { id }))).then((answers) => {
      if (!stale) setBoxes(answers.map((answer) => answer?.box).filter((box): box is Box => box != null))
    })
    return () => {
      stale = true
    }
  }, [query, key, revision])
  return boxes
}

/** One value the selection shares, or null when they differ (the field is blank then). */
function shared(boxes: Box[], of: (box: Box) => number) {
  if (!boxes.length) return null
  const first = of(boxes[0])
  return boxes.every((box) => Math.abs(of(box) - first) < 0.005) ? first : null
}

/**
 * A size field: shows the value, takes a typed one on Enter or when it is left, Escape puts the
 * shown value back; the arrow keys step it. Enter and Escape give the keyboard back to the slide.
 */
function SizeField({ icon: Icon, label, value, show, parse, step, onCommit, testId }: { icon: IconComponent; label: string; value: number | null; show: (value: number) => string; parse: (text: string) => number | null; step: number; onCommit: (value: number) => void; testId: string }) {
  const { enabled, session } = useSlides()
  const shown = value == null ? '' : show(value)
  const [text, setText] = useState(shown)
  const focused = useRef(false)
  const done = useRef(false)
  /** The click that focused the field: its release must not undo the select-all. */
  const entering = useRef(false)
  const input = useRef<HTMLInputElement>(null)
  useEffect(() => {
    if (!focused.current) setText(shown)
  }, [shown])
  // Escape in the field puts the shown value back, even when an open menu takes the key first (the
  // field then loses the keyboard without its own Escape): seen on the window, before anyone.
  useEffect(() => {
    const escape = (event: globalThis.KeyboardEvent) => {
      if (event.key === 'Escape' && event.target === input.current) done.current = true
    }
    window.addEventListener('keydown', escape, true)
    return () => window.removeEventListener('keydown', escape, true)
  }, [])
  const commit = () => {
    // The shown value is rounded: left as shown, it changes nothing.
    if (text.trim() === shown) return
    const typed = parse(text)
    if (typed == null || (value != null && Math.abs(typed - value) < 0.005)) {
      setText(shown)
      return
    }
    onCommit(typed)
  }
  const keyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === 'Enter') {
      event.preventDefault()
      commit()
      done.current = true
      releaseField(event.currentTarget, session.state?.editingText)
    } else if (event.key === 'Escape') {
      event.preventDefault()
      event.stopPropagation()
      setText(shown)
      done.current = true
      releaseField(event.currentTarget, session.state?.editingText)
    } else if (event.key === 'ArrowUp' || event.key === 'ArrowDown') {
      event.preventDefault()
      const from = parse(text) ?? value
      if (from == null) return
      const next = from + (event.key === 'ArrowUp' ? step : -step)
      setText(show(next))
      onCommit(next)
    }
    // The field keeps its keys: no slide shortcut runs while a size is typed.
    event.stopPropagation()
  }
  return (
    <label className="slides-size-field" title={label}>
      <Icon size={14} aria-hidden />
      <input
        ref={input}
        className="kui-ribbon-field"
        aria-label={label}
        value={text}
        disabled={!enabled('shape.resize')}
        onChange={(event) => setText(event.target.value)}
        onFocus={(event) => {
          focused.current = true
          done.current = false
          entering.current = true
          event.currentTarget.select()
        }}
        onMouseUp={(event) => {
          if (entering.current) event.preventDefault()
          entering.current = false
        }}
        onBlur={() => {
          focused.current = false
          if (!done.current) commit()
          else setText(shown)
          done.current = false
        }}
        onKeyDown={keyDown}
        data-testid={testId}
      />
    </label>
  )
}

/** Height, width and rotation typed in, and whether width and height keep their ratio. */
function SizeGroup({ lockByDefault }: { lockByDefault: boolean }) {
  const { run, t } = useSlides()
  const boxes = useSelectedBoxes()
  const [lock, setLock] = useState(lockByDefault)
  const cm = (pt: number) => `${(Math.round((pt / CM) * 100) / 100).toFixed(2)} ${t('unit.cm')}`
  return (
    <RibbonGroup label={t('group.size')} icon={RulerIcon}>
      <RibbonStack>
        <RibbonItem primary>
          <SizeField icon={MoveVerticalIcon} label={t('size.height')} value={shared(boxes, (box) => box.h)} show={cm} parse={parseLength} step={0.1 * CM} onCommit={(h) => void run('shape.resize', { h, lockAspect: lock })} testId="slides-size-height" />
        </RibbonItem>
        <RibbonItem primary>
          <SizeField icon={MoveHorizontalIcon} label={t('size.width')} value={shared(boxes, (box) => box.w)} show={cm} parse={parseLength} step={0.1 * CM} onCommit={(w) => void run('shape.resize', { w, lockAspect: lock })} testId="slides-size-width" />
        </RibbonItem>
        <RibbonItem>
          <SizeField icon={RotateCwIcon} label={t('size.rotation')} value={shared(boxes, (box) => box.rot ?? 0)} show={(deg) => `${Math.round(deg * 10) / 10}°`} parse={parseAngle} step={1} onCommit={(deg) => void run('shape.rotate', { deg: ((deg % 360) + 360) % 360 })} testId="slides-size-rotation" />
        </RibbonItem>
      </RibbonStack>
      <RibbonButton icon={lock ? LockIcon : LockOpenIcon} label={t('size.lockAspect')} size="small" primary pressed={lock} onClick={() => setLock((on) => !on)} data-testid="slides-size-lock" />
    </RibbonGroup>
  )
}

/** The arrange group both tabs share: order, align, group, rotate, Selection Pane. */
function ArrangeGroup() {
  const { t, view } = useSlides()
  return (
    <RibbonGroup label={t('group.arrange')} icon={LayersIcon}>
      <RibbonStack>
        <OrderButtons primary />
      </RibbonStack>
      <RibbonStack>
        <AlignMenu primary />
        <GroupMenu primary />
        <RotateMenu primary />
      </RibbonStack>
      <RibbonButton icon={PanelRightIcon} label={t('selectionPane')} size="small" onClick={() => view.setPane('selection')} data-testid="slides-selection-pane" />
    </RibbonGroup>
  )
}

const WEIGHTS = [0.75, 1, 1.5, 2.25, 3, 4.5, 6]
const DASHES = ['solid', 'sysDot', 'sysDash', 'dash', 'lgDash', 'dashDot']

/** An outline's color, weight and dashes (Shape Outline, Picture Border). */
function OutlineMenu({ label, icon, testId, primary }: { label: string; icon: IconComponent; testId: string; primary?: boolean }) {
  const { run, enabled, t } = useSlides()
  return (
    <RibbonMenu icon={icon} label={label} size="small" disabled={!enabled('shape.line')} testId={testId} primary={primary}>
      <MenuItem label={t('noOutline')} onSelect={() => void run('shape.line', { none: true })} testId={`${testId}-none`} />
      <ColorPicker onPick={(color) => void run('shape.line', { color })} testId={`${testId}-colors`} />
      <MenuSeparator />
      <div className="slides-menu-heading">{t('outline.weight')}</div>
      {WEIGHTS.map((width) => <MenuItem key={width} label={t('outline.points', { pt: width })} onSelect={() => void run('shape.line', { width })} testId={`${testId}-weight-${width}`} />)}
      <div className="slides-menu-heading">{t('outline.dashes')}</div>
      {DASHES.map((dash) => <MenuItem key={dash} label={t(`dash.${dash}`)} onSelect={() => void run('shape.line', { dash })} />)}
    </RibbonMenu>
  )
}

/** The shapes Change Shape offers: the common ones, by name in the person's language. */
const MERGE: Array<{ op: string; icon: IconComponent }> = [
  { op: 'union', icon: SquaresUniteIcon },
  { op: 'combine', icon: SquaresExcludeIcon },
  { op: 'fragment', icon: CombineIcon },
  { op: 'intersect', icon: SquaresIntersectIcon },
  { op: 'subtract', icon: SquaresSubtractIcon },
]

/** A More menu of the rare commands left to their parameter forms, each by its own name. */
function MoreMenu({ items }: { items: Array<[string, string]> }) {
  const { t, byId } = useSlides()
  const shown = items.filter(([command]) => byId.has(command))
  if (!shown.length) return null
  return (
    <RibbonGroup label={t('group.commands')} icon={EllipsisIcon}>
      <RibbonMenu icon={EllipsisIcon} label={t('more')} size="large" testId="slides-more-commands" primary>
        {shown.map(([command, key]) => <CommandItem key={command} command={command} label={t(key)} />)}
      </RibbonMenu>
    </RibbonGroup>
  )
}

export function ShapeFormatTab() {
  const { run, enabled, format, t, session } = useSlides()
  const editor = useKitEditor()
  const tools = useTools()
  const count = session.state?.selected ?? 0
  return (
    <>
      <RibbonGroup label={t('group.insertShapes')} icon={ShapesIcon}>
        <ShapesMenu label={t('shapes')} primary />
        <RibbonStack>
          <RibbonButton icon={TextCursorInputIcon} label={t('textBox')} size="small" pressed={tools.active === 'textBox'} onClick={() => tools.select('textBox')} data-testid="slides-tool-textBox" />
          <RibbonMenu icon={ShapesIcon} label={t('changeShape')} size="small" disabled={!enabled('shape.change')} testId="slides-change-shape">
            {CHANGE_TO.map((preset) => <MenuItem key={preset} label={t(`shape.${preset}`)} onSelect={() => void run('shape.change', { preset })} testId={`slides-change-shape-${preset}`} />)}
          </RibbonMenu>
          <RibbonMenu icon={CombineIcon} label={t('mergeShapes')} size="small" disabled={!enabled('shape.merge') || count < 2} testId="slides-merge-shapes">
            {MERGE.map(({ op, icon }) => <MenuItem key={op} icon={icon} label={t(`merge.${op}`)} onSelect={() => void run('shape.merge', { op })} />)}
          </RibbonMenu>
        </RibbonStack>
      </RibbonGroup>

      <RibbonGroup label={t('group.shapeStyles')} icon={PaintBucketIcon}>
        <RibbonStack>
          <RibbonMenu icon={PaintBucketIcon} label={t('shapeFill')} size="small" disabled={!enabled('shape.fill')} testId="slides-shape-fill" primary>
            <MenuItem label={t('noFill')} onSelect={() => void run('shape.fill', { none: true })} testId="slides-shape-fill-none" />
            <ColorPicker onPick={(color) => void run('shape.fill', { color })} testId="slides-fills" />
          </RibbonMenu>
          <OutlineMenu label={t('shapeOutline')} icon={PenLineIcon} testId="slides-shape-outline" primary />
          <RibbonMenu icon={SparklesIcon} label={t('shapeEffects')} size="small" disabled={!enabled('shape.effects')} testId="slides-shape-effects">
            <div className="slides-menu-heading">{t('effects.shadow')}</div>
            {['none', 'outer', 'inner', 'perspective'].map((shadow) => <MenuItem key={shadow} label={t(`shadow.${shadow}`)} onSelect={() => void run('shape.effects', { shadow })} />)}
            <div className="slides-menu-heading">{t('effects.reflection')}</div>
            {['none', 'tight', 'half', 'full'].map((reflection) => <MenuItem key={reflection} label={t(`reflection.${reflection}`)} onSelect={() => void run('shape.effects', { reflection })} />)}
            <div className="slides-menu-heading">{t('effects.softEdges')}</div>
            {[0, 2.5, 5, 10].map((softEdges) => <MenuItem key={softEdges} label={softEdges ? t('outline.points', { pt: softEdges }) : t('effects.noSoftEdges')} onSelect={() => void run('shape.effects', { softEdges })} />)}
            <MenuSeparator />
            <MenuItem label={t('effects.reset')} onSelect={() => void run('shape.effects', { reset: true })} />
          </RibbonMenu>
        </RibbonStack>
      </RibbonGroup>

      <RibbonGroup label={t('group.artText')} icon={TypeIcon}>
        <RibbonStack>
          <RibbonRow>
            <CommandButton command="format.bold" icon={BoldIcon} label={t('bold')} size="icon" pressed={format.bold} params={{}} />
            <CommandButton command="format.italic" icon={ItalicIcon} label={t('italic')} size="icon" pressed={format.italic} params={{}} />
            <CommandButton command="format.underline" icon={UnderlineIcon} label={t('underline')} size="icon" pressed={format.underline} params={{}} />
            <RibbonMenu icon={BaselineIcon} label={t('fontColor')} size="icon" disabled={!enabled('format.color')} swatch="#C00000" testId="slides-shape-font-color">
              <ColorPicker onPick={(color) => void run('format.color', { color })} testId="slides-shape-font-colors" />
            </RibbonMenu>
          </RibbonRow>
          <RibbonRow>
            <DrawingWarpMenu testId="slide-drawing" disabled={!editor.enabled('format.transform')} onPick={(preset) => void editor.run('format.transform', { preset: preset ?? 'none' })} />
            <RibbonMenu icon={ScalingIcon} label={t('autofit')} size="icon" disabled={!enabled('format.autofit')} testId="slides-autofit">
              {['none', 'shrink', 'resize'].map((mode) => <MenuItem key={mode} label={t(`autofit.${mode}`)} onSelect={() => void run('format.autofit', { mode })} />)}
            </RibbonMenu>
            <RibbonMenu icon={TextWrapIcon} label={t('wrapText')} size="icon" disabled={!enabled('format.wrap')} testId="slides-wrap">
              <MenuItem label={t('wrap.on')} onSelect={() => void run('format.wrap', { on: true })} />
              <MenuItem label={t('wrap.off')} onSelect={() => void run('format.wrap', { on: false })} />
            </RibbonMenu>
          </RibbonRow>
        </RibbonStack>
      </RibbonGroup>

      <ArrangeGroup />
      <SizeGroup lockByDefault={false} />
      <MoreMenu items={[['shape.altText', 'more.altText'], ['format.margins', 'more.margins'], ['format.paragraph', 'more.paragraph'], ['format.textOutline', 'more.textOutline'], ['shape.style', 'more.quickStyle'], ['arrange.regroup', 'more.regroup'], ['shape.setDefault', 'more.setDefault']]} />
    </>
  )
}

const LEVELS = [-0.4, -0.2, 0, 0.2, 0.4]
const percent = (value: number) => `${value > 0 ? '+' : ''}${Math.round(value * 100)}%`

export function PictureFormatTab() {
  const { run, enabled, t } = useSlides()
  const picture = usePicturePick()
  const change = async () => {
    const picked = await picture.pick()
    if (picked) await run('image.change', picked)
  }
  return (
    <>
      <RibbonGroup label={t('group.adjust')} icon={SunIcon}>
        <RibbonStack>
          <RibbonMenu icon={SunIcon} label={t('picture.corrections')} size="small" disabled={!enabled('image.adjust')} testId="slides-picture-corrections" primary>
            <div className="slides-menu-heading">{t('picture.brightness')}</div>
            {LEVELS.map((brightness) => <MenuItem key={`b${brightness}`} label={percent(brightness)} onSelect={() => void run('image.adjust', { brightness })} testId={`slides-picture-brightness-${brightness}`} />)}
            <div className="slides-menu-heading">{t('picture.contrast')}</div>
            {LEVELS.map((contrast) => <MenuItem key={`c${contrast}`} label={percent(contrast)} onSelect={() => void run('image.adjust', { contrast })} />)}
          </RibbonMenu>
          <RibbonMenu icon={PaletteIcon} label={t('picture.color')} size="small" disabled={!enabled('image.adjust')} testId="slides-picture-color" primary>
            <MenuItem label={t('picture.grayscale')} onSelect={() => void run('image.adjust', { grayscale: true })} />
            <MenuItem label={t('picture.fullColor')} onSelect={() => void run('image.adjust', { grayscale: false, saturation: 1 })} />
            <div className="slides-menu-heading">{t('picture.saturation')}</div>
            {[0, 0.5, 1, 2].map((saturation) => <MenuItem key={saturation} label={`${saturation * 100}%`} onSelect={() => void run('image.adjust', { saturation })} />)}
          </RibbonMenu>
          <RibbonMenu icon={BlendIcon} label={t('picture.transparency')} size="small" disabled={!enabled('image.adjust')} testId="slides-picture-transparency">
            {[0, 0.25, 0.5, 0.75].map((transparency) => <MenuItem key={transparency} label={`${transparency * 100}%`} onSelect={() => void run('image.adjust', { transparency })} />)}
          </RibbonMenu>
        </RibbonStack>
        <RibbonStack>
          {picture.can ? <RibbonButton icon={ImageUpIcon} label={t('picture.change')} size="small" disabled={!enabled('image.change')} onClick={() => void change()} data-testid="slides-picture-change" /> : null}
          <CommandButton command="image.reset" icon={RotateCcwIcon} label={t('picture.reset')} size="small" params={{}} />
        </RibbonStack>
      </RibbonGroup>

      <RibbonGroup label={t('group.pictureStyles')} icon={PenLineIcon}>
        <OutlineMenu label={t('picture.border')} icon={PenLineIcon} testId="slides-picture-border" primary />
        <RibbonMenu icon={ContrastIcon} label={t('shapeEffects')} size="small" disabled={!enabled('shape.effects')} testId="slides-picture-effects">
          {['none', 'outer', 'inner', 'perspective'].map((shadow) => <MenuItem key={shadow} label={t(`shadow.${shadow}`)} onSelect={() => void run('shape.effects', { shadow })} />)}
          <MenuSeparator />
          {['none', 'tight', 'half', 'full'].map((reflection) => <MenuItem key={reflection} label={t(`reflection.${reflection}`)} onSelect={() => void run('shape.effects', { reflection })} />)}
        </RibbonMenu>
      </RibbonGroup>

      <ArrangeGroup />

      <RibbonGroup label={t('group.crop')} icon={CropIcon}>
        <RibbonMenu icon={CropIcon} label={t('picture.crop')} size="small" disabled={!enabled('image.crop')} testId="slides-picture-crop" primary>
          {[0.1, 0.2].map((side) => <MenuItem key={side} label={t('crop.each', { pct: side * 100 })} onSelect={() => void run('image.crop', { left: side, top: side, right: side, bottom: side })} />)}
          <MenuItem label={t('crop.reset')} onSelect={() => void run('image.crop', { left: 0, top: 0, right: 0, bottom: 0 })} />
        </RibbonMenu>
      </RibbonGroup>
      <SizeGroup lockByDefault />
      <MoreMenu items={[['shape.altText', 'more.altText'], ['image.crop', 'more.crop'], ['image.adjust', 'more.adjust']]} />
    </>
  )
}
