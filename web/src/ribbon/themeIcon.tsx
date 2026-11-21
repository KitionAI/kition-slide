import type { IconComponent } from '@kition/tessera/icons/react'

/**
 * A theme's thumbnail as an icon: a slide in the theme's light background with a title bar in its
 * dark text colour and four accent swatches, so the theme gallery shows the themes rather than one
 * palette icon each. `colors` is the scheme in OOXML order: dk1, lt1, dk2, lt2, accent1–6, …
 */
const cache = new Map<string, IconComponent>()

export function themeIcon(name: string, colors: readonly string[]): IconComponent {
  const key = `${name}:${colors.join(',')}`
  const hit = cache.get(key)
  if (hit) return hit
  const [dark = '#1a1a1a', light = '#ffffff', , , ...rest] = colors
  const accents = rest.slice(0, 4)
  const Icon: IconComponent = ({ size = 24, className, style }) => (
    <svg width={size} height={size} viewBox="0 0 24 24" className={className} style={style} aria-hidden="true">
      <rect x={2.5} y={4.5} width={19} height={15} rx={2} fill={light} stroke="currentColor" strokeOpacity={0.25} />
      <rect x={5} y={7.5} width={10} height={2} rx={1} fill={dark} />
      {accents.map((color, i) => (
        <rect key={i} x={5 + i * 3.75} y={14} width={3} height={3} rx={0.75} fill={color} />
      ))}
    </svg>
  )
  cache.set(key, Icon)
  return Icon
}
