import type { RefObject } from 'react'
import { BookOpenIcon as BookOpen, Columns2Icon as Columns2, FileTextIcon as FileText, Grid2x2Icon as Grid2X2, LayersIcon as Layers, Maximize2Icon as Maximize2, MoonIcon as Moon, PanelsTopLeftIcon as PanelsTopLeft, PaletteIcon as Palette, PresentationIcon as Presentation, SearchIcon as Search, StickyNoteIcon as StickyNote } from '@kition/tessera/icons/react'
import { MenuItem, RibbonButton, RibbonGroup, RibbonStack, type KitCanvasHandle } from '@kition/sdk/editor'
import { useSlides } from '../slides'
import { RibbonMenu } from './common'
import type { ViewMode } from '../useSlideView'

export function ViewTab({ canvas }: { canvas: RefObject<KitCanvasHandle | null> }) {
  const { t, view, run, session } = useSlides()
  const master = (session.state?.target.kind ?? 'slides') !== 'slides'
  const switchMode = async (mode: ViewMode) => { if (master) await run('view.closeMaster', {}); view.setMode(mode) }
  return <>
    <RibbonGroup label={t('group.views')} icon={PanelsTopLeft}>
      {([['normal', PanelsTopLeft], ['outline', Columns2], ['sorter', Grid2X2], ['notesPage', FileText]] as const).map(([mode, icon]) => <RibbonButton key={mode} primary={mode === 'normal' || mode === 'sorter'} icon={icon} label={t(mode)} size="large" pressed={!master && view.mode === mode} onClick={() => void switchMode(mode)} data-testid={`slides-view-${mode}`} />)}
      <RibbonButton primary icon={BookOpen} label={t('reading')} size="large" onClick={() => view.present(session.state?.current ?? 0, { reading: true })} data-testid="slides-view-reading" />
    </RibbonGroup>
    <RibbonGroup label={t('group.masters')} icon={Layers}>
      <RibbonButton primary icon={Layers} label={t('slideMaster')} size="large" pressed={master} onClick={() => { view.setMode('normal'); void run(master ? 'view.closeMaster' : 'view.slideMaster', {}) }} data-testid="slides-view-master" />
      <RibbonButton icon={Presentation} label={t('handoutMaster')} size="large" disabled />
      <RibbonButton icon={StickyNote} label={t('notesMaster')} size="large" disabled />
    </RibbonGroup>
    <RibbonGroup label={t('group.show')} icon={Grid2X2}>
      <RibbonStack>{(['ruler', 'gridlines', 'guides'] as const).map((key) => <label className="slides-field-row" key={key}>
        <input type="checkbox" checked={view[key]} onChange={(event) => ({ ruler: view.setRuler, gridlines: view.setGridlines, guides: view.setGuides }[key])(event.target.checked)} data-testid={`slides-view-${key}`} />{t(key)}
      </label>)}</RibbonStack>
      <RibbonButton primary icon={Grid2X2} label={t('gridSettings')} size="large" onClick={() => view.setGridDialog(true)} data-testid="slides-view-gridSettings" />
      <RibbonButton primary icon={StickyNote} label={t('notes')} size="large" pressed={view.notesOpen} onClick={() => view.setNotesOpen(!view.notesOpen)} />
    </RibbonGroup>
    <RibbonGroup label={t('group.zoom')} icon={Search}>
      <RibbonMenu primary icon={Search} label={t('zoom')} size="large" testId="slides-view-zoom">{[400, 200, 150, 100, 75, 66, 50, 33].map((percent) => <MenuItem key={percent} label={`${percent}%`} onSelect={() => canvas.current?.zoomTo(percent / 100)} />)}</RibbonMenu>
      <RibbonButton primary icon={Maximize2} label={t('fit')} size="large" onClick={() => canvas.current?.fit()} data-testid="slides-view-fit" />
    </RibbonGroup>
    <RibbonGroup label={t('group.color')} icon={Palette}>
      <RibbonButton icon={Palette} label={t('grayscale')} size="large" pressed={view.grayscale} onClick={() => view.setGrayscale(!view.grayscale)} data-testid="slides-view-grayscale" />
      <RibbonButton icon={Moon} label={t('darkMode')} size="large" pressed={view.dark} onClick={() => view.setDark(!view.dark)} data-testid="slides-view-dark" />
    </RibbonGroup>
  </>
}
