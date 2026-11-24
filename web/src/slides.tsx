import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState } from 'react'
import type { KitCommentAnchor, KitShownThread } from '@kition/sdk/protocol'
import { useKitEditorState, useKitSession, usePresence, useRemoteSelections, type HostConnection, type KitEngineInfo, type KitPresence, type KitModuleManifest, type PresenceRect, type Strings } from '@kition/sdk/editor'
import { useSlideView } from './useSlideView'
import { canvasColor } from './backdrop'
import kit from '../../kit.json'

/**
 * The engine, from this package's own folder: one slide at a time, on the host's canvas color.
 * Called after the host's theme is applied.
 */
export function slidesManifest(commentOnlyAs: string | null = null): KitModuleManifest {
  const canvas = canvasColor()
  return {
    kitId: 'kit.slide',
    base: new URL('./engine/', document.baseURI).href,
    module: 'kition_slide_kit',
    documentClass: 'SlideDocument',
    // The web build embeds only a Japanese CJK face and Inter: build-engine.sh copies these from
    // kition-fonts, Chinese and the metric-compatible stand-ins for Office's fonts (decision F1).
    fonts: [
      'DroidSansFallbackFull.ttf',
      'Carlito-Regular.ttf',
      'Carlito-Bold.ttf',
      'Carlito-Italic.ttf',
      'Carlito-BoldItalic.ttf',
      'Caladea-Regular.ttf',
      'Caladea-Bold.ttf',
      'Arimo-Variable.ttf',
      'Arimo-Italic-Variable.ttf',
      'Tinos-Regular.ttf',
      'Tinos-Bold.ttf',
      'Cousine-Regular.ttf',
      'Cousine-Bold.ttf',
    ].map((file) => new URL(`./fonts/${file}`, document.baseURI).href),
    setup: [
      { method: 'setSingle', args: [true] },
      ...(canvas ? [{ method: 'setBackdrop', args: [canvas] }] : []),
      ...(commentOnlyAs !== null ? [{ method: 'setCommentOnly', args: [commentOnlyAs] }] : []),
    ],
    // What a commenter's read-only session runs (kit.json `collaboration.comments.commands`).
    commentCommands: kit.collaboration.comments.commands,
  }
}

/** Where a comment goes: a slide (its index), and the shape on it when there is one. */
export type CommentAnchor = { object: string; slide: number; x: number; y: number; shape?: number; [key: string]: unknown }

/** Who comments here and how (Kit UI protocol `document.comments`, Kit Standard 2.0 rule M23). */
export type CommentAccess = { me: string | null; name?: string; canComment: boolean; othersReadOnly: boolean }

/**
 * The host's `document.comments`; without it the person is unknown, may comment unless the
 * document is read-only, and others' threads are writable.
 */
export function commentAccess(host: HostConnection): CommentAccess {
  const given = host.init.document.comments
  if (!given) return { me: null, canComment: host.readOnly !== true, othersReadOnly: false }
  return { me: given.me ?? null, ...(given.name ? { name: given.name } : {}), canComment: given.canComment === true, othersReadOnly: given.othersReadOnly === true }
}

/**
 * A commenter's document is read-only for its content yet takes comments: the session stays
 * read-only, and kit-editor lets the manifest's `commentCommands` through as long as they change only
 * comment objects. The engine, told to (`setCommentOnly`), writes them as the commenter.
 */
function isCommenter(host: HostConnection, access: CommentAccess) {
  return host.readOnly === true && access.canComment
}

export type SlideArea = { x0: number; y0: number; x1: number; y1: number }

/** What the editor reads from the engine's `inspect()`. */
export type SlidesInfo = {
  slides: number
  current: number
  /** The current slide on the canvas. */
  area: SlideArea | null
  selected: number
  editingText: boolean
  selectedKinds: string[]
  target: { kind: 'slides' | 'master' | 'layout'; master?: number; layout?: number }
  masters: Array<{ index: number; name: string; layouts: Array<{ index: number; name: string }> }>
}

function describe(info: KitEngineInfo): SlidesInfo {
  const inspected = info.inspected as { slides?: Array<{ area?: { x: number; y: number; width: number; height: number } | null }>; slide?: number; selection?: number; editingText?: boolean; selectedKinds?: string[]; target?: SlidesInfo['target']; masters?: SlidesInfo['masters'] }
  const slides = inspected.slides ?? []
  const current = inspected.slide ?? 0
  const area = slides[current]?.area
  return {
    slides: slides.length,
    current,
    area: area ? { x0: area.x, y0: area.y, x1: area.x + area.width, y1: area.y + area.height } : null,
    selected: inspected.selection ?? 0,
    editingText: inspected.editingText === true,
    selectedKinds: inspected.selectedKinds ?? [],
    target: inspected.target ?? { kind: 'slides' },
    masters: inspected.masters ?? [],
  }
}

/** `format.state`: the formatting at the selection. */
export type FormatState = {
  font?: string
  size?: number | null
  bold?: boolean
  italic?: boolean
  underline?: boolean
  strike?: boolean
  superscript?: boolean
  subscript?: boolean
  bullets?: boolean
  numbering?: boolean
  align?: string | null
  painter?: boolean
}

/** `document.inspect`: the slides' layouts, slides, sections, and fonts. */
export type DocumentFacts = {
  layouts: Array<{ id: number; name: string }>
  slides: Array<{ id: number; index: number; hidden: boolean; layout?: string; title?: string }>
  sections: Array<{ name: string; start?: number; slides?: number[] }>
  fonts?: { major: string; minor: string }
  selection?: { shapes: number[] }
}

/** `comment.threads`: the slide file's comment threads (M23 shape), each with its slide and point. */
export type SlideThread = {
  id: string
  slide: number
  anchor: CommentAnchor & { box?: { x: number; y: number; width: number; height: number } }
  resolved: boolean
  comments: Array<{ id: string; author: string; name?: string; body: string; created: string }>
}

/**
 * A host's comment thread as this editor marks it (Kit UI protocol 2.6 section 7k): where its
 * anchor is now (`comment.place`), in slide points. `rect` is its shape's box, `point` the point it
 * was left at; neither, the slide itself. `slide` null: nothing of it is left to mark.
 */
export type HostMark = KitShownThread & { slide: number | null; rect: { x: number; y: number; width: number; height: number } | null; point: { x: number; y: number } | null }

const SESSION_OPTIONS = { replicaMergesFile: true } as const

type Placed = { found?: boolean; slide?: number | null; rect?: HostMark['rect']; point?: HostMark['point'] }

/** The protocol's plain-text rule (UI-V6) for a label: one line, no control or bidirectional formatting characters. */
// eslint-disable-next-line no-control-regex
const labelText = (text: string) => text.replace(/[\u0000-\u001f\u007f-\u009f\u202a-\u202e\u2066-\u2069\u200e\u200f\u061c]+/g, ' ').trim().slice(0, 500)

function useSlidesState(host: HostConnection, t: Strings) {
  const access = useMemo(() => commentAccess(host), [host])
  const commenter = isCommenter(host, access)
  const manifest = useMemo(() => slidesManifest(commenter ? (access.me ?? '') : null), [commenter, access])
  // The engine merges a file changed outside Kition into its stored replica as one `import`
  // change (`open_with_replica`), so the replica opens whatever the file's stamp (UI-R1).
  const session = useKitSession(host, manifest, describe, SESSION_OPTIONS)
  const editor = useKitEditorState(session, host, t, 'slides')
  const view = useSlideView()
  const [threads, setThreads] = useState<SlideThread[]>([])
  const [activeThread, setActiveThread] = useState<string | null>(null)
  /** Where a new comment goes, while its composer is open. */
  const [pendingComment, setPendingComment] = useState<CommentAnchor | null>(null)
  const { doc, change } = session
  const { query } = editor
  const [format, setFormat] = useState<FormatState>({})
  const [facts, setFacts] = useState<DocumentFacts>({ layouts: [], slides: [], sections: [] })
  const [notes, setNotes] = useState('')

  // After every change: the selection's formatting, the slides' facts, the slide's notes.
  const current = session.state?.current ?? 0
  useEffect(() => {
    if (!doc) return
    let stale = false
    void (async () => {
      const [state, inspected, slide, listed] = await Promise.all([
        query<FormatState>('format.state'),
        query<DocumentFacts>('document.inspect'),
        query<{ notes?: string }>('slide.inspect', { index: current, select: false }),
        query<SlideThread[]>('comment.threads'),
      ])
      if (stale) return
      setThreads(Array.isArray(listed) ? listed : [])
      setFormat(state ?? {})
      if (inspected) setFacts({ layouts: inspected.layouts ?? [], slides: inspected.slides ?? [], sections: inspected.sections ?? [], fonts: inspected.fonts, selection: inspected.selection })
      setNotes(slide?.notes ?? '')
    })()
    return () => {
      stale = true
    }
  }, [change.revision, current, doc, query])

  // Comment threads the host keeps (Kit UI protocol 2.6 section 7k, capability `comment-anchors`):
  // the host sends them all each time, the editor places each anchor on its slide now, and the
  // host asks whether anchors are still found.
  const hostComments = host.commentAnchors
  const canHostComment = hostComments && host.hasService('comment.create')
  const [hostThreads, setHostThreads] = useState<KitShownThread[]>([])
  const [hostMarks, setHostMarks] = useState<HostMark[]>([])
  const [activeHostThread, setActiveHostThread] = useState<string | null>(null)
  const latestQuery = useRef(query)
  latestQuery.current = query
  useEffect(() => (hostComments ? host.onComments(setHostThreads) : undefined), [host, hostComments])
  const placeAnchors = useCallback(async (anchors: KitCommentAnchor[]) => {
    if (!anchors.length) return [] as Placed[]
    const placed = await latestQuery.current<Placed[]>('comment.place', { anchors })
    return Array.isArray(placed) ? placed : []
  }, [])
  useEffect(() => {
    if (!hostComments || !doc) return
    host.locateComments(async (anchors) => {
      const placed = await placeAnchors(anchors)
      return anchors.map((_, index) => placed[index]?.found === true)
    })
  }, [host, hostComments, doc, placeAnchors])
  useEffect(() => {
    if (!doc || !hostThreads.length) {
      setHostMarks([])
      return
    }
    let stale = false
    void placeAnchors(hostThreads.map((thread) => thread.anchor)).then((placed) => {
      if (stale) return
      setHostMarks(hostThreads.map((thread, index) => {
        const at = placed[index]
        return { ...thread, slide: typeof at?.slide === 'number' ? at.slide : null, rect: at?.rect ?? null, point: at?.point ?? null }
      }))
    })
    return () => {
      stale = true
    }
  }, [hostThreads, change.revision, doc, placeAnchors])
  /** The person clicked a thread's marker: the host opens that thread (`comments/activated`). */
  const openHostThread = (id: string) => {
    setActiveHostThread(id)
    host.activateComment(id)
  }
  /** Where the person last pressed on the current slide with nothing under the pointer, in slide points. */
  const commentPoint = useRef<{ slide: number; x: number; y: number } | null>(null)

  // Presence (Kit UI protocol section 7e): the selected shapes as KDM object ids, shared with the
  // other people in the slide file, and where the shapes they selected are on the current slide.
  const [local, setLocal] = useState<KitPresence | null>(null)
  useEffect(() => {
    if (!doc) return
    let stale = false
    void query<{ slide?: string | null; objects?: string[] }>('presence.selection').then((shared) => {
      if (stale || !shared) return
      setLocal({ selection: shared.objects ?? [], ...(shared.slide ? { viewport: { slide: shared.slide } } : {}) })
    })
    return () => {
      stale = true
    }
  }, [change.revision, session.state, doc, query])
  const { peers } = usePresence(host, local)
  const wanted = useMemo(() => [...new Set(peers.flatMap((peer) => peer.selection))], [peers])
  const [boxes, setBoxes] = useState<Map<string, PresenceRect>>(() => new Map())
  useEffect(() => {
    if (!doc || !wanted.length) {
      setBoxes(new Map())
      return
    }
    let stale = false
    void query<Array<PresenceRect & { id: string }>>('presence.locate', { ids: wanted }).then((placed) => {
      if (!stale) setBoxes(new Map((Array.isArray(placed) ? placed : []).map(({ id, ...rect }) => [id, rect])))
    })
    return () => {
      stale = true
    }
  }, [wanted, current, change.revision, doc, query])
  const locate = useCallback((id: string) => boxes.get(id) ?? null, [boxes])
  const remoteSelections = useRemoteSelections(peers, locate)

  const run: typeof editor.run = async (id, params) => {
    // With a host that keeps the threads, New Comment (its shortcut, More Commands) asks the host.
    if (id === 'comment.add' && canHostComment && params === undefined) {
      await newComment()
      return null
    }
    if (['media.play', 'media.pause', 'media.stop'].includes(id)) {
      view.setPane('media')
      view.setMediaAction((previous) => ({ kind: id.slice(6), revision: previous.revision + 1 }))
      return null
    }
    if (id === 'show.fromStart' || id === 'show.fromCurrent') {
      view.present(id === 'show.fromStart' ? 0 : current)
      return null
    }
    if (id === 'review.spelling' || id === 'review.accessibility') {
      view.setPane(id === 'review.spelling' ? 'spelling' : 'accessibility')
      return null
    }
    // Agent schemas keep conditional parameters optional. In the editor, a field-based
    // command still needs a form unless its native button intentionally uses defaults.
    const command = editor.byId.get(id)
    if (params === undefined && command && Object.keys(command.schema ?? {}).length > 0 && !['slide.new', 'slide.reset', 'view.slideMaster', 'master.insertLayout'].includes(id)) {
      editor.ask(id)
      return null
    }
    return editor.run(id, params)
  }
  /**
   * Comment commands go to the session itself, so a refusal reaches the caller: they are the edits
   * a commenter may make (the manifest's `commentCommands`), and the engine checks who may do what.
   */
  const comment = async (id: 'comment.add' | 'comment.reply' | 'comment.resolve' | 'comment.delete', params: Record<string, unknown>) => {
    const who = { ...(access.me ? { author: access.me } : {}), ...(access.name ? { name: access.name } : {}), created: new Date().toISOString() }
    const written = id === 'comment.add' || id === 'comment.reply'
    return (await session.run(id, written ? { ...params, ...who } : params)) as { thread?: string } | null
  }
  /**
   * A new comment on the selected text or shapes, else the point last pressed on the slide, else the
   * slide. A host that keeps comment threads gets it (`comment.create` with the anchor; the host
   * asks for the text and keeps the thread); otherwise the comments pane opens a thread in the file.
   */
  const newComment = async () => {
    if (canHostComment) {
      const point = commentPoint.current?.slide === current && !facts.selection?.shapes?.length ? commentPoint.current : null
      // A read-only session without a replica yet has no anchor (making one would be an edit).
      const anchor = await query<KitCommentAnchor | null>('comment.anchor', point ? { x: point.x, y: point.y } : {}).catch(() => null)
      if (!anchor) return
      const title = typeof anchor.page === 'number' ? facts.slides[anchor.page]?.title : undefined
      const label = title ? labelText(title) : ''
      try {
        const { thread } = await host.service('comment.create', { anchor: label ? { ...anchor, label } : anchor })
        if (thread) setActiveHostThread(thread)
      } catch {
        // Cancelled or refused: the host said so itself.
      }
      return
    }
    const shape = facts.selection?.shapes?.[0]
    setPendingComment({ object: shape != null ? `shape:${shape}` : `slide:${current}`, slide: current, x: 10, y: 10, ...(shape != null ? { shape } : {}) })
    view.setPane('comments')
  }
  return {
    ...editor,
    run,
    session,
    format,
    facts,
    notes,
    view,
    threads,
    comment,
    access,
    commenter,
    activeThread,
    setActiveThread,
    pendingComment,
    setPendingComment,
    newComment,
    /** Whether the person may start a comment: through the host when it keeps threads, else in the file. */
    canNewComment: hostComments ? canHostComment : access.canComment,
    hostComments,
    hostMarks,
    activeHostThread,
    openHostThread,
    commentPoint,
    remoteSelections,
  }
}

export type Slides = ReturnType<typeof useSlidesState>

const SlidesContext = createContext<Slides | null>(null)

export function SlidesProvider({ host, t, children }: { host: HostConnection; t: Strings; children: (slides: Slides) => React.ReactNode }) {
  const slides = useSlidesState(host, t)
  return <SlidesContext.Provider value={slides}>{children(slides)}</SlidesContext.Provider>
}

export function useSlides() {
  const slides = useContext(SlidesContext)
  if (!slides) throw new Error('useSlides outside SlidesProvider')
  return slides
}
