// One undo step per run of typing and per held arrow key (plan stage 2; walkthrough I2, I4, D5;
// WPS W-H1, W-H2): a person's keys and clicks only, the text and the boxes read back from the
// engine through the development host.
// node tests/hand/undo.mjs [chromium|webkit] [segment…]
import assert from 'node:assert/strict'
import { open, segments } from './harness.mjs'

const press = async (e, key, ms = 300) => {
  await e.page.keyboard.press(key)
  await e.wait(ms)
}

const undo = (e) => press(e, 'ControlOrMeta+z', 400)
const redo = (e) => press(e, 'ControlOrMeta+Shift+z', 400)

/** A new blank slide (made current) with `shapes` on it, nothing selected; answers their ids. */
async function fresh(e, shapes = []) {
  await e.agent([{ command: 'slide.new', params: { layout: 'Blank', at: 1 } }])
  await e.goto(1)
  const ids = []
  for (const shape of shapes) ids.push((await e.ask('shape.insert', shape))?.id)
  await e.click(930, 20)
  await e.until(async () => (await e.state()).selected === 0)
  return ids
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
  await e.until(async () => (await textOf(e, id)) === want, 30)
  return textOf(e, id)
}

const list = [
  ['i2-typing-undo', async (e) => {
    const id = await textBox(e)
    // "one two three", with a pause of 2.6 s in the middle (WPS W-H1: a pause does not end the run).
    await e.page.keyboard.type('one two', { delay: 40 })
    await e.wait(2600)
    await e.page.keyboard.type(' three', { delay: 40 })
    assert.equal(await textBecomes(e, id, 'one two three'), 'one two three')
    await undo(e)
    assert.equal(await textBecomes(e, id, ''), '', 'one Cmd+Z takes back the whole run')
    assert.equal((await shapesOn(e)).length, 1, 'and not the text box')
    await redo(e)
    assert.equal(await textBecomes(e, id, 'one two three'), 'one two three', 'one Cmd+Shift+Z brings it back')
    await press(e, 'Escape')
    await press(e, 'Escape')
  }],
  ['i2-typing-steps', async (e) => {
    const id = await textBox(e)
    // Typing, the caret moved, typing again: two steps (WPS W-H2).
    await e.page.keyboard.type('ab', { delay: 40 })
    await press(e, 'ArrowLeft')
    await e.page.keyboard.type('X', { delay: 40 })
    assert.equal(await textBecomes(e, id, 'aXb'), 'aXb')
    await undo(e)
    assert.equal(await textBecomes(e, id, 'ab'), 'ab', 'the first Cmd+Z: the typing after the caret moved')
    await undo(e)
    assert.equal(await textBecomes(e, id, ''), '', 'the second: the typing before')
    // Three Backspaces in a row: one step.
    await e.page.keyboard.type('hello', { delay: 40 })
    for (let index = 0; index < 3; index += 1) await press(e, 'Backspace', 60)
    assert.equal(await textBecomes(e, id, 'he'), 'he')
    await undo(e)
    assert.equal(await textBecomes(e, id, 'hello'), 'hello', 'one Cmd+Z puts the three characters back')
    await undo(e)
    assert.equal(await textBecomes(e, id, ''), '', 'the next takes the typing back')
    await press(e, 'Escape')
    await press(e, 'Escape')
  }],
  ['i4-nudge-undo', async (e) => {
    const [id] = await fresh(e, [{ preset: 'rect', rect: [300, 200, 160, 100] }])
    const x = async () => (await e.ask('shape.inspect', { id }))?.box?.x
    await e.click(380, 250)
    await e.until(async () => (await e.state()).selected === 1)
    const before = await x()
    // An arrow key held down: the first press and nine repeats.
    for (let index = 0; index < 10; index += 1) {
      await e.page.keyboard.down('ArrowRight')
      await e.wait(40)
    }
    await e.page.keyboard.up('ArrowRight')
    await e.until(async () => (await x()) > before + 50)
    const moved = await x()
    assert(moved > before + 50, `ten nudges moved the shape (${before} → ${moved})`)
    await undo(e)
    await e.until(async () => (await x()) === before)
    assert.equal(await x(), before, 'one Cmd+Z takes all ten nudges back')
    assert.equal((await shapesOn(e)).length, 1, 'and leaves the shape')
  }],
]

await segments('undo', list, (which) => open({ which }))
