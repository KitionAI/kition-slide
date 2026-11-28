// The system clipboard and pictures coming in (plan stage 5; walkthrough J2, F8; WPS W-I6):
// node tests/hand/clipboard.mjs [chromium|webkit] [segment…]
// Real keys and clicks. Chromium is given the clipboard permissions, so the test reads what a real
// Cmd+C put on the system clipboard and puts text and a PNG there for a real Cmd+V. WebKit cannot
// read or write its system clipboard from a test: there the test listens to the `copy` and `cut`
// events the real keys make (the editor's mark is in them), and stands in for another app's paste
// and for a drop of a file with the events the browser would send (`ClipboardEvent` and `DragEvent`
// carrying a `DataTransfer`). What changed is read back from the engine.
import assert from 'node:assert/strict'
import { open, segments } from './harness.mjs'

const MARK = 'application/x-kition-slide'

const press = async (e, key, ms = 350) => {
  await e.page.keyboard.press(key)
  await e.wait(ms)
}

/** A new blank slide (made current) with nothing selected. */
async function fresh(e) {
  await e.agent([{ command: 'slide.new', params: { layout: 'Blank', at: 1 } }])
  await e.goto(1)
  await e.click(930, 20)
  await e.until(async () => (await e.state()).selected === 0)
}

const shapesOn = async (e) => (await e.ask('slide.inspect', { index: (await e.state()).slide, select: false }))?.shapes ?? []
const textOf = async (e, id) => (await e.ask('text.get', { id }))?.text

/** Waits until `read()` answers `want` (deep equal); answers what it read last. */
async function becomes(e, read, want, tries = 40) {
  let last
  await e.until(async () => {
    last = await read()
    try {
      assert.deepEqual(last, want)
      return true
    } catch {
      return false
    }
  }, tries)
  return last
}

/** A text box made the way a person makes one (the Text Box tool, one click) with `text` typed in it, still being edited. */
async function typedBox(e, text) {
  await fresh(e)
  await e.kit.getByTestId('ribbon-tab-home').click()
  await e.kit.getByTestId('slides-tool-textBox').click()
  await e.click(300, 200)
  assert((await e.state()).editing, 'one click with the Text Box tool: typing')
  await e.page.keyboard.type(text)
  const [box] = await shapesOn(e)
  assert.equal(await becomes(e, () => textOf(e, box.id), text), text, 'the text was typed')
  return box.id
}

/** Chromium: the system clipboard's plain text, as another app would read it. */
const systemText = (e) => e.frame().evaluate(() => navigator.clipboard.readText())
/** Chromium: puts `text` on the system clipboard, as another app would. */
const putText = (e, text) => e.frame().evaluate((value) => navigator.clipboard.writeText(value), text)

/** Records the `copy` and `cut` events the page gets (after the editor had them): their mark and whether the editor took them. */
const listenToCopies = (e) =>
  e.frame().evaluate((mark) => {
    window.__copies = []
    for (const type of ['copy', 'cut']) window.addEventListener(type, (event) => window.__copies.push({ type, mark: event.clipboardData?.getData(mark) ?? '', taken: event.defaultPrevented }))
  }, MARK)
const copies = (e) => e.frame().evaluate(() => window.__copies ?? [])

/**
 * WebKit: another app's paste, as the browser sends it to the element with the keyboard: `text`
 * and, with `png`, a picture of `png.width` × `png.height` pixels.
 */
const sendPaste = (e, { text = '', png = null } = {}) =>
  e.frame().evaluate(async ({ text, png }) => {
    const data = new DataTransfer()
    if (text) data.setData('text/plain', text)
    if (png) {
      const canvas = document.createElement('canvas')
      canvas.width = png.width
      canvas.height = png.height
      canvas.getContext('2d').fillRect(0, 0, png.width, png.height)
      const blob = await new Promise((resolve) => canvas.toBlob(resolve, 'image/png'))
      data.items.add(new File([blob], 'image.png', { type: 'image/png' }))
    }
    const event = new ClipboardEvent('paste', { clipboardData: data, bubbles: true, cancelable: true, composed: true })
    if (event.clipboardData !== data) Object.defineProperty(event, 'clipboardData', { value: data })
    ;(document.activeElement ?? document.body).dispatchEvent(event)
    return event.defaultPrevented
  }, { text, png })

/** Chromium: a PNG of `width` × `height` pixels on the system clipboard, as a screenshot tool puts it there. */
const putPicture = (e, width, height) =>
  e.frame().evaluate(async ({ width, height }) => {
    const canvas = document.createElement('canvas')
    canvas.width = width
    canvas.height = height
    canvas.getContext('2d').fillRect(0, 0, width, height)
    const blob = await new Promise((resolve) => canvas.toBlob(resolve, 'image/png'))
    await navigator.clipboard.write([new ClipboardItem({ 'image/png': blob })])
  }, { width, height })

const isPicture = (shape) => shape.kind === 'picture' || shape.type === 'picture'
const near = (a, b, what, within = 0.6) => assert(Math.abs(a - b) < within, `${what}: ${a} (want ${b})`)

const both = [
  // J2: Cmd+C of selected characters reaches other apps; Cut takes them; text another app copied
  // pastes at the caret; with nothing selected it becomes a new text box on the slide.
  ['j2-text-in-and-out', async (e) => {
    const chromium = e.which === 'chromium'
    if (!chromium) await listenToCopies(e)
    const id = await typedBox(e, 'Hello world')
    for (let index = 0; index < 5; index += 1) await press(e, 'Shift+ArrowLeft', 80)
    if (chromium) await putText(e, 'before')
    await press(e, 'ControlOrMeta+c', 500)
    if (chromium) assert.equal(await becomes(e, () => systemText(e), 'world'), 'world', 'Cmd+C put the selected characters on the system clipboard')
    else assert.deepEqual((await copies(e)).map((item) => item.taken && item.mark !== ''), [true], 'Cmd+C sent a copy event, and the editor took it with its mark')
    await press(e, 'ControlOrMeta+x', 500)
    assert.equal(await becomes(e, () => textOf(e, id), 'Hello '), 'Hello ', 'Cmd+X took the characters away')
    if (chromium) assert.equal(await systemText(e), 'world', 'and left them on the system clipboard')
    // Back in at the start: the editor's own copy.
    await press(e, 'Home', 150)
    await press(e, 'ControlOrMeta+v', 600)
    assert.equal(await becomes(e, () => textOf(e, id), 'worldHello '), 'worldHello ', 'Cmd+V pasted the characters cut')
    // Text another app copied, while the text is edited: in at the caret.
    await press(e, 'End', 150)
    if (chromium) {
      await putText(e, 'from another app')
      await press(e, 'ControlOrMeta+v', 600)
    } else {
      assert(await sendPaste(e, { text: 'from another app' }), 'the editor took the paste')
      await e.wait(600)
    }
    assert.equal(await becomes(e, () => textOf(e, id), 'worldHello from another app'), 'worldHello from another app', "another app's text went in at the caret")
    assert((await e.state()).editing, 'the text is still being edited')
    // Nothing selected: the text becomes a new text box on the slide.
    await press(e, 'Escape', 200)
    await press(e, 'Escape', 200)
    await e.click(930, 20)
    assert.equal((await e.state()).selected, 0, 'nothing selected')
    const before = (await shapesOn(e)).length
    if (chromium) {
      await putText(e, 'A new note')
      await press(e, 'ControlOrMeta+v', 700)
    } else {
      await sendPaste(e, { text: 'A new note' })
      await e.wait(700)
    }
    assert(await e.until(async () => (await shapesOn(e)).length === before + 1), 'the paste made a new shape')
    const made = (await shapesOn(e)).at(-1)
    assert.equal(await textOf(e, made.id), 'A new note', 'a text box with the text')
    const undone = (await shapesOn(e)).length
    await press(e, 'ControlOrMeta+z', 500)
    assert.equal((await shapesOn(e)).length, undone - 1, 'one Cmd+Z takes the pasted text box back')
  }],
  // Copy and paste inside the editor still carry the shape and its formatting, at the offset Duplicate uses.
  ['j1-objects-inside', async (e) => {
    const chromium = e.which === 'chromium'
    await fresh(e)
    await e.agent([{ command: 'shape.insert', params: { preset: 'rect', rect: [100, 100, 240, 120], text: 'Bold words' } }])
    const [shape] = await shapesOn(e)
    await e.agent([{ command: 'format.bold', params: { ids: [shape.id], on: true } }, { command: 'shape.fill', params: { ids: [shape.id], color: '#2E75B6' } }])
    // A press on the shape away from its words selects it (WPS W-F2).
    await e.click(112, 108)
    assert.equal((await e.state()).selected, 1, 'the shape is selected')
    await press(e, 'ControlOrMeta+d', 500)
    const duplicate = (await shapesOn(e)).at(-1)
    const offset = [duplicate.box[0] - shape.box[0], duplicate.box[1] - shape.box[1]]
    await press(e, 'ControlOrMeta+z', 500)
    await e.click(112, 108)
    if (chromium) await putText(e, 'before')
    await press(e, 'ControlOrMeta+c', 500)
    if (chromium) assert.equal(await becomes(e, () => systemText(e), 'Bold words'), 'Bold words', "Cmd+C put the shape's text on the system clipboard")
    await press(e, 'ControlOrMeta+v', 700)
    assert(await e.until(async () => (await shapesOn(e)).length === 2), 'Cmd+V pasted a shape')
    const pasted = (await shapesOn(e)).at(-1)
    assert.notEqual(pasted.id, shape.id, 'a new shape')
    near(pasted.box[0] - shape.box[0], offset[0], 'pasted as far right as Duplicate puts it')
    near(pasted.box[1] - shape.box[1], offset[1], 'and as far down')
    near(pasted.box[2], shape.box[2], 'as wide')
    const [original, copy] = await Promise.all([e.ask('shape.inspect', { id: shape.id }), e.ask('shape.inspect', { id: pasted.id })])
    assert.deepEqual(copy.shape.fill, original.shape.fill, 'with the fill')
    assert.equal(await textOf(e, pasted.id), 'Bold words', 'and the text')
    assert(JSON.stringify(original.shape.text).includes('"bold":true'), 'the original is bold')
    assert.deepEqual(copy.shape.text, original.shape.text, 'formatted as the original: the formatting came along')
  }],
  // F8: a picture on the clipboard (a screenshot) pastes at 0.75 pt a pixel in the middle of the slide (WPS W-I6).
  ['f8-paste-picture', async (e) => {
    await fresh(e)
    await e.click(930, 20)
    if (e.which === 'chromium') {
      await putPicture(e, 480, 320)
      await press(e, 'ControlOrMeta+v', 900)
    } else {
      assert(await sendPaste(e, { png: { width: 480, height: 320 } }), 'the editor took the paste')
      await e.wait(900)
    }
    assert(await e.until(async () => (await shapesOn(e)).some(isPicture)), 'the paste inserted a picture')
    const picture = (await shapesOn(e)).find(isPicture)
    assert.deepEqual(picture.box.map((value) => Math.round(value)), [300, 150, 360, 240], '480 × 320 pixels: 360 × 240 pt in the middle of the slide')
    await press(e, 'ControlOrMeta+z', 500)
    assert(!(await shapesOn(e)).some(isPicture), 'one Cmd+Z takes the picture back')
  }],
  // F8: a picture file dropped on the slide goes where it was dropped, its middle there.
  ['f8-drop-picture', async (e) => {
    await fresh(e)
    const at = await e.toScreen(600, 300)
    const dropped = await e.frame().evaluate(async ({ x, y }) => {
      const frame = window.frameElement.getBoundingClientRect()
      const clientX = x - frame.left, clientY = y - frame.top
      const canvas = document.createElement('canvas')
      canvas.width = 200
      canvas.height = 100
      canvas.getContext('2d').fillRect(0, 0, 200, 100)
      const blob = await new Promise((resolve) => canvas.toBlob(resolve, 'image/png'))
      const data = new DataTransfer()
      data.items.add(new File([blob], 'Chart.png', { type: 'image/png' }))
      const target = document.elementFromPoint(clientX, clientY)
      let taken = false
      for (const type of ['dragenter', 'dragover', 'drop']) {
        const event = new DragEvent(type, { dataTransfer: data, clientX, clientY, bubbles: true, cancelable: true, composed: true })
        if (event.dataTransfer !== data) Object.defineProperty(event, 'dataTransfer', { value: data })
        target.dispatchEvent(event)
        if (type === 'drop') taken = event.defaultPrevented
      }
      return taken
    }, at)
    assert(dropped, 'the slide took the drop')
    assert(await e.until(async () => (await shapesOn(e)).some(isPicture)), 'the drop inserted a picture')
    const picture = (await shapesOn(e)).find(isPicture)
    const box = picture.box
    near(box[2], 150, '200 pixels wide: 150 pt')
    near(box[3], 75, '100 pixels high: 75 pt')
    // Where the drop was aimed comes from the zoom the status bar shows (a whole percent): within 2 pt.
    near(box[0] + box[2] / 2, 600, 'its middle where it was dropped (x)', 2)
    near(box[1] + box[3] / 2, 300, 'and (y)', 2)
    await press(e, 'ControlOrMeta+z', 500)
    assert(!(await shapesOn(e)).some(isPicture), 'one Cmd+Z takes the dropped picture back')
  }],
]

const chromiumOnly = [
  // J8c: after a shape was copied here, text another app copied since is what Cmd+V pastes.
  ['j2-newer-outside-text-wins', async (e) => {
    await fresh(e)
    await e.agent([{ command: 'shape.insert', params: { preset: 'ellipse', rect: [100, 100, 200, 120] } }])
    await e.click(200, 160)
    assert.equal((await e.state()).selected, 1, 'the shape is selected')
    await press(e, 'ControlOrMeta+c', 500)
    await putText(e, 'newer outside text')
    await e.click(930, 20)
    await press(e, 'ControlOrMeta+v', 700)
    assert(await e.until(async () => (await shapesOn(e)).length === 2), 'Cmd+V pasted something')
    const made = (await shapesOn(e)).at(-1)
    assert.equal(await textOf(e, made.id), 'newer outside text', "the other app's text, not the shape copied before")
  }],
  // Cmd+Alt+Shift+V pastes text only; Cmd+Shift+V is the Format Painter's again.
  ['j2-paste-text-only', async (e) => {
    const id = await typedBox(e, 'x')
    await putText(e, 'plain words')
    await press(e, 'ControlOrMeta+Alt+Shift+v', 700)
    assert.equal(await becomes(e, () => textOf(e, id), 'xplain words'), 'xplain words', 'Cmd+Alt+Shift+V pasted the text')
    // One Escape leaves the text with the text box selected; Cmd+Shift+C picks its formatting up.
    await press(e, 'Escape', 300)
    assert.equal((await e.state()).selected, 1, 'the text box is selected')
    await press(e, 'ControlOrMeta+Shift+c', 400)
    assert.equal((await e.ask('format.state'))?.painter, true, 'the Format Painter has the formatting')
    const count = (await shapesOn(e)).length
    await press(e, 'ControlOrMeta+Shift+v', 600)
    assert.equal((await shapesOn(e)).length, count, 'Cmd+Shift+V pasted no text box')
    assert.equal(await textOf(e, id), 'xplain words', 'nor text')
    assert.equal((await e.ask('format.state'))?.painter, false, 'it applied the Format Painter (once, which put it down)')
  }],
]

const which = process.argv[2] ?? 'chromium'
await segments('clipboard', which === 'chromium' ? [...both, ...chromiumOnly] : both, async (browser) => {
  const editor = await open({ which: browser })
  if (browser === 'chromium') await editor.page.context().grantPermissions(['clipboard-read', 'clipboard-write'])
  return editor
})
