import { CircleIcon as Circle, MinusIcon as Minus, MoveRightIcon as MoveRight, PencilIcon as Pencil, SplineIcon as Spline, SquareIcon as Square, StarIcon as Star, TriangleIcon as Triangle, type IconComponent } from '@kition/tessera/icons/react'
import { CommandButton, MenuItem, RibbonMenu } from '@kition/sdk/editor'
import { useEffect, useState } from 'react'
import { useSlides } from '../slides'
import { useTools } from '../tools'

/** The theme's colors first (they follow theme changes), then the standard ones. */
export const THEME_COLORS = ['tx1', 'bg1', 'tx2', 'bg2', 'accent1', 'accent2', 'accent3', 'accent4', 'accent5', 'accent6']
export const STANDARD_COLORS = ['#C00000', '#FF0000', '#FFC000', '#FFFF00', '#92D050', '#00B050', '#00B0F0', '#0070C0', '#002060', '#7030A0']

/** A grid of color swatches; theme colors show as their names until a theme preview exists. */
export function ColorGrid({ onPick, testId }: { onPick: (color: string) => void; testId: string }) {
  return (
    <div style={{ display: 'grid', gridTemplateColumns: 'repeat(10, 18px)', gap: 4, padding: 6 }} data-testid={testId}>
      {STANDARD_COLORS.map((color) => (
        <button key={color} type="button" title={color} aria-label={color} onClick={() => onPick(color)} style={{ width: 18, height: 18, border: '1px solid var(--kit-border)', borderRadius: 4, background: color, cursor: 'pointer' }} />
      ))}
      {THEME_COLORS.map((color) => (
        <button key={color} type="button" title={color} aria-label={color} onClick={() => onPick(color)} className="slides-theme-swatch" data-color={color} />
      ))}
    </div>
  )
}

/** The shapes the engine draws with its own tools, as a gallery. */
export const SHAPE_TOOLS: Array<{ id: string; icon: IconComponent; label: string }> = [
  { id: 'rect', icon: Square, label: 'Rectangle' },
  { id: 'roundRect', icon: Square, label: 'Rounded Rectangle' },
  { id: 'ellipse', icon: Circle, label: 'Oval' },
  { id: 'triangle', icon: Triangle, label: 'Triangle' },
  { id: 'rightArrow', icon: MoveRight, label: 'Arrow' },
  { id: 'star5', icon: Star, label: 'Star' },
  { id: 'line', icon: Minus, label: 'Line' },
  { id: 'straightConnector1', icon: MoveRight, label: 'Connector' },
  { id: 'freeform', icon: Spline, label: 'Freeform' },
  { id: 'scribble', icon: Pencil, label: 'Scribble' },
]

export function ShapesMenu({ label, size = 'large', primary }: { label: string; size?: 'large' | 'small'; primary?: boolean }) {
  const tools = useTools()
  const { query, run, session } = useSlides()
  const [presets, setPresets] = useState<Array<{ name: string; label: string; category: string }>>([])
  useEffect(() => { void query<typeof presets>('shape.presets').then((list) => setPresets(list ?? [])) }, [query])
  return (
    <RibbonMenu icon={SHAPE_TOOLS[0].icon} label={label} size={size} testId="slides-shapes" primary={primary}>
      {SHAPE_TOOLS.map((shape) => (
        <MenuItem key={shape.id} icon={shape.icon} label={shape.label} onSelect={() => tools.select(shape.id)} testId={`slides-shape-${shape.id}`} />
      ))}
      {presets.filter((preset) => !SHAPE_TOOLS.some((tool) => tool.id === preset.name)).map((preset) => <MenuItem key={preset.name} label={`${preset.label} (${preset.category})`} onSelect={() => {
        const area = session.state?.area
        void run('shape.insert', { preset: preset.name, rect: [area ? (area.x1 - area.x0) / 2 - 48 : 400, area ? (area.y1 - area.y0) / 2 - 48 : 200, 96, 96] })
      }} />)}
    </RibbonMenu>
  )
}

export { CommandButton, MenuItem, RibbonMenu }
