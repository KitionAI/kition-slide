// Chinese input (plan stage 4; walkthrough E10; WPS W-R1, W-R2): node tests/hand/ime.mjs [chromium|webkit] [segment…]
// A text box made with the Text Box tool and real keys. The hidden field the input method types
// into sits on the caret's line, so its candidate window opens there (both browsers). In Chromium the
// input method itself is driven through the DevTools protocol (`Input.imeSetComposition`,
// `Input.insertText`): "n", "ni", "ni hao" show in the text at the caret, underlined, while the
// document stays as it was; what is committed is one undo step of its own, a single character too;
// Escape, Enter, the arrows and Backspace pressed while composing are the input method's (the text
// stays in editing, nothing moves or goes). The text, the undo steps and the state are read back
// from the engine; the caret and the underline from what is drawn over the slide.
import assert from 'node:assert/strict'
import { open, segments } from './harness.mjs'

const press = async (e, key, ms = 300) => {
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

/** A text box made the way a person makes one (the Text Box tool, one click), being edited. */
async function textBox(e) {
  await fresh(e)
  await e.kit.getByTestId('slides-tool-textBox').click()
  await e.click(300, 200)
  assert((await e.state()).editing, 'one click with the Text Box tool: typing')
  const [box] = await shapesOn(e)
  assert(box?.id != null, 'a text box was made')
  return box.id
}

const textOf = async (e, id) => (await e.ask('text.get', { id }))?.text

/** Waits until shape `id`'s text is `want`; answers the text it has then. */
async function textBecomes(e, id, want) {
  await e.until(async () => (await textOf(e, id)) === want, 40)
  return textOf(e, id)
}

/** The two-point lines of the last frame drawn over the slide, with their stroke, in canvas pixels. */
async function lines(e) {
  const ops = await e.drawn()
  const out = []
  let path = []
  for (const op of ops) {
    if (op.op === 'begin') path = []
    else if (op.op === 'move' || op.op === 'line') path.push(op)
    else if (op.op === 'stroke') {
      if (path.length === 2) out.push({ a: path[0].at, b: path[1].at, style: op.style })
      path = []
    }
  }
  return out
}

/** The caret (the engine's near-black upright line), waiting for a beat it shows on: its x, top and bottom. */
async function caret(e) {
  for (let index = 0; index < 30; index += 1) {
    const found = (await lines(e)).find((line) => line.style === '#141414' && Math.abs(line.a[0] - line.b[0]) < 0.5)
    if (found) return { x: found.a[0], top: Math.min(found.a[1], found.b[1]), bottom: Math.max(found.a[1], found.b[1]) }
    await e.wait(60)
  }
  return null
}

/** The line under a composition (dark grey, level): from x0 to x1 at y; null when none is drawn. */
async function underline(e) {
  const found = (await lines(e)).filter((line) => line.style === '#404040' && Math.abs(line.a[1] - line.b[1]) < 0.5)
  if (!found.length) return null
  const xs = found.flatMap((line) => [line.a[0], line.b[0]])
  return { x0: Math.min(...xs), x1: Math.max(...xs), y: found[0].a[1] }
}

/** The input method's hidden field, in canvas pixels (left and top of its box). */
const sink = (e) =>
  e.frame().evaluate(() => {
    const field = document.querySelector('textarea.kui-text-sink')
    const surface = document.querySelector('.slides-canvas-view .kui-canvas-surface')
    if (!field || !surface) return null
    const a = field.getBoundingClientRect()
    const b = surface.getBoundingClientRect()
    return { x: a.left - b.left, y: a.top - b.top, focused: document.activeElement === field }
  })

/** Waits until the hidden field is within 4 px of (`x`, `y`); answers where it is. */
async function sinkNear(e, x, y) {
  let at = null
  await e.until(async () => {
    at = await sink(e)
    return at != null && Math.abs(at.x - x) <= 4 && Math.abs(at.y - y) <= 4
  }, 40)
  return at
}

/** Chromium's input method, through the DevTools protocol. */
async function inputMethod(e) {
  const cdp = await e.page.context().newCDPSession(e.page)
  return {
    compose: async (text) => {
      await cdp.send('Input.imeSetComposition', { text, selectionStart: text.length, selectionEnd: text.length })
      await e.wait(150)
    },
    commit: async (text) => {
      await cdp.send('Input.insertText', { text })
      await e.wait(200)
    },
    /** A key as the browser sends it while an input method composes (key code 229). */
    key: async (key, code = key) => {
      await cdp.send('Input.dispatchKeyEvent', { type: 'rawKeyDown', key, code, windowsVirtualKeyCode: 229, nativeVirtualKeyCode: 229 })
      await cdp.send('Input.dispatchKeyEvent', { type: 'keyUp', key, code, windowsVirtualKeyCode: 229, nativeVirtualKeyCode: 229 })
      await e.wait(150)
    },
  }
}

/** How many pixels of two PNG screenshots differ clearly, and the longest level run of them. */
const differ = (e, one, two) =>
  e.page.evaluate(
    async ([a, b]) => {
      const pixels = async (data) => {
        const image = new Image()
        image.src = `data:image/png;base64,${data}`
        await image.decode()
        const canvas = document.createElement('canvas')
        canvas.width = image.width
        canvas.height = image.height
        const context = canvas.getContext('2d')
        context.drawImage(image, 0, 0)
        return { width: canvas.width, height: canvas.height, data: context.getImageData(0, 0, canvas.width, canvas.height).data }
      }
      const [p, q] = [await pixels(a), await pixels(b)]
      let total = 0
      for (let index = 0; index < p.data.length; index += 4) {
        if (Math.abs(p.data[index] - q.data[index]) + Math.abs(p.data[index + 1] - q.data[index + 1]) + Math.abs(p.data[index + 2] - q.data[index + 2]) > 120) total += 1
      }
      return total
    },
    [one.toString('base64'), two.toString('base64')],
  )

const undo = (e) => press(e, 'ControlOrMeta+z', 400)

const both = [
  ['e10-sink-at-caret', async (e) => {
    const id = await textBox(e)
    await e.page.keyboard.type('Hello', { delay: 30 })
    assert.equal(await textBecomes(e, id, 'Hello'), 'Hello')
    const end = await caret(e)
    assert(end, 'the caret is drawn')
    let at = await sinkNear(e, end.x, end.top)
    assert(at && Math.abs(at.x - end.x) <= 4 && Math.abs(at.y - end.top) <= 4, `the input method's field sits on the caret's line (${JSON.stringify(at)} vs ${JSON.stringify(end)}); it used to sit in the canvas's corner`)
    assert(at.focused, 'and has the keyboard')
    await press(e, 'ArrowLeft', 120)
    await press(e, 'ArrowLeft')
    const moved = await caret(e)
    assert(moved && moved.x < end.x - 5, 'the caret moved left')
    at = await sinkNear(e, moved.x, moved.top)
    assert(at && Math.abs(at.x - moved.x) <= 4, `the field follows it (${JSON.stringify(at)} vs ${JSON.stringify(moved)})`)
    // Text an input method commits goes in at the caret.
    await e.page.keyboard.insertText('你好')
    assert.equal(await textBecomes(e, id, 'Hel你好lo'), 'Hel你好lo')
    await press(e, 'Escape')
    await press(e, 'Escape')
  }],
]

const chromiumOnly = [
  ['e10-ime-composition', async (e) => {
    const ime = await inputMethod(e)
    const id = await textBox(e)
    await e.page.keyboard.type('ab', { delay: 30 })
    assert.equal(await textBecomes(e, id, 'ab'), 'ab')
    const start = await caret(e)
    assert(start, 'the caret is drawn')
    const box = await e.kit.getByTestId('kit-canvas-surface').boundingBox()
    // Where the composition shows, compared below with the same place without it.
    const clip = { x: Math.round(box.x + start.x) + 2, y: Math.round(box.y + start.top), width: 70, height: Math.max(8, Math.round(start.bottom - start.top)) }
    const before = await e.page.screenshot({ clip })
    const draws = await e.slideDraws()
    let width = 0
    for (const typed of ['n', 'ni', 'ni hao']) {
      await ime.compose(typed)
      let line = null
      await e.until(async () => (line = await underline(e)) != null && line.x1 - line.x0 > width, 40)
      assert(line && line.x1 - line.x0 > width, `composing “${typed}” draws a longer line under it (${JSON.stringify(line)}; nothing showed while composing)`)
      assert(Math.abs(line.x0 - start.x) <= 2, `from the caret where the composition started (${line.x0} vs ${start.x})`)
      width = line.x1 - line.x0
      assert.equal(await textOf(e, id), 'ab', 'the document does not change while composing')
    }
    assert((await e.slideDraws()) > draws, 'the slide is drawn again with the composition in it')
    const now = await caret(e)
    const line = await underline(e)
    assert(now && Math.abs(now.x - line.x1) <= 2, `the caret is at the input method's cursor, the composition's end (${now?.x} vs ${line.x1})`)
    const at = await sinkNear(e, start.x, start.top)
    assert(at && Math.abs(at.x - start.x) <= 4 && Math.abs(at.y - start.top) <= 4, `the field sits where the composition starts, so the candidate window lines up with it (${JSON.stringify(at)} vs ${JSON.stringify(start)})`)
    await e.page.mouse.move(5, 5)
    await e.wait(300)
    const composing = await e.page.screenshot({ clip })
    const changed = await differ(e, before, composing)
    assert(changed > 40, `the composed letters are on the slide (${changed} pixels changed)`)
    // The input method commits: the text, a step of its own.
    await ime.commit('你好')
    assert.equal(await textBecomes(e, id, 'ab你好'), 'ab你好')
    assert(await e.until(async () => (await underline(e)) == null, 30), 'no composition shows after the commit')
    // A single character committed, then typing: each a step of its own (WPS W-R2, W-H1).
    await ime.compose('zhong')
    await e.until(async () => (await underline(e)) != null, 30)
    await ime.commit('中')
    await e.page.keyboard.type('cd', { delay: 30 })
    assert.equal(await textBecomes(e, id, 'ab你好中cd'), 'ab你好中cd')
    await undo(e)
    assert.equal(await textBecomes(e, id, 'ab你好中'), 'ab你好中', 'the first Cmd+Z: the typing after the commit')
    await undo(e)
    assert.equal(await textBecomes(e, id, 'ab你好'), 'ab你好', 'the second: the one committed character (it used to go with the typing around it)')
    await undo(e)
    assert.equal(await textBecomes(e, id, 'ab'), 'ab', 'the third: the commit before it')
    await undo(e)
    assert.equal(await textBecomes(e, id, ''), '', 'the fourth: the typing before')
    await press(e, 'Escape')
    await press(e, 'Escape')
  }],
  ['e10-ime-keys', async (e) => {
    const ime = await inputMethod(e)
    const id = await textBox(e)
    await e.page.keyboard.type('ab', { delay: 30 })
    assert.equal(await textBecomes(e, id, 'ab'), 'ab')
    const shapes = (await shapesOn(e)).length
    await ime.compose('ni hao')
    assert(await e.until(async () => (await underline(e)) != null, 30), 'the composition shows')
    // Escape while composing is the input method's: it cancels the composition, not the editing.
    await ime.key('Escape')
    await ime.compose('')
    assert(await e.until(async () => (await underline(e)) == null, 30), 'Escape while composing takes the composition away')
    assert.equal(await textOf(e, id), 'ab', 'and types nothing')
    assert((await e.state()).editing, 'the text is still being edited')
    // Enter, the arrows and Backspace while composing: no line break, nothing moves or goes.
    await ime.compose('ni hao')
    await e.until(async () => (await underline(e)) != null, 30)
    const placed = (await shapesOn(e)).find((shape) => shape.id === id)?.box
    for (const key of ['Enter', 'ArrowLeft', 'ArrowUp', 'Backspace', 'Delete']) await ime.key(key)
    await e.wait(300)
    assert.equal(await textOf(e, id), 'ab', 'no key while composing changed the text')
    assert((await e.state()).editing, 'nor ended the editing')
    const after = await shapesOn(e)
    assert.equal(after.length, shapes, 'nor deleted the text box')
    assert.deepEqual(after.find((shape) => shape.id === id)?.box, placed, 'nor moved it')
    await ime.compose('')
    await e.until(async () => (await underline(e)) == null, 30)
    // The compositions left no undo step: one Cmd+Z takes back the typing.
    await undo(e)
    assert.equal(await textBecomes(e, id, ''), '', 'one Cmd+Z takes back the typing before the compositions')
    await press(e, 'Escape')
    await press(e, 'Escape')
  }],
]

const which = process.argv[2] ?? 'chromium'
await segments('ime', which === 'chromium' ? [...both, ...chromiumOnly] : both, (browser) => open({ which: browser }))
