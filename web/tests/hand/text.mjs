// Text by hand, as WPS works (plan stage 3; walkthrough C8, D12, E3, E6, E9, E11, E13, R8; WPS W-E3,
// W-F2 to W-F5): a click on a shape's words puts the caret there, a press elsewhere on it takes the
// object, a drag on words selects them, the frame's band moves an edited box; Enter and F2 select
// all; typing on a selected shape goes on where its caret was; the platform's text keys; an empty
// item keeps its place; an empty text box left goes; the caret blinks without the slide being drawn
// again. A person's mouse and keys only; the text is read back from the engine.
// node tests/hand/text.mjs [chromium|webkit] [segment…]
import assert from 'node:assert/strict'
import { open, segments } from './harness.mjs'

const press = async (e, key, ms = 250) => {
  await e.page.keyboard.press(key)
  await e.wait(ms)
}

/** A new blank slide (made current) with `shapes` on it, nothing selected; answers their ids. */
async function fresh(e, shapes = [], layout = 'Blank') {
  await e.agent([{ command: 'slide.new', params: { layout, at: 1 } }])
  await e.goto(1)
  const ids = []
  for (const { textBox, ...shape } of shapes) ids.push((await e.ask(textBox ? 'textBox.insert' : 'shape.insert', shape))?.id)
  await e.click(930, 20)
  await e.until(async () => (await e.state()).selected === 0)
  return ids
}

const shapesOn = async (e) => (await e.ask('slide.inspect', { index: (await e.state()).slide, select: false }))?.shapes ?? []
const textOf = async (e, id) => (await e.ask('text.get', { id }))?.text
const boxOf = async (e, id) => (await e.ask('shape.inspect', { id }))?.box
/** Waits until shape `id`'s text is `want`; answers the text it has then. */
async function textBecomes(e, id, want) {
  await e.until(async () => (await textOf(e, id)) === want, 30)
  return textOf(e, id)
}

/** The canvas's pointer, after the mouse rests at page point (`x`, `y`). */
async function cursorAt(e, x, y) {
  await e.page.mouse.move(x, y, { steps: 2 })
  await e.wait(150)
  let last = ''
  for (let index = 0; index < 20; index += 1) {
    const now = await e.kit.getByTestId('kit-canvas-surface').evaluate((element) => element.style.cursor)
    if (now === last) return now
    last = now
    await e.wait(80)
  }
  return last
}

/** Presses at page point `from`, moves to `to` in steps, releases. */
async function drag(e, from, to, steps = 8) {
  await e.page.mouse.move(from.x, from.y)
  await e.page.mouse.down()
  await e.page.mouse.move(from.x + (to.x - from.x) / 2, from.y + (to.y - from.y) / 2, { steps })
  await e.page.mouse.move(to.x, to.y, { steps })
  await e.wait(100)
  await e.page.mouse.up()
  await e.wait(300)
}

/** The caret drawn over the slide (the engine's near-black line), in canvas points, or null. */
async function caretDrawn(e) {
  const ops = await e.drawn()
  let path = []
  for (const op of ops) {
    if (op.op === 'begin') path = []
    else if (op.op === 'move' || op.op === 'line') path.push(op)
    else if (op.op === 'stroke') {
      if (op.style === '#141414' && path.length === 2 && Math.abs(path[0].at[0] - path[1].at[0]) < 0.5) return { x: path[0].at[0], y: path[0].at[1] }
      path = []
    }
  }
  return null
}

/** The caret, waiting for a beat it shows on. */
async function caret(e) {
  for (let index = 0; index < 30; index += 1) {
    const found = await caretDrawn(e)
    if (found) return found
    await e.wait(60)
  }
  return null
}

/** Makes the editor's frame report `platform` as `isMac` reads it (`null`: its own again). */
const pretend = (e, platform) =>
  e.frame().evaluate((name) => {
    window.__ownPlatform ??= navigator.platform
    const value = name ?? window.__ownPlatform
    Object.defineProperty(Navigator.prototype, 'platform', { configurable: true, get: () => value })
  }, platform)

const list = [
  ['c8-click-shape-text', async (e) => {
    const [id] = await fresh(e, [{ preset: 'rect', rect: [200, 150, 560, 150], text: 'Hello world' }])
    const word = await e.toScreen(480, 225)
    const corner = await e.toScreen(215, 165)
    assert.equal(await cursorAt(e, word.x, word.y), 'text', 'an unselected shape’s words: the text cursor')
    assert.equal(await cursorAt(e, corner.x, corner.y), 'move', 'its empty part: move')
    // The first click on the words: the caret there (it used to only select the shape).
    await e.page.mouse.click(word.x, word.y)
    await e.wait(300)
    assert((await e.state()).editing, 'one click on the words edits the text')
    await press(e, 'Escape')
    assert.deepEqual(await e.state().then((s) => [s.selected, s.editing]), [1, false], 'Escape: the shape selected')
    // A click on the selected shape away from its words keeps the object selected: as the pointer says.
    assert.equal(await cursorAt(e, corner.x, corner.y), 'move', 'the selected shape away from the words: move')
    await e.page.mouse.click(corner.x, corner.y)
    await e.wait(300)
    assert.deepEqual(await e.state().then((s) => [s.selected, s.editing]), [1, false], 'a click there selects, it does not edit')
    assert.equal(await textOf(e, id), 'Hello world')
  }],
  ['d12-drag-on-text-selects', async (e) => {
    const [id] = await fresh(e, [{ textBox: true, rect: [200, 300, 560, 40], text: 'Drag across these words' }])
    const before = await boxOf(e, id)
    await drag(e, await e.toScreen(212, 314), await e.toScreen(330, 314))
    assert((await e.state()).editing, 'a drag on the words of an unselected text box edits it')
    const copied = (await e.ask('edit.copy'))?.text ?? ''
    assert(copied.length >= 3 && 'Drag across these words'.includes(copied), `and selects them (${JSON.stringify(copied)})`)
    assert.deepEqual(await boxOf(e, id), before, 'the box did not move')
  }],
  ['r8-drag-frame-while-editing', async (e) => {
    const [id] = await fresh(e, [{ textBox: true, rect: [200, 300, 560, 40], text: 'Move me by my frame' }])
    await e.click(230, 314)
    assert((await e.state()).editing, 'editing the box')
    const before = await boxOf(e, id)
    // Its top edge, 2 px inside, away from the handles.
    const edge = await e.toScreen(400, 300)
    edge.y += 2
    assert.equal(await cursorAt(e, edge.x, edge.y), 'move', 'the frame’s band while editing: move')
    await drag(e, edge, { x: edge.x + 80, y: edge.y + 40 })
    const after = await boxOf(e, id)
    assert(after.x > before.x + 30 && after.y > before.y + 15, `the box moved by its frame (${before.x},${before.y} → ${after.x},${after.y})`)
    assert.equal(await textOf(e, id), 'Move me by my frame', 'its text as it was')
    await press(e, 'ControlOrMeta+z', 400)
    assert.deepEqual(await boxOf(e, id), before, 'one undo step')
  }],
  ['e9-enter-selects-all', async (e) => {
    const [id] = await fresh(e, [{ preset: 'rect', rect: [200, 150, 560, 150], text: 'Replace me' }])
    await e.click(215, 165)
    await e.until(async () => (await e.state()).selected === 1)
    await press(e, 'Enter')
    await e.page.keyboard.type('N')
    assert.equal(await textBecomes(e, id, 'N'), 'N', 'Enter selects all the text: typing replaces it')
    await press(e, 'Escape')
    await press(e, 'F2')
    assert((await e.state()).editing, 'F2 edits the text')
    await e.page.keyboard.type('M')
    assert.equal(await textBecomes(e, id, 'M'), 'M', 'with all of it selected too')
    await press(e, 'Escape')
    await press(e, 'Escape')
  }],
  ['e9-typing-at-last-caret', async (e) => {
    const [id] = await fresh(e, [{ preset: 'rect', rect: [200, 150, 560, 150], text: 'Hello world' }])
    // A click in the middle of the first word, Escape, then typing: the letter goes where the caret was.
    await e.click(452, 225)
    assert((await e.state()).editing)
    await press(e, 'Escape')
    assert.deepEqual(await e.state().then((s) => [s.selected, s.editing]), [1, false])
    await e.page.keyboard.type('X')
    await e.until(async () => (await textOf(e, id))?.includes('X'))
    const text = await textOf(e, id)
    const at = text.indexOf('X')
    assert.equal(text.replace('X', ''), 'Hello world', `the text is kept (${text})`)
    assert(at >= 1 && at <= 4, `typed inside the word clicked (${text})`)
    await press(e, 'Escape')
    // A shape never edited: at the end of its text.
    const [other] = await fresh(e, [{ preset: 'rect', rect: [200, 150, 560, 150], text: 'Back' }])
    await e.click(215, 165)
    await e.until(async () => (await e.state()).selected === 1)
    await e.page.keyboard.type('Hi')
    assert.equal(await textBecomes(e, other, 'BackHi'), 'BackHi', 'typed after its text, the text kept')
    await press(e, 'Escape')
    await press(e, 'Escape')
  }],
  ['e3-cmd-arrows', async (e) => {
    const [id] = await fresh(e, [{ textBox: true, rect: [200, 300, 560, 40], text: 'one two three' }])
    await e.click(330, 314)
    assert((await e.state()).editing)
    // Windows' and Linux's keys: Ctrl+Left by word, Ctrl+Home and Ctrl+End to the text's ends,
    // Ctrl+Backspace a word back. Then the Mac's.
    await pretend(e, 'Linux x86_64')
    await press(e, 'End')
    await press(e, 'Control+ArrowLeft')
    await e.page.keyboard.type('X')
    assert.equal(await textBecomes(e, id, 'one two Xthree'), 'one two Xthree', 'Ctrl+Left goes back a word (Windows, Linux)')
    await press(e, 'Control+Home')
    await e.page.keyboard.type('^')
    await press(e, 'Control+End')
    await e.page.keyboard.type('$')
    assert.equal(await textBecomes(e, id, '^one two Xthree$'), '^one two Xthree$', 'Ctrl+Home, Ctrl+End')
    await press(e, 'Control+Backspace')
    assert.equal(await textBecomes(e, id, '^one two '), '^one two ', 'Ctrl+Backspace deletes a word')
    // The Mac's: Cmd+Left / Right to the line's ends, Cmd+Backspace to its start, Ctrl+A / Ctrl+E.
    await pretend(e, 'MacIntel')
    try {
      await press(e, 'Meta+ArrowLeft')
      await e.page.keyboard.type('A')
      await press(e, 'Meta+ArrowRight')
      await e.page.keyboard.type('Z')
      assert.equal(await textBecomes(e, id, 'A^one two Z'), 'A^one two Z', 'Cmd+Left, Cmd+Right: the line’s start and end')
      await press(e, 'Control+a')
      await e.page.keyboard.type('a')
      await press(e, 'Control+e')
      await e.page.keyboard.type('e')
      assert.equal(await textBecomes(e, id, 'aA^one two Ze'), 'aA^one two Ze', 'Ctrl+A, Ctrl+E: the line’s start and end')
      await press(e, 'Meta+Shift+ArrowLeft')
      await e.page.keyboard.type('q')
      assert.equal(await textBecomes(e, id, 'q'), 'q', 'Shift+Cmd+Left selects to the line’s start')
      await e.page.keyboard.type(' more')
      await press(e, 'Meta+Backspace')
      assert.equal(await textBecomes(e, id, ''), '', 'Cmd+Backspace deletes to the line’s start')
    } finally {
      await pretend(e, null)
    }
    await press(e, 'Escape')
    await press(e, 'Escape')
  }],
  ['e6-empty-bullet', async (e) => {
    await e.agent([{ command: 'slide.new', params: { layout: 'titleAndContent', at: 1, title: 'Title', body: 'First' } }])
    await e.goto(1)
    const body = (await shapesOn(e))[1]?.id
    assert(body != null, 'a body placeholder')
    const box = await boxOf(e, body)
    await e.click(box.x + 60, box.y + 20)
    assert((await e.state()).editing, 'editing the list')
    await press(e, 'Home')
    const start = await caret(e)
    await press(e, 'End')
    await press(e, 'Enter')
    const empty = await caret(e)
    assert(start && empty, 'the caret is drawn')
    assert(Math.abs(start.x - empty.x) < 1, `the empty item’s caret is where an item’s text starts (${empty.x} vs ${start.x})`)
    assert(empty.y > start.y + 5, 'on the next line')
    await press(e, 'Backspace')
    assert.equal(await textBecomes(e, body, 'First\n'), 'First\n', 'Backspace at an empty item takes its bullet away and keeps the line')
    await press(e, 'Backspace')
    assert.equal(await textBecomes(e, body, 'First'), 'First', 'the next one joins it to the item before')
    await press(e, 'Escape')
    await press(e, 'Escape')
  }],
  ['e11-empty-textbox', async (e) => {
    await fresh(e)
    await e.kit.getByTestId('slides-tool-textBox').click()
    await e.click(300, 200)
    assert((await e.state()).editing, 'one click with the Text Box tool: typing')
    assert.equal((await shapesOn(e)).length, 1)
    await e.click(900, 500)
    await e.until(async () => (await shapesOn(e)).length === 0)
    assert.equal((await shapesOn(e)).length, 0, 'left with nothing typed, the box goes')
    await press(e, 'ControlOrMeta+Shift+z', 400)
    assert.equal((await shapesOn(e)).length, 0, 'and there is nothing to redo')
    // Escape leaves it too; a box typed in stays.
    await e.kit.getByTestId('slides-tool-textBox').click()
    await e.click(300, 200)
    await press(e, 'Escape')
    await e.until(async () => (await shapesOn(e)).length === 0)
    assert.equal((await shapesOn(e)).length, 0, 'Escape: the box goes')
    await e.kit.getByTestId('slides-tool-textBox').click()
    await e.click(300, 200)
    await e.page.keyboard.type('kept')
    await press(e, 'Escape')
    await e.click(900, 500)
    assert.equal((await shapesOn(e)).length, 1, 'a box typed in stays')
  }],
  ['e13-caret-blinks', async (e) => {
    await fresh(e, [{ textBox: true, rect: [200, 300, 560, 40], text: 'Blinking' }])
    await e.click(230, 314)
    assert((await e.state()).editing)
    await e.page.mouse.move(5, 5)
    await e.wait(600)
    const draws = await e.slideDraws()
    let shown = 0
    let hidden = 0
    for (let index = 0; index < 24; index += 1) {
      if (await caretDrawn(e)) shown += 1
      else hidden += 1
      await e.wait(100)
    }
    assert(shown > 2 && hidden > 2, `the caret blinks (${shown} beats shown, ${hidden} hidden)`)
    assert.equal(await e.slideDraws(), draws, 'and the slide is not drawn again for it')
    await press(e, 'Escape')
    await press(e, 'Escape')
  }],
]

await segments('text', list, (which) => open({ which }))
