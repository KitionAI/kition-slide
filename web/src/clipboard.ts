/**
 * The system clipboard, and pictures coming in by paste or drop (plan stage 5, walkthrough J2,
 * F8; WPS W-I6). Copy, Cut and Paste used to stay in the engine's own clipboard: nothing copied
 * here reached another app, and text or a picture copied in another app never pasted.
 *
 * Cmd+C, Cmd+X and Cmd+V keep the browser's own action (kit-editor's shortcut, which cancels it,
 * does not run), so the page gets its `copy`, `cut` and `paste` events: they carry the system
 * clipboard and need no permission, in a frame too. Every command they start runs in the input
 * order, after the keys and clicks before it.
 *
 * - Copy and Cut run `edit.copy` / `edit.cut`, and put a mark of this copy on the clipboard at
 *   once (its own type, which other apps leave alone); the plain text the command answers (the
 *   selected characters, or the text of the selected shapes) follows on the clipboard for other
 *   apps, written from inside the person's key. With only a caret in the text they take nothing
 *   and the clipboard keeps what it had.
 * - Paste: the clipboard this editor put there (its mark, or the text it wrote) is the engine's own
 *   copy, shapes and formatting and all (`edit.paste`). Else, while text is edited, the clipboard's
 *   text goes in at the caret; else a picture on the clipboard is inserted on the slide (its pixels
 *   at 0.75 pt each, in the middle of the slide, as WPS does); else its text becomes a new text box.
 * - Cmd+Alt+Shift+V pastes plain text only (`edit.pasteText`); Cmd+Shift+V is the Format Painter's.
 *
 * The ribbon's Copy and Cut send the same events (`document.execCommand`); its Paste reads the
 * clipboard only where the browser lets the page do so without asking, else it pastes the engine's
 * copy. A key whose event the browser never sends runs as the command.
 *
 * Picture files dropped on the slide are inserted where they are dropped (their middle there, kept
 * on the slide), each one undo step. The bytes are the dropped file's, which the page was given;
 * nothing is read or written behind the host's back.
 */
import { useEffect, useState, type RefObject } from 'react'
import { base64, isMac, toast, type KitCanvasHandle, type KitEngine, type Strings } from '@kition/sdk/editor'
import { composingKey } from './inputMethod'
import { editorRoot, focusEditor, keyboardLost } from './focus'

/** The clipboard type of this editor's mark: which copy of which page the clipboard holds. */
export const CLIPBOARD_TYPE = 'application/x-kition-slide'
/** The most a picture pasted or dropped may be, in bytes. */
const MOST_BYTES = 64 * 1024 * 1024
/** The most text pasted at once, in characters. */
const MOST_TEXT = 1_000_000
/** A picture's pixel is 0.75 pt (96 per inch), as the engine and WPS size pictures. */
const POINTS_PER_PIXEL = 0.75
/** A picture larger than this share of the slide is made smaller to fit (as the engine does). */
const MOST_OF_SLIDE = 0.9

type Kind = 'copy' | 'cut' | 'paste' | 'pasteText'
/** What a paste carries: the clipboard's plain text, this editor's mark, and the pictures on it. */
type Pasted = { text: string; mark: string; images: File[] }
type Run = (id: string, params?: Record<string, unknown>, options?: { inOrder: true }) => Promise<string | null>
type Area = { x0: number; y0: number; x1: number; y1: number }

/** What the clipboard needs from the editor (`useSystemClipboard`). */
export type ClipboardEditor = {
  /** The editor's run, not this module's (the frame's error reports, the read-only rules). */
  run: Run
  inOrder: <R>(work: () => Promise<R>) => Promise<R>
  doc: KitEngine | null
  readOnly: boolean
  editingText: boolean
  /** Characters are selected in the text being edited (not only a caret). */
  textSelected: boolean
  area: Area | null
  canvas: RefObject<KitCanvasHandle | null>
  t: Strings
}

let latest: ClipboardEditor | null = null

const EMPTY: Pasted = { text: '', mark: '', images: [] }
/** This page's copies are named `<page>:<n>`, so a copy of another window is not taken for this one's. */
const PAGE = Math.random().toString(36).slice(2, 10)
let copies = 0
/** What this editor put on the clipboard last: its mark and the text it wrote there. */
let mine: { mark: string; texts: string[] } | null = null
/** The copy or cut on its way to the engine: a paste waits for it. */
let copying: Promise<unknown> = Promise.resolve()
/** The command a copy or cut event started, for the ribbon's Copy that sent the event. */
let eventCommand: Promise<string | null> | null = null
/** Keys whose clipboard event has not come yet (the browser may never send it). */
const waiting: Partial<Record<Kind, number>> = {}

/** A field that takes text itself (its copy and paste are its own), not kit-editor's text sink. */
function ownField(target: EventTarget | null) {
  if (target instanceof HTMLInputElement) return !['checkbox', 'radio', 'button', 'submit', 'reset', 'range', 'color', 'file', 'image'].includes(target.type)
  if (target instanceof HTMLTextAreaElement) return !target.classList.contains('kui-text-sink')
  return target instanceof HTMLSelectElement || (target instanceof HTMLElement && target.isContentEditable)
}

/** Whether a clipboard event or key at `target` is the editor's: not a field's, a dialog's or the slide show's. */
function editorsEvent(target: EventTarget | null) {
  if (!latest?.doc || ownField(target) || ownField(document.activeElement)) return false
  if (document.querySelector('.kui-dialog-backdrop, .slides-show')) return false
  return !(target instanceof Element && target.closest('.kui-dialog'))
}

/** Whether the editor itself has the keyboard: its frame, the canvas, or the text sink. */
function editorFocused() {
  const active = document.activeElement
  return active === editorRoot() || (active instanceof Element && (active.classList.contains('kui-canvas-surface') || active.classList.contains('kui-text-sink')))
}

/** Text selected on the page itself (a panel's words) while the slide does not have the keyboard: its copy is the browser's. */
function pageTextSelected() {
  if (editorFocused()) return false
  const range = document.getSelection()
  return !!range && !range.isCollapsed && range.toString() !== ''
}

/** The text a copy or cut answered (`{text}`), or '' when it answered none. */
function copiedText(answer: string | null) {
  if (answer == null) return ''
  try {
    const text = (JSON.parse(answer) as { text?: unknown })?.text
    return typeof text === 'string' ? text : ''
  } catch {
    return ''
  }
}

/**
 * Puts `text` (when it comes) on the system clipboard, after the event's own data: started from
 * inside the person's key or click, so the browser counts it as their copy. Answers whether it got there.
 */
function writeLater(text: Promise<string>): Promise<boolean> {
  const clipboard = navigator.clipboard
  const remembered = text.then((value) => {
    if (!value) throw new Error('nothing to write')
    if (mine && !mine.texts.includes(value)) mine.texts.push(value)
    return value
  })
  // Nothing to write is no error: the clipboard keeps the mark.
  remembered.catch(() => undefined)
  try {
    if (clipboard?.write && typeof ClipboardItem !== 'undefined') {
      const blob = remembered.then((value) => new Blob([value], { type: 'text/plain' }))
      return clipboard.write([new ClipboardItem({ 'text/plain': blob })]).then(() => true, () => false)
    }
    if (clipboard?.writeText) return remembered.then((value) => clipboard.writeText(value).then(() => true)).catch(() => false)
  } catch {
    // A browser without the asynchronous clipboard.
  }
  return Promise.resolve(false)
}

/** Runs `edit.copy` or `edit.cut` without a clipboard event; its text goes on the clipboard where the browser allows. */
function copyWithoutEvent(editor: ClipboardEditor, kind: 'copy' | 'cut') {
  mine = { mark: '', texts: [] }
  const started = editor.run(kind === 'cut' ? 'edit.cut' : 'edit.copy', {})
  const text = started.then(copiedText, () => '')
  copying = text
  void writeLater(text)
  return started
}

/** Whether text is edited with only a caret in it, as the engine's last answer has it: a copy then takes nothing. */
function caretOnly(editor: ClipboardEditor) {
  const inspected = editor.doc?.info?.inspected as { editingText?: unknown; textSelected?: unknown } | undefined
  return inspected ? inspected.editingText === true && inspected.textSelected !== true : editor.editingText && !editor.textSelected
}

/**
 * Copy or Cut with only a caret in the text: the system clipboard keeps what another app put there.
 * The command still runs, after the keys before it, in case they selected characters after all
 * (their text then goes on the clipboard).
 */
function copyCaretOnly(editor: ClipboardEditor, kind: 'copy' | 'cut') {
  const started = editor.run(kind === 'cut' ? 'edit.cut' : 'edit.copy', {})
  const text = started.then(copiedText, () => '')
  copying = text
  void text.then((value) => {
    if (!value) return
    mine = { mark: '', texts: [] }
    void writeLater(Promise.resolve(value))
  })
  return started
}

/** A `copy` or `cut` event the editor takes: puts its mark on the clipboard and runs the command. */
function copyEvent(event: ClipboardEvent, kind: 'copy' | 'cut') {
  window.clearTimeout(waiting[kind])
  delete waiting[kind]
  const editor = latest
  if (!editor || !editorsEvent(event.target) || pageTextSelected()) return
  if (caretOnly(editor)) {
    eventCommand = copyCaretOnly(editor, kind)
    return
  }
  event.preventDefault()
  // A read-only document keeps the clipboard as it was; the command says why it cannot cut.
  if (kind === 'cut' && editor.readOnly) {
    eventCommand = editor.run('edit.cut', {})
    return
  }
  copies += 1
  const mark = `${PAGE}:${copies}`
  event.clipboardData?.setData(CLIPBOARD_TYPE, mark)
  mine = { mark, texts: [] }
  const started = editor.run(kind === 'cut' ? 'edit.cut' : 'edit.copy', {})
  eventCommand = started
  const text = started.then(copiedText, () => '')
  copying = text
  void writeLater(text)
}

/** The pictures among a clipboard's or a drop's files (SVG too: the engine draws it). */
function picturesOf(data: DataTransfer | null): File[] {
  const files = Array.from(data?.files ?? []).filter((file) => file.type.startsWith('image/'))
  if (files.length) return files
  const found: File[] = []
  for (const item of Array.from(data?.items ?? [])) {
    if (item.kind !== 'file' || !item.type.startsWith('image/')) continue
    const file = item.getAsFile()
    if (file) found.push(file)
  }
  return found
}

/** The clipboard data of a paste event. */
function pastedFrom(data: DataTransfer | null): Pasted {
  return { text: data?.getData('text/plain') ?? '', mark: data?.getData(CLIPBOARD_TYPE) ?? '', images: picturesOf(data) }
}

/** What the system clipboard holds, where the browser lets the page read it without asking; else null. */
async function readClipboard(): Promise<Pasted | null> {
  try {
    const state = await navigator.permissions?.query({ name: 'clipboard-read' as PermissionName })
    if (state?.state !== 'granted' || !navigator.clipboard?.read) return null
    const pasted: Pasted = { text: '', mark: '', images: [] }
    for (const item of await navigator.clipboard.read()) {
      for (const type of item.types) {
        if (type === 'text/plain' && !pasted.text) pasted.text = await (await item.getType(type)).text()
        else if (type.startsWith('image/')) {
          const blob = await item.getType(type)
          pasted.images.push(new File([blob], `Pasted image.${type.slice(6).replace('svg+xml', 'svg')}`, { type }))
        }
      }
    }
    return pasted
  } catch {
    return null
  }
}

/** Text as a paste takes it: line breaks as `\n`, at most `MOST_TEXT` characters. */
const pasteable = (text: string) => Array.from(text.replace(/\r\n?/g, '\n')).slice(0, MOST_TEXT).join('')

/** Whether the engine's text is being edited now, as its last answer has it. */
function editingNow(editor: ClipboardEditor) {
  const inspected = editor.doc?.info?.inspected as { editingText?: unknown } | undefined
  return inspected ? inspected.editingText === true : editor.editingText
}

/** A picture's size in pixels, or null when the browser cannot read it. */
async function pixelSize(image: Blob): Promise<{ width: number; height: number } | null> {
  const url = URL.createObjectURL(image)
  try {
    const element = new Image()
    element.src = url
    await element.decode()
    return element.naturalWidth > 0 && element.naturalHeight > 0 ? { width: element.naturalWidth, height: element.naturalHeight } : null
  } catch {
    return null
  } finally {
    URL.revokeObjectURL(url)
  }
}

/**
 * Where a picture dropped at slide point `at` goes: its natural size (0.75 pt a pixel, made smaller
 * to fit the slide as the engine does), its middle at `at`, kept on the slide. Null when the
 * picture's size cannot be read (the engine then sizes it and puts it in the middle).
 */
async function droppedRect(image: Blob, at: { x: number; y: number }, area: Area) {
  const size = await pixelSize(image)
  if (!size) return null
  const slideWidth = area.x1 - area.x0, slideHeight = area.y1 - area.y0
  let width = size.width * POINTS_PER_PIXEL, height = size.height * POINTS_PER_PIXEL
  const scale = Math.min((slideWidth * MOST_OF_SLIDE) / width, (slideHeight * MOST_OF_SLIDE) / height, 1)
  width *= scale
  height *= scale
  const x = Math.min(Math.max(at.x - width / 2, 0), slideWidth - width)
  const y = Math.min(Math.max(at.y - height / 2, 0), slideHeight - height)
  const round = (value: number) => Math.round(value * 100) / 100
  return [round(x), round(y), round(width), round(height)]
}

/**
 * Inserts picture `image` on the current slide: where it was dropped (`at`, slide points), or in
 * the middle of the slide. Read and inserted in the input order.
 */
function insertPicture(editor: ClipboardEditor, image: File, at: { x: number; y: number } | null) {
  if (image.size > MOST_BYTES) {
    toast('error', editor.t('picture.tooLarge', { name: image.name || editor.t('picture.pasted') }))
    return Promise.resolve(null)
  }
  return editor.inOrder(async () => {
    const data = base64(new Uint8Array(await image.arrayBuffer()))
    const area = editor.area
    const rect = at && area ? await droppedRect(image, at, area) : null
    const name = image.name && image.name !== 'image.png' ? image.name : `${editor.t('picture.pasted')}.${(image.type.split('/')[1] ?? 'png').replace('svg+xml', 'svg').replace('jpeg', 'jpg')}`
    return editor.run('image.insert', { data, name, ...(rect ? { rect } : {}) }, { inOrder: true })
  })
}

/** Pastes `pasted` (a paste event's, or the clipboard read for the ribbon's Paste); `plain`: text only. */
async function pasteFrom(editor: ClipboardEditor, pasted: Pasted, plain: boolean): Promise<string | null> {
  await copying
  const { text, mark, images } = pasted
  // The clipboard holds this editor's copy: its mark, or the text it wrote there for other apps
  // (which may have dropped the mark); an empty clipboard, nothing to read, is the engine's copy too.
  const ours = (mark !== '' && mark === mine?.mark) || (text !== '' && !!mine?.texts.includes(text)) || (!mark && !text && !images.length)
  if (plain) return editor.run('edit.pasteText', ours || !text ? {} : { text: pasteable(text) })
  if (ours) return editor.run('edit.paste', {})
  // Another window's copy of objects carries nothing another app could paste: nothing to do.
  if (!text && !images.length) return null
  // Text being edited takes the text; on the slide a picture comes first, then text as a new text
  // box (`edit.paste` with the text decides between the two in the engine).
  if (images[0] && !(text && editingNow(editor))) return insertPicture(editor, images[0], null)
  return editor.run('edit.paste', { text: pasteable(text) })
}

/** A `paste` event the editor takes. */
function pasteEvent(event: ClipboardEvent) {
  const plain = waiting.pasteText !== undefined
  window.clearTimeout(waiting.paste)
  window.clearTimeout(waiting.pasteText)
  delete waiting.paste
  delete waiting.pasteText
  const editor = latest
  if (!editor || !editorsEvent(event.target)) return
  // The text sink must not take the pasted text as typing.
  event.preventDefault()
  void pasteFrom(editor, pastedFrom(event.clipboardData), plain)
}

/** WebKit and Chromium send the copy, cut and paste events of a page without a text selection only when its `before…` event is cancelled. */
function beforeClipboard(event: Event) {
  if (editorsEvent(event.target) && !pageTextSelected()) event.preventDefault()
}

/**
 * For the editor's run (slides.tsx): the ribbon's (and the context menu's) Copy, Cut, Paste and
 * Paste Text Only of the selection go through
 * the system clipboard. Answers the run's answer, or null when `id` runs as usual.
 */
export function clipboardCommand(id: string, params: Record<string, unknown> | undefined): Promise<string | null> | null {
  const editor = latest
  if (!editor?.doc || (params && Object.keys(params).length)) return null
  if (id === 'edit.copy' || id === 'edit.cut') {
    const kind = id === 'edit.copy' ? 'copy' : 'cut'
    if (caretOnly(editor)) return copyCaretOnly(editor, kind)
    // The browser's copy event, which writes the clipboard (allowed after a person's click or key).
    eventCommand = null
    let sent = false
    try {
      sent = document.execCommand(kind)
    } catch {
      sent = false
    }
    const started = eventCommand as Promise<string | null> | null
    eventCommand = null
    if (sent && started) return started
    return copyWithoutEvent(editor, kind)
  }
  if (id === 'edit.paste' || id === 'edit.pasteText') return readClipboard().then((pasted) => pasteFrom(editor, pasted ?? EMPTY, id === 'edit.pasteText'))
  return null
}

/** The clipboard key `event` is (Cmd+C, Cmd+X, Cmd+V, Cmd+Alt+Shift+V), or null. */
function clipboardKind(event: KeyboardEvent): Kind | null {
  if (isMac() ? !event.metaKey || event.ctrlKey : !event.ctrlKey || event.metaKey) return null
  const key = event.key.toLowerCase()
  const is = (letter: string) => event.code === `Key${letter.toUpperCase()}` || key === letter
  if (!event.altKey && !event.shiftKey) return is('c') ? 'copy' : is('x') ? 'cut' : is('v') ? 'paste' : null
  return event.altKey && event.shiftKey && is('v') ? 'pasteText' : null
}

/**
 * Cmd+C, Cmd+X, Cmd+V and Cmd+Alt+Shift+V on the editor keep their default, so the browser sends
 * the clipboard event; kit-editor's shortcut (which cancels it) and the keyboard's safety net (which
 * re-sends the key) do not run. A capture listener on the document, before theirs.
 */
function clipboardKeyDown(event: KeyboardEvent) {
  if (!latest || event.defaultPrevented || composingKey(event)) return
  const kind = clipboardKind(event)
  if (!kind || !editorsEvent(event.target)) return
  event.stopImmediatePropagation()
  // Words selected in a panel: the browser copies them.
  if ((kind === 'copy' || kind === 'cut') && pageTextSelected()) return
  // Only a caret in the text: nothing to copy, and the clipboard stays as it is.
  if ((kind === 'copy' || kind === 'cut') && caretOnly(latest)) {
    event.preventDefault()
    void copyCaretOnly(latest, kind)
    return
  }
  // The page's body had the keyboard: the editor has it now, and the event goes to it.
  if (keyboardLost()) focusEditor(latest.editingText)
  window.clearTimeout(waiting[kind])
  // The browser sends the event during the key's default action, before this runs: if it did not, the command.
  waiting[kind] = window.setTimeout(() => {
    delete waiting[kind]
    const editor = latest
    if (!editor?.doc) return
    if (kind === 'copy' || kind === 'cut') void copyWithoutEvent(editor, kind)
    else void readClipboard().then((pasted) => pasteFrom(editor, pasted ?? EMPTY, kind === 'pasteText'))
  }, 0)
}

/** The page's clipboard events and clipboard keys while the editor shows slides. */
export function useSystemClipboard(editor: ClipboardEditor) {
  latest = editor
  useEffect(() => {
    const copy = (event: ClipboardEvent) => copyEvent(event, 'copy')
    const cut = (event: ClipboardEvent) => copyEvent(event, 'cut')
    document.addEventListener('keydown', clipboardKeyDown, true)
    document.addEventListener('copy', copy, true)
    document.addEventListener('cut', cut, true)
    document.addEventListener('paste', pasteEvent, true)
    for (const name of ['beforecopy', 'beforecut', 'beforepaste']) document.addEventListener(name, beforeClipboard, true)
    return () => {
      document.removeEventListener('keydown', clipboardKeyDown, true)
      document.removeEventListener('copy', copy, true)
      document.removeEventListener('cut', cut, true)
      document.removeEventListener('paste', pasteEvent, true)
      for (const name of ['beforecopy', 'beforecut', 'beforepaste']) document.removeEventListener(name, beforeClipboard, true)
      latest = null
    }
  }, [])
}

/** Whether a drag carries files. */
const carriesFiles = (event: DragEvent) => Array.from(event.dataTransfer?.types ?? []).includes('Files')

/** The slide point under client point (`x`, `y`), or null before the canvas is laid out. */
function slideAt(editor: ClipboardEditor, x: number, y: number) {
  const surface = document.querySelector('.slides-canvas-view .kui-canvas-surface')?.getBoundingClientRect()
  const view = editor.canvas.current?.view()
  if (!surface || !view || !(view.zoom > 0) || !editor.area) return null
  return { x: view.x + (x - surface.left) / view.zoom - editor.area.x0, y: view.y + (y - surface.top) / view.zoom - editor.area.y0 }
}

/**
 * Files dragged onto the slide (`root`, the canvas view): pictures dropped there are inserted where
 * they land; another kind of file is refused with a note. Answers whether files are over it now
 * (the canvas shows it). A file dropped anywhere else on the page is stopped, so the browser does
 * not open it in place of the editor.
 */
export function useFileDrop(root: RefObject<HTMLElement | null>): boolean {
  const [over, setOver] = useState(false)
  useEffect(() => {
    const element = root.current
    if (!element) return
    const enter = (event: DragEvent) => {
      if (!carriesFiles(event)) return
      event.preventDefault()
      if (event.dataTransfer) event.dataTransfer.dropEffect = latest?.readOnly ? 'none' : 'copy'
      setOver(true)
    }
    const leave = (event: DragEvent) => {
      if (!(event.relatedTarget instanceof Node && element.contains(event.relatedTarget))) setOver(false)
    }
    const drop = (event: DragEvent) => {
      setOver(false)
      if (!carriesFiles(event)) return
      event.preventDefault()
      const editor = latest
      if (!editor?.doc || editor.readOnly) return
      const at = slideAt(editor, event.clientX, event.clientY)
      const files = Array.from(event.dataTransfer?.files ?? [])
      const pictures = files.filter((file) => file.type.startsWith('image/') || /\.(png|jpe?g|gif|webp|bmp|svg|tiff?)$/i.test(file.name))
      for (const file of files) if (!pictures.includes(file)) toast('info', editor.t('picture.dropOnly', { name: file.name }))
      // Each picture one undo step, in the order dropped; the second and later ones 10 pt further each (WPS W-I1).
      pictures.forEach((file, index) => void insertPicture(editor, file, at ? { x: at.x + index * 10, y: at.y + index * 10 } : null))
    }
    const away = (event: DragEvent) => {
      if (carriesFiles(event) && !(event.target instanceof Node && element.contains(event.target))) event.preventDefault()
    }
    element.addEventListener('dragenter', enter)
    element.addEventListener('dragover', enter)
    element.addEventListener('dragleave', leave)
    element.addEventListener('drop', drop)
    document.addEventListener('dragover', away)
    document.addEventListener('drop', away)
    return () => {
      element.removeEventListener('dragenter', enter)
      element.removeEventListener('dragover', enter)
      element.removeEventListener('dragleave', leave)
      element.removeEventListener('drop', drop)
      document.removeEventListener('dragover', away)
      document.removeEventListener('drop', away)
    }
  }, [root])
  return over
}
