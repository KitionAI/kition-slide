import { useMemo, useRef, useState, type ReactNode } from 'react'
import {
  Button,
  HAND_TOOL,
  KitEditorProvider,
  EditorFrame,
  FrameChrome,
  createKitStrings,
  useEngineTools,
  type FrameTab,
  type HostConnection,
  type KitCanvasHandle,
  type RasterView,
} from '@kition/kit-editor'
import { SlidesProvider, type Slides } from './slides'
import { CATALOGS } from './strings'
import { SlidesPanel } from './SlidesPanel'
import { NotesPane } from './NotesPane'
import { SlideShow } from './SlideShow'
import { HomeTab } from './ribbon/HomeTab'
import { InsertTab } from './ribbon/InsertTab'
import { DrawTab } from './ribbon/DrawTab'
import { DesignTab } from './ribbon/DesignTab'
import { TransitionsTab } from './ribbon/TransitionsTab'
import { SlideShowTab } from './ribbon/SlideShowTab'
import { ViewTab } from './ribbon/ViewTab'
import { useSlideWheel } from './useSlideWheel'
import { SlideCanvas } from './SlideCanvas'
import { NotesPage } from './NotesPage'
import { MasterPanel } from './MasterPanel'
import { TaskPane } from './TaskPane'
import { AnimationsTab } from './ribbon/AnimationsTab'
import { ReviewTab } from './ribbon/ReviewTab'
import { TableTab } from './ribbon/TableTab'
import { ContextTab } from './ribbon/ContextTab'
import { ChartTab } from './ribbon/ChartTab'
import { MoreCommands } from './ribbon/MoreCommands'
import { CONTEXT_PRIMARY, RIBBON_MENUS } from './ribbon/menus'
import { FileTab } from './ribbon/FileTab'
import { ExportDialog } from './ExportDialog'

const HOST_TOOLS = { hand: HAND_TOOL }

/** Escape alone, the key that cancels. */
const plainEscape = (event: { key: string; shiftKey: boolean; altKey: boolean; metaKey: boolean; ctrlKey: boolean }) =>
  event.key === 'Escape' && !event.shiftKey && !event.altKey && !event.metaKey && !event.ctrlKey

export function App({ host }: { host: HostConnection }) {
  const t = useMemo(() => createKitStrings(CATALOGS, host.init.locale), [host])
  return <SlidesProvider host={host} t={t}>{(slides) => <Editor slides={slides} />}</SlidesProvider>
}

function Editor({ slides }: { slides: Slides }) {
  const { session, t, view } = slides
  const { doc, change, state } = session
  const canvas = useRef<KitCanvasHandle>(null)
  const canvasRegion = useRef<HTMLDivElement>(null)
  const viewMemory = useRef<RasterView | null>(null)
  useSlideWheel(canvasRegion, canvas, viewMemory, slides)
  const [zoom, setZoom] = useState(1)
  const [tab, setTab] = useState('home')
  const [exporting, setExporting] = useState(false)
  const engineTools = useEngineTools(session.tool, doc != null, change, { initial: 'select', hostTools: HOST_TOOLS })
  // Escape cancels first (KitPointer 4.4): the engine's tool has it while text is edited, shapes
  // are selected or a gesture is under way; else a tool other than Select gives way to Select,
  // as PowerPoint puts a shape tool down. Only an Escape with nothing to cancel is left untaken,
  // so in Focus it leaves Focus.
  const tools = {
    ...engineTools,
    keyDown: (event: Parameters<typeof engineTools.keyDown>[0]) => {
      if (engineTools.keyDown(event)) return true
      if (!plainEscape(event) || engineTools.active === 'select') return false
      engineTools.select('select')
      return true
    },
  }

  /** A tab's own groups, then the engine's commands under its menus that no group shows. */
  const more = (id: string, content: ReactNode = null) => (
    <>
      {content}
      <MoreCommands menus={RIBBON_MENUS[id] ?? []} />
    </>
  )
  const contextual = (id: string) => more(id, <ContextTab menu={RIBBON_MENUS[id][0]} extra={RIBBON_MENUS[id].slice(1)} primary={CONTEXT_PRIMARY[id]} />)
  const kinds = state?.selectedKinds ?? []
  const tabs: FrameTab[] = [
    // File: Export (the host keeps Save, Open, and Close).
    { id: 'file', label: t('tab.file'), view: true, ribbon: <FileTab onExport={() => setExporting(true)} /> },
    { id: 'home', label: t('tab.home'), ribbon: more('home', <HomeTab />) },
    { id: 'insert', label: t('tab.insert'), ribbon: more('insert', <InsertTab />) },
    { id: 'draw', label: t('tab.draw'), ribbon: more('draw', <DrawTab />) },
    { id: 'design', label: t('tab.design'), ribbon: more('design', <DesignTab />) },
    { id: 'transitions', label: t('tab.transitions'), ribbon: more('transitions', <TransitionsTab />) },
    { id: 'animations', label: t('tab.animations'), ribbon: more('animations', <AnimationsTab />) },
    { id: 'slideShow', label: t('tab.slideShow'), view: true, ribbon: more('slideShow', <SlideShowTab />) },
    { id: 'record', label: t('tab.record'), ribbon: more('record', <SlideShowTab record />) },
    { id: 'review', label: t('tab.review'), view: true, ribbon: more('review', <ReviewTab />) },
    { id: 'view', label: t('tab.view'), view: true, ribbon: more('view', <ViewTab canvas={canvas} />) },
    ...(kinds.some((kind) => ['shape', 'textBox', 'placeholder', 'connector', 'group'].includes(kind)) || state?.editingText ? [{ id: 'shapeFormat', label: t('tab.shapeFormat'), ribbon: contextual('shapeFormat') }] : []),
    ...(kinds.includes('picture') ? [{ id: 'pictureFormat', label: t('tab.pictureFormat'), ribbon: contextual('pictureFormat') }] : []),
    ...(kinds.includes('table')
      ? [
          { id: 'tableDesign', label: t('tab.tableDesign'), ribbon: more('tableDesign', <TableTab design />) },
          { id: 'tableLayout', label: t('tab.tableLayout'), ribbon: more('tableLayout', <TableTab />) },
        ]
      : []),
    ...(kinds.includes('chart') ? [{ id: 'chartDesign', label: t('tab.chartDesign'), ribbon: more('chartDesign', <ChartTab />) }] : []),
    ...(kinds.some((kind) => ['media', 'audio', 'video'].includes(kind)) ? [{ id: 'playback', label: t('tab.playback'), ribbon: contextual('playback') }] : []),
    ...((state?.target.kind ?? 'slides') !== 'slides' ? [{ id: 'slideMaster', label: t('tab.slideMaster'), ribbon: contextual('slideMaster') }] : []),
  ]
  const activeTab = tabs.some((item) => item.id === tab) ? tab : 'home'

  return (
    <div className="slides-editor" data-dark={view.dark}>
    <KitEditorProvider editor={slides} tools={tools}>
      <EditorFrame
        tabs={tabs}
        tab={activeTab}
        onTab={setTab}
        canvas={canvas}
        zoom={zoom}
        status={
          <>
            <span data-testid="slides-status">{state ? t('slideOf', { slide: state.current + 1, slides: state.slides }) : ''}</span>
            {state && state.selected > 0 ? <span>{t('selected', { count: state.selected })}</span> : null}
            {slides.remoteSelections.filter((item) => item.label).map((item) => (
              <span key={item.principal} className="slides-peer-selection" style={{ borderColor: item.color }} data-testid="slides-peer-selection" data-peer={item.name}>
                {t('peerSelected', { name: item.name, count: slides.remoteSelections.filter((other) => other.principal === item.principal).length })}
              </span>
            ))}
          </>
        }
        statusEnd={
          <Button variant="ghost" aria-pressed={view.notesOpen} onClick={() => view.setNotesOpen((open) => !open)}>
            {t('notes')}
          </Button>
        }
        sidebar={view.mode !== 'sorter' ? ((state?.target.kind ?? 'slides') !== 'slides' ? <MasterPanel /> : <SlidesPanel />) : null}
        panels={<TaskPane />}
      >
        <div className="kui-row">
          <div className="slides-main" ref={canvasRegion}>
            {view.mode === 'sorter' ? <SlidesPanel /> : view.mode === 'notesPage' ? <NotesPage /> : <>
              <SlideCanvas canvas={canvas} viewMemory={viewMemory} onZoom={setZoom} />
              {view.notesOpen ? <FrameChrome><NotesPane /></FrameChrome> : null}
            </>}
          </div>
        </div>
        {exporting ? <ExportDialog onClose={() => setExporting(false)} /> : null}
        {view.showing != null && doc ? <SlideShow doc={doc} from={view.showing.from} slides={state?.slides ?? 1} onEnd={view.endShow} hint={t('endShow')} options={view.showing} /> : null}
      </EditorFrame>
    </KitEditorProvider>
    </div>
  )
}
