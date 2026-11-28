import { CommandButton, MenuItem, RibbonMenu } from '@kition/sdk/editor'
import { useEffect, useRef, useState, type KeyboardEvent } from 'react'
import { useSlides } from '../slides'

/** The theme's colors first (they follow theme changes), then the standard ones. */
export const THEME_COLORS = ['tx1', 'bg1', 'tx2', 'bg2', 'accent1', 'accent2', 'accent3', 'accent4', 'accent5', 'accent6']
export const STANDARD_COLORS = ['#C00000', '#FF0000', '#FFC000', '#FFFF00', '#92D050', '#00B050', '#00B0F0', '#0070C0', '#002060', '#7030A0']

/** The theme's scheme slot each theme color name shows (`tx1` is the scheme's `dk1`, and so on). */
const SCHEME_SLOT: Record<string, string> = { tx1: 'dk1', bg1: 'lt1', tx2: 'dk2', bg2: 'lt2' }

/** The current theme's colors by name (`tx1`, `accent1`…) as `#RRGGBB`, read again after every change. */
export function useThemeColors() {
  const { query, session } = useSlides()
  const [colors, setColors] = useState<Record<string, string>>({})
  const revision = session.change.revision
  useEffect(() => {
    let stale = false
    // `theme.colors` with no values only reads the scheme.
    void query<{ colors?: Record<string, string> }>('theme.colors', {}).then((answer) => {
      if (stale || !answer?.colors) return
      setColors(Object.fromEntries(THEME_COLORS.map((name) => [name, answer.colors?.[SCHEME_SLOT[name] ?? name] ?? ''])))
    })
    return () => {
      stale = true
    }
  }, [query, revision])
  return colors
}

/** A color a menu picks: `#RRGGBB`, a theme color's name, or a theme color made lighter or darker. */
export type PickedColor = string | { scheme: string; lumMod: number; lumOff?: number }

/** The tints and shades under each theme color, as WPS and PowerPoint show them: lighter 80, 60, 40 %, darker 25, 50 %. */
const SHADES: Array<{ key: string; lumMod: number; lumOff?: number }> = [
  { key: 'lighter80', lumMod: 20000, lumOff: 80000 },
  { key: 'lighter60', lumMod: 40000, lumOff: 60000 },
  { key: 'lighter40', lumMod: 60000, lumOff: 40000 },
  { key: 'darker25', lumMod: 75000 },
  { key: 'darker50', lumMod: 50000 },
]

/** `#RRGGBB` with its HSL lightness scaled by `mod` and raised by `off` (both in 1/100000), as the file's color transforms do. */
function shade(hex: string, mod: number, off = 0) {
  const value = /^#([0-9a-f]{6})$/i.exec(hex)?.[1]
  if (!value) return hex
  const [r, g, b] = [0, 2, 4].map((at) => parseInt(value.slice(at, at + 2), 16) / 255)
  const max = Math.max(r, g, b)
  const min = Math.min(r, g, b)
  let h = 0
  let s = 0
  let l = (max + min) / 2
  if (max !== min) {
    const d = max - min
    s = l > 0.5 ? d / (2 - max - min) : d / (max + min)
    h = max === r ? (g - b) / d + (g < b ? 6 : 0) : max === g ? (b - r) / d + 2 : (r - g) / d + 4
    h /= 6
  }
  l = Math.min(1, Math.max(0, l * (mod / 100000) + off / 100000))
  const hue = (p: number, q: number, t: number) => {
    const u = t < 0 ? t + 1 : t > 1 ? t - 1 : t
    return u < 1 / 6 ? p + (q - p) * 6 * u : u < 1 / 2 ? q : u < 2 / 3 ? p + (q - p) * (2 / 3 - u) * 6 : p
  }
  const q = l < 0.5 ? l * (1 + s) : l + s - l * s
  const p = 2 * l - q
  const rgb = s === 0 ? [l, l, l] : [hue(p, q, h + 1 / 3), hue(p, q, h), hue(p, q, h - 1 / 3)]
  return `#${rgb.map((c) => Math.round(c * 255).toString(16).padStart(2, '0')).join('')}`.toUpperCase()
}

/** A typed color: `#RGB` or `#RRGGBB`, the `#` optional; null when it is neither. */
export function parseHex(text: string) {
  const value = /^#?([0-9a-f]{3}|[0-9a-f]{6})$/i.exec(text.trim())?.[1]
  if (!value) return null
  const full = value.length === 3 ? [...value].map((c) => c + c).join('') : value
  return `#${full.toUpperCase()}`
}

/** One swatch button: `label` is its name and tooltip. */
function Swatch({ color, label, onPick, id }: { color: string; label: string; onPick: () => void; id?: string }) {
  return <button type="button" title={label} aria-label={label} onClick={onPick} className="slides-swatch" data-color={id} style={{ background: color || undefined }} />
}

/** A grid of color swatches: the standard colors (named by their hex), then the theme's colors as they are now. */
export function ColorGrid({ onPick, testId }: { onPick: (color: string) => void; testId: string }) {
  const { t } = useSlides()
  const theme = useThemeColors()
  return (
    <div style={{ display: 'grid', gridTemplateColumns: 'repeat(10, 18px)', gap: 4, padding: 6 }} data-testid={testId}>
      {STANDARD_COLORS.map((color) => (
        <button key={color} type="button" title={color} aria-label={color} onClick={() => onPick(color)} style={{ width: 18, height: 18, border: '1px solid var(--kit-border)', borderRadius: 4, background: color, cursor: 'pointer' }} />
      ))}
      {THEME_COLORS.map((color) => (
        <Swatch key={color} id={color} color={theme[color] ?? ''} label={t(`color.${color}`)} onPick={() => onPick(color)} />
      ))}
    </div>
  )
}

/**
 * A color menu's body (shape fill, outline, font color; walkthrough G2): the theme's colors with
 * their tints and shades, the standard colors, and a `#RRGGBB` field. Enter in the field applies it
 * (through its button, so the menu closes as on any choice).
 */
export function ColorPicker({ onPick, testId }: { onPick: (color: PickedColor) => void; testId: string }) {
  const { t } = useSlides()
  const theme = useThemeColors()
  const [text, setText] = useState('')
  const apply = useRef<HTMLButtonElement>(null)
  const typed = parseHex(text)
  const keyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    // The field keeps its own keys (the menu's arrows, Home and End move through its rows otherwise).
    if (event.key === 'Escape') return
    event.stopPropagation()
    if (event.key === 'Enter' && typed) {
      event.preventDefault()
      apply.current?.click()
    }
  }
  return (
    <div className="slides-color-picker" data-testid={testId}>
      <div className="slides-menu-heading">{t('color.theme')}</div>
      <div className="slides-swatches">
        {THEME_COLORS.map((name) => (
          <Swatch key={name} id={name} color={theme[name] ?? ''} label={t(`color.${name}`)} onPick={() => onPick(name)} />
        ))}
        {SHADES.map((step) =>
          THEME_COLORS.map((name) => (
            <Swatch
              key={`${name}-${step.key}`}
              id={`${name}-${step.key}`}
              color={theme[name] ? shade(theme[name], step.lumMod, step.lumOff) : ''}
              label={t('color.shade', { color: t(`color.${name}`), shade: t(`color.${step.key}`) })}
              onPick={() => onPick({ scheme: name, lumMod: step.lumMod, ...(step.lumOff ? { lumOff: step.lumOff } : {}) })}
            />
          )),
        )}
      </div>
      <div className="slides-menu-heading">{t('color.standard')}</div>
      <div className="slides-swatches">
        {STANDARD_COLORS.map((color) => (
          <Swatch key={color} id={color} color={color} label={color} onPick={() => onPick(color)} />
        ))}
      </div>
      <div className="slides-hex-row">
        <input className="kui-ribbon-field" aria-label={t('color.hex')} placeholder="#RRGGBB" value={text} aria-invalid={text !== '' && !typed} onChange={(event) => setText(event.target.value)} onKeyDown={keyDown} data-testid={`${testId}-hex`} />
        <button ref={apply} type="button" className="kui-button" disabled={!typed} onClick={() => typed && onPick(typed)} data-testid={`${testId}-hex-apply`}>
          {t('color.apply')}
        </button>
      </div>
    </div>
  )
}

export { CommandButton, MenuItem, RibbonMenu }
