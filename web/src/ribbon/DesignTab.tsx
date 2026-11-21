import { useEffect, useState } from 'react'
import { ALargeSmallIcon as ALargeSmall, PaintbrushIcon as Paintbrush, PaletteIcon as Palette, RectangleHorizontalIcon as RectangleHorizontal } from '@kition/tessera/icons/react'
import { MenuItem, RibbonGroup, RibbonButton, CommandItem } from '@kition/sdk/editor'
import { useSlides } from '../slides'
import { ColorGrid, RibbonMenu } from './common'
import { themeIcon } from './themeIcon'

type Themes = { themes: Array<{ name: string; colors: string[] }>; colorSchemes: string[]; fontSchemes: Array<{ name: string; major: string; minor: string }> }

export function DesignTab() {
  const { run, enabled, query, t } = useSlides()
  const [themes, setThemes] = useState<Themes>({ themes: [], colorSchemes: [], fontSchemes: [] })
  useEffect(() => {
    void query<Themes>('design.themes').then((value) => value && setThemes(value))
  }, [query])
  return (
    <>
      <RibbonGroup label={t('group.themes')} icon={Palette}>
        <div className="slides-gallery" data-testid="slides-themes">{themes.themes.map((theme) => <RibbonButton key={theme.name} icon={themeIcon(theme.name, theme.colors)} label={theme.name} size="large" disabled={!enabled('design.theme')} onClick={() => void run('design.theme', { name: theme.name })} />)}</div>
      </RibbonGroup>
      <RibbonGroup label={t('group.variants')} icon={Paintbrush}>
        <RibbonMenu primary icon={Palette} label={t('themeColors')} size="large" disabled={!enabled('design.colors')} testId="slides-theme-colors">
          {themes.colorSchemes.map((name) => (
            <MenuItem key={name} label={name} onSelect={() => void run('design.colors', { name })} testId={`slides-colors-${name}`} />
          ))}
        </RibbonMenu>
        <RibbonMenu primary icon={ALargeSmall} label={t('themeFonts')} size="large" disabled={!enabled('design.fonts')}>
          {themes.fontSchemes.map((scheme) => (
            <MenuItem key={scheme.name} label={`${scheme.name} (${scheme.major} / ${scheme.minor})`} onSelect={() => void run('design.fonts', { name: scheme.name })} />
          ))}
        </RibbonMenu>
      </RibbonGroup>
      <RibbonGroup label={t('group.customize')} icon={RectangleHorizontal}>
        <RibbonMenu primary icon={RectangleHorizontal} label={t('slideSize')} size="large" disabled={!enabled('design.slideSize')}>
          <MenuItem label={t('widescreen')} onSelect={() => void run('design.slideSize', { preset: 'widescreen' })} />
          <MenuItem label={t('standard')} onSelect={() => void run('design.slideSize', { preset: 'standard' })} />
          <CommandItem command="design.slideSize" ask />
        </RibbonMenu>
        <RibbonMenu primary icon={Paintbrush} label={t('background')} size="large" disabled={!enabled('design.background')}>
          <MenuItem label={t('resetBackground')} onSelect={() => void run('design.background', { reset: true })} />
          <ColorGrid onPick={(color) => void run('design.background', { color })} testId="slides-backgrounds" />
        </RibbonMenu>
      </RibbonGroup>
    </>
  )
}
