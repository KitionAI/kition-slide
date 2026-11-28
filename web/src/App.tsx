import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from 'react'
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
} from '@kition/sdk/editor'
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
import { PictureFormatTab, ShapeFormatTab } from './ribbon/ShapeFormatTab'
import { ChartTab } from './ribbon/ChartTab'
import { MoreCommands } from './ribbon/MoreCommands'
import { CONTEXT_PRIMARY, RIBBON_MENUS } from './ribbon/menus'
import { FileTab } from './ribbon/FileTab'
import { ExportDialog } from './ExportDialog'
import { useEngineBackdrop } from './backdrop'
import { focusEditor, returnKeyboardLater, takeKeyboardOnOpen, useKeyboardFallback } from './focus'
import { slideKeys, usePopupEscape } from './editorKeys'
import { drawSlideHandle } from './handles'
import { LinkDialog } from './LinkDialog'
import { GridDialog } from './GridDialog'
import { TableDialog } from './ribbon/TableGrid'
import { useEngineSnapping } from './gridSettings'
import { ZoomBox } from './ZoomBox'
import { steppedCanvas } from './zoom'
import { useSystemClipboard } from './clipboard'
import { SlideContextMenu, type OpenMenu } from './ContextMenu'
import type { ContextTarget } from './contextMenus'

const HOST_TOOLS = { hand: HAND_TOOL }

/** Escape alone, the key that cancels. */
const plainEscape = (event: { key: string; shiftKey: boolean; altKey: boolean; metaKey: boolean; ctrlKey: boolean }) =>
  event.key === 'Escape' && !event.shiftKey && !event.altKey && !event.metaKey && !event.ctrlKey

/** The kinds of object Shape Format is for. */
const SHAPE_KINDS = ['shape', 'textBox', 'placeholder', 'connector', 'group']
/** The tabs that come and go with the selection; the others are the person's own choice. */
const CONTEXT_TABS = new Set(['shapeFormat', 'pictureFormat', 'tableDesign', 'tableLayout', 'chartDesign', 'playback', 'slideMaster'])

/**
 * The tab the selection brings forward, as WPS does (W-A15): a table's, a chart's, a media
 * object's, a picture's, or a shape's tools; none while text is edited (its formatting is on Home,
 * where WPS shows its Text Tools) or with nothing selected.
 */
export function contextTab(kinds: readonly string[], editing: boolean): string | null {
  if (kinds.includes('table')) return 'tableDesign'
  if (kinds.includes('chart')) return 'chartDesign'
  if (kinds.some((kind) => ['media', 'audio', 'video'].includes(kind))) return 'playback'
  if (editing) return null
  if (kinds.some((kind) => SHAPE_KINDS.includes(kind))) return 'shapeFormat'
  if (kinds.includes('picture')) return 'pictureFormat'
  return null
}

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
  useEngineBackdrop(doc, canvas)
  // Grid and guides: the person's settings steer the engine's snapping and arrow keys (plan stage 8).
  useEngineSnapping(doc, view.grid)
  // The system clipboard (plan stage 5): its key listener goes before the keyboard's safety net.
  useSystemClipboard({ run: slides.plainRun, query: slides.query, inOrder: session.inOrder, doc, readOnly: slides.host.readOnly === true, editingText: state?.editingText === true, textSelected: state?.textSelected === true, area: state?.area ?? null, canvas, t })
  useKeyboardFallback()
  usePopupEscape()
  // The slides just opened have the keyboard (walkthrough K1).
  useEffect(() => {
    if (doc) takeKeyboardOnOpen()
  }, [doc])
  const area = useRef(state?.area)
  area.current = state?.area
  // Fit fits the slide, and zoom steps go 10% at a time, from every entry (zoom.ts).
  const zoomCanvas = useMemo(() => steppedCanvas(canvas, () => area.current), [])
  const keys = slideKeys(slides, zoomCanvas)
  const [zoom, setZoom] = useState(1)
  const [tab, setTab] = useState('home')
  /** The last tab the person chose that is not a contextual one: where a contextual tab goes back to. */
  const ownTab = useRef('home')
  const chooseTab = useCallback((id: string) => {
    setTab(id)
    if (!CONTEXT_TABS.has(id)) ownTab.current = id
  }, [])
  // Selecting an object brings its tab forward, and clearing the selection goes back to the tab
  // the person had (WPS W-A15); only when what the selection asks for changes, so a tab the person
  // picks meanwhile stays. A read-only document keeps its tab.
  const wanted = slides.host.readOnly === true ? null : contextTab(state?.selectedKinds ?? [], state?.editingText === true)
  const lastWanted = useRef<string | null>(null)
  useEffect(() => {
    if (wanted === lastWanted.current) return
    lastWanted.current = wanted
    setTab(wanted ?? ownTab.current)
  }, [wanted])
  const [exporting, setExporting] = useState(false)
  // The slide's context menu (plan stage 6A): open while set. Closing it gives the slide the
  // keyboard again, the text being edited first.
  const [menu, setMenu] = useState<OpenMenu | null>(null)
  const closeMenu = useCallback(() => {
    setMenu(null)
    focusEditor(slides.session.state?.editingText === true)
  }, [slides.session.state?.editingText])
  // Format Object… brings the object's own tab forward (stage 1), Shape Format for text.
  const formatObject = useCallback((target: ContextTarget) => chooseTab(contextTab(target.kinds, false) ?? 'shapeFormat'), [chooseTab])
  const engineTools = useEngineTools(session.tool, doc != null, change, { initial: 'select', hostTools: HOST_TOOLS, drawOverlay: drawSlideHandle })
  const endShow = view.endShow
  // The show ended: the editor has the keyboard again (walkthrough K2).
  const showEnded = useCallback(() => {
    endShow()
    returnKeyboardLater()
  }, [endShow])
  // Escape cancels first (KitPointer 4.4): the engine's tool has it while text is edited, shapes
  // are selected or a gesture is under way; else a tool other than Select gives way to Select,
  // as PowerPoint puts a shape tool down. Only an Escape with nothing to cancel is left untaken,
  // so in Focus it leaves Focus. An open menu or panel takes Escape before all of them (usePopupEscape).
  const tools = {
    ...engineTools,
    keyDown: (event: Parameters<typeof engineTools.keyDown>[0]) => {
      if (keys.beforeTool(event, engineTools.keyDown)) return true
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
    { id: 'view', label: t('tab.view'), view: true, ribbon: more('view', <ViewTab canvas={zoomCanvas} />) },
    ...(kinds.some((kind) => SHAPE_KINDS.includes(kind)) || state?.editingText ? [{ id: 'shapeFormat', label: t('tab.shapeFormat'), ribbon: <ShapeFormatTab /> }] : []),
    ...(kinds.includes('picture') ? [{ id: 'pictureFormat', label: t('tab.pictureFormat'), ribbon: <PictureFormatTab /> }] : []),
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
  const activeTab = tabs.some((item) => item.id === tab) ? tab : tabs.some((item) => item.id === ownTab.current) ? ownTab.current : 'home'

  return (
    <div className="slides-editor" data-dark={view.dark}>
    <KitEditorProvider editor={slides} tools={tools}>
      <EditorFrame
        tabs={tabs}
        tab={activeTab}
        onTab={chooseTab}
        canvas={zoomCanvas}
        onKeyDown={keys.afterTool}
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
          <>
            <Button variant="ghost" aria-pressed={view.notesOpen} onClick={() => view.setNotesOpen((open) => !open)}>
              {t('notes')}
            </Button>
            {doc ? <ZoomBox canvas={zoomCanvas} zoom={zoom} /> : null}
          </>
        }
        sidebar={view.mode !== 'sorter' ? ((state?.target.kind ?? 'slides') !== 'slides' ? <MasterPanel /> : <SlidesPanel />) : null}
        panels={<TaskPane />}
      >
        <div className="kui-row">
          <div className="slides-main" ref={canvasRegion}>
            {view.mode === 'sorter' ? <SlidesPanel /> : view.mode === 'notesPage' ? <NotesPage /> : <>
              <SlideCanvas canvas={canvas} viewMemory={viewMemory} onZoom={setZoom} onMenu={setMenu} />
              {view.notesOpen ? <FrameChrome><NotesPane /></FrameChrome> : null}
            </>}
          </div>
        </div>
        {exporting ? <ExportDialog onClose={() => setExporting(false)} /> : null}
        {slides.linking ? <LinkDialog onClose={() => slides.setLinking(false)} /> : null}
        {view.gridDialog ? <GridDialog onClose={() => view.setGridDialog(false)} /> : null}
        {view.tableDialog ? <TableDialog onClose={() => view.setTableDialog(false)} /> : null}
        {menu ? <SlideContextMenu menu={menu} onClose={closeMenu} onFormat={formatObject} /> : null}
        {view.showing != null && doc ? <SlideShow doc={doc} from={view.showing.from} slides={state?.slides ?? 1} onEnd={showEnded} hint={t('endShow')} options={view.showing} /> : null}
      </EditorFrame>
    </KitEditorProvider>
    </div>
  )
}
