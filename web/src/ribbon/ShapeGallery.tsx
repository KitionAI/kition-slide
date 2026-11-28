/**
 * The shape gallery (plan stage 9, walkthrough F4; WPS W-E6, W-E7, W-E9, W-E10): every preset shape
 * as an icon the engine draws from the preset's own geometry, in WPS's nine groups, named by its
 * tooltip. Choosing one picks it as the drawing tool: a drag on the slide draws it as large as the
 * drag, a click makes it 72 × 72 pt, and the tool goes back to Select with the new shape selected.
 * A right-click on a shape offers Lock Drawing Mode: the tool stays for shape after shape until
 * Escape. The Icons menu is the same gallery for a few shapes.
 */
import { PenToolIcon as PenTool, PencilIcon as Pencil, LockIcon as Lock, ShapesIcon as Shapes, SplineIcon as Spline, FaceSlightlySmilingIcon as Smile, type IconComponent } from '@kition/tessera/icons/react'
import { MenuItem, RibbonMenu } from '@kition/sdk/editor'
import { useEffect, useState, type KeyboardEvent, type MouseEvent } from 'react'
import { useSlides } from '../slides'
import { useTools } from '../tools'

/** One sub-path of an icon: an SVG path in a square of `ICON_PT`, filled or not, stroked or not. */
type IconPath = { d: string; fill: boolean; stroke: boolean }
/** A preset's icon: its paths and the square that holds them all (`[x, y, side, side]`). */
type Icon = { view: [number, number, number, number]; paths: IconPath[] }
type Icons = Record<string, Icon>
type Preset = { name: string; label: string; category: string }

/** The icons' side, in the engine's points (drawn at that many CSS pixels). */
const ICON_PT = 20

/** The freeform tools, after the presets in Lines (Lucide icons: they have no preset geometry). */
const FREEFORM: Array<{ name: string; icon: IconComponent }> = [
  { name: 'curve', icon: Spline },
  { name: 'freeform', icon: PenTool },
  { name: 'scribble', icon: Pencil },
]

/** WPS's nine groups, in order, by the engine's category name. */
const GROUPS: Array<[category: string, key: string]> = [
  ['Lines', 'lines'],
  ['Rectangles', 'rectangles'],
  ['Basic Shapes', 'basicShapes'],
  ['Block Arrows', 'blockArrows'],
  ['Equation Shapes', 'equationShapes'],
  ['Flowchart', 'flowchart'],
  ['Stars and Banners', 'starsAndBanners'],
  ['Callouts', 'callouts'],
  ['Action Buttons', 'actionButtons'],
]

/** The Icons menu's shapes. */
const ICON_SHAPES = ['star5', 'heart', 'lightningBolt', 'sun', 'moon', 'cloud', 'smileyFace', 'chevron', 'rightArrow', 'flowChartDecision', 'flowChartMagneticDisk', 'flowChartDocument']

/** The icons of each open document, drawn once by its engine. */
const drawn = new WeakMap<object, Promise<Icons>>()

function usePresetIcons(): Icons {
  const { session } = useSlides()
  const doc = session.doc
  const [icons, setIcons] = useState<Icons>({})
  useEffect(() => {
    if (!doc) return undefined
    let stale = false
    let pending = drawn.get(doc)
    if (!pending) {
      pending = doc.invoke<string>('presetIcons', ICON_PT).then((json) => JSON.parse(json) as Icons).catch(() => ({}))
      drawn.set(doc, pending)
    }
    void pending.then((found) => {
      if (!stale) setIcons(found)
    })
    return () => {
      stale = true
    }
  }, [doc])
  return icons
}

function usePresets() {
  const { query } = useSlides()
  const [presets, setPresets] = useState<Preset[]>([])
  useEffect(() => {
    let stale = false
    void query<Preset[]>('shape.presets').then((list) => {
      if (!stale) setPresets(list ?? [])
    })
    return () => {
      stale = true
    }
  }, [query])
  return presets
}

/** A preset's icon as the engine drew it, in the text's color (closed shapes lightly filled). */
function PresetIcon({ icon }: { icon: Icon | undefined }) {
  const [x, y, side] = icon?.view ?? [0, 0, ICON_PT]
  const pad = side * 0.06
  return (
    <svg width={ICON_PT} height={ICON_PT} viewBox={`${x - pad} ${y - pad} ${side + 2 * pad} ${side + 2 * pad}`} aria-hidden="true" focusable="false">
      {(icon?.paths ?? []).map((path, index) => (
        <path key={index} d={path.d} fill={path.fill ? 'currentColor' : 'none'} fillOpacity={path.fill ? 0.16 : undefined} stroke={path.stroke ? 'currentColor' : 'none'} strokeWidth={1.1} vectorEffect="non-scaling-stroke" strokeLinejoin="round" strokeLinecap="round" />
      ))}
    </svg>
  )
}

/** Picks gallery shape `name` as the tool, locked for drawing one after another or not. */
function useChoose() {
  const tools = useTools()
  const { session } = useSlides()
  return (name: string, lock: boolean) => {
    tools.select(name)
    // The binding chooses the tool too, so the lock holds whichever of the two the engine has first.
    void session.doc?.invoke('setToolLock', name, lock).catch(() => undefined)
  }
}

/** The cells of `names`, and the Lock Drawing Mode menu a right-click on one opens. */
function GalleryGroups({ groups, prefix }: { groups: Array<{ key: string | null; names: string[] }>; prefix: string }) {
  const { t } = useSlides()
  const tools = useTools()
  const icons = usePresetIcons()
  const choose = useChoose()
  const [locking, setLocking] = useState<{ name: string; x: number; y: number } | null>(null)
  const freeform = new Map(FREEFORM.map((tool) => [tool.name, tool.icon]))
  const offerLock = (name: string, cell: HTMLElement, at?: { x: number; y: number }) => {
    const gallery = cell.closest('.slides-shapes-gallery')?.getBoundingClientRect()
    const box = cell.getBoundingClientRect()
    const point = at ?? { x: box.left + box.width / 2, y: box.bottom }
    setLocking({ name, x: point.x - (gallery?.left ?? 0), y: point.y - (gallery?.top ?? 0) })
  }
  const contextMenu = (name: string) => (event: MouseEvent<HTMLButtonElement>) => {
    event.preventDefault()
    event.stopPropagation()
    offerLock(name, event.currentTarget, event.clientX || event.clientY ? { x: event.clientX, y: event.clientY } : undefined)
  }
  const keyDown = (name: string) => (event: KeyboardEvent<HTMLButtonElement>) => {
    if ((event.shiftKey && event.key === 'F10') || event.key === 'ContextMenu') {
      event.preventDefault()
      event.stopPropagation()
      offerLock(name, event.currentTarget)
    }
  }
  return (
    <div className="slides-shapes-gallery" data-testid={`${prefix}-gallery`} onPointerDown={(event) => event.button === 0 && !(event.target as HTMLElement).closest('.slides-shapes-gallery-lock') && setLocking(null)}>
      {groups.map(({ key, names }) => (
        <div key={key ?? 'all'} role="group" aria-label={key ? t(`shapeGroup.${key}`) : undefined} data-testid={key ? `${prefix}-group-${key}` : undefined}>
          {key ? <div className="slides-menu-heading">{t(`shapeGroup.${key}`)}</div> : null}
          <div className="slides-shapes-gallery-grid">
            {names.map((name) => {
              const Icon = freeform.get(name)
              const label = t(`shape.${name}`)
              return (
                <button
                  key={name}
                  type="button"
                  role="menuitem"
                  className="slides-shapes-gallery-cell"
                  title={label}
                  aria-label={label}
                  data-active={tools.active === name || undefined}
                  data-testid={`${prefix}-${name}`}
                  onClick={() => choose(name, false)}
                  onContextMenu={contextMenu(name)}
                  onKeyDown={keyDown(name)}
                >
                  {Icon ? <Icon size={ICON_PT - 2} strokeWidth={1.5} aria-hidden="true" /> : <PresetIcon icon={icons[name]} />}
                </button>
              )
            })}
          </div>
        </div>
      ))}
      {locking ? (
        <div className="slides-shapes-gallery-lock" style={{ left: locking.x, top: locking.y }} data-testid={`${prefix}-lock-menu`}>
          <MenuItem icon={Lock} label={t('shapes.lock')} onSelect={() => choose(locking.name, true)} testId={`${prefix}-lock`} />
        </div>
      ) : null}
    </div>
  )
}

/** Insert › Shapes (also on Home and Shape Format): every shape the engine draws, in WPS's groups. */
export function ShapesMenu({ label, size = 'large', primary }: { label: string; size?: 'large' | 'small'; primary?: boolean }) {
  const presets = usePresets()
  const groups = GROUPS.map(([category, key]) => ({
    key,
    names: [...presets.filter((preset) => preset.category === category).map((preset) => preset.name), ...(key === 'lines' ? FREEFORM.map((tool) => tool.name) : [])],
  })).filter((group) => group.names.length > 0)
  return (
    <RibbonMenu icon={Shapes} label={label} size={size} testId="slides-shapes" primary={primary}>
      <GalleryGroups groups={groups} prefix="slides-shape" />
    </RibbonMenu>
  )
}

/** Insert › Icons: a few shapes, drawn and chosen as in the shape gallery. */
export function IconsMenu({ label }: { label: string }) {
  return (
    <RibbonMenu icon={Smile} label={label} size="large" testId="slides-icons">
      <GalleryGroups groups={[{ key: null, names: ICON_SHAPES }]} prefix="slides-icon" />
    </RibbonMenu>
  )
}
