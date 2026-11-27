// The pointer says what a press would do, and the handles are where they are drawn (plan 0B;
// walkthrough C3, D7, and the gestures that already worked: D1, D3, D6, D8, C4, C5, F1, I1; WPS
// W-B1, W-Q1). A person's mouse and keys only; the shapes are read back from the engine, the
// handles from what is drawn over the slide.
// node tests/hand/pointer.mjs [chromium|webkit] [segment…]
import assert from 'node:assert/strict'
import { open, segments } from './harness.mjs'

const near = (a, b, within) => Math.abs(a - b) <= within

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

/**
 * The handles drawn over the slide, in page points: the eight resize handles (white circles 9 px
 * across), the rotation handle (a grey circle) and the adjust diamonds' count.
 */
async function handlesDrawn(e) {
  const box = await e.surface()
  const ops = await e.drawn()
  const resize = []
  let rotate = null
  let path = null
  for (const op of ops) {
    if (op.op === 'begin') path = { arcs: [], fill: null }
    else if (!path) continue
    else if (op.op === 'arc') path.arcs.push(op)
    else if (op.op === 'fill') path.fill = op.style
    else if (op.op === 'stroke') {
      const arc = path.arcs.length === 1 ? path.arcs[0] : null
      if (arc && near(arc.r, 4.5, 0.01) && path.fill === '#ffffff') resize.push({ x: box.x + arc.at[0], y: box.y + arc.at[1] })
      else if (arc && arc.r > 5 && path.fill && path.fill !== '#ffffff') rotate = { x: box.x + arc.at[0], y: box.y + arc.at[1] }
      path = null
    }
  }
  return { resize, rotate }
}

/** The cursor a resize handle at `handle` of a frame centred at `centre` should show (all page points). */
function resizeCursor(centre, handle) {
  const angle = ((Math.atan2(handle.y - centre.y, handle.x - centre.x) * 180) / Math.PI + 360) % 180
  return ['ew-resize', 'nwse-resize', 'ns-resize', 'nesw-resize'][Math.floor((angle + 22.5) / 45) % 4]
}

/** A new blank slide (made current) with `shapes` on it, nothing selected; answers their ids. */
async function fresh(e, shapes) {
  await e.agent([{ command: 'slide.new', params: { layout: 'Blank', at: 1 } }])
  await e.goto(1)
  const ids = []
  for (const shape of shapes) {
    const answer = await e.ask(shape.textBox ? 'textBox.insert' : 'shape.insert', shape)
    ids.push(answer?.id)
  }
  await e.click(930, 20)
  await e.until(async () => (await e.state()).selected === 0)
  return ids
}

/** Shape `id`'s frame, in slide points (`rot` 0 when upright). */
const boxOf = async (e, id) => {
  const box = (await e.ask('shape.inspect', { id }))?.box
  return box ? { ...box, rot: box.rot ?? 0 } : box
}
const scale = async (e) => (await e.zoom()) / 100

/** Presses at page point `from`, moves to `to` in steps (with `keys` held), releases. */
async function drag(e, from, to, { keys = [], steps = 8, before } = {}) {
  await e.page.mouse.move(from.x, from.y)
  for (const key of keys) await e.page.keyboard.down(key)
  await e.page.mouse.down()
  await e.page.mouse.move(from.x + (to.x - from.x) / 2, from.y + (to.y - from.y) / 2, { steps })
  await e.page.mouse.move(to.x, to.y, { steps })
  await e.wait(100)
  if (before) await before()
  await e.page.mouse.up()
  for (const key of keys) await e.page.keyboard.up(key)
  await e.wait(300)
}

const undo = async (e) => {
  await e.page.keyboard.press('ControlOrMeta+z')
  await e.wait(300)
}

const list = [
  ['c3-cursors', async (e) => {
    const [star, rect] = await fresh(e, [{ preset: 'star5', rect: [330, 120, 300, 300] }, { preset: 'rect', rect: [40, 380, 240, 120], text: 'Hello' }])
    const at = (x, y) => e.toScreen(x, y)
    const blank = await at(900, 60)
    assert.equal(await cursorAt(e, blank.x, blank.y), 'default', 'the empty slide: the arrow')
    const middle = await at(480, 290)
    assert.equal(await cursorAt(e, middle.x, middle.y), 'move', 'an unselected shape: move')
    await e.page.mouse.click(middle.x, middle.y)
    await e.until(async () => (await e.state()).selected === 1)
    assert.equal(await cursorAt(e, middle.x, middle.y), 'move', 'a selected star without text: move, not the text cursor')
    const check = async (label) => {
      await e.wait(200)
      const { resize, rotate } = await handlesDrawn(e)
      assert.equal(resize.length, 8, `${label}: eight handles drawn (${resize.length})`)
      const centre = { x: resize.reduce((sum, h) => sum + h.x, 0) / 8, y: resize.reduce((sum, h) => sum + h.y, 0) / 8 }
      for (const handle of resize) {
        const expected = resizeCursor(centre, handle)
        assert.equal(await cursorAt(e, handle.x, handle.y), expected, `${label}: handle at ${Math.round(handle.x - centre.x)},${Math.round(handle.y - centre.y)}`)
      }
      assert(rotate, `${label}: a rotation handle drawn`)
      assert.match(await cursorAt(e, rotate.x, rotate.y), /^url\(/, `${label}: the rotation handle has its own pointer`)
    }
    await check('upright')
    // Turned by 30°: each handle's arrow turns with it.
    await e.ask('shape.rotate', { ids: [star], deg: 30 })
    await e.click(930, 20)
    await e.page.mouse.click(middle.x, middle.y)
    await e.until(async () => (await e.state()).selected === 1)
    await check('turned 30°')
    // The selected rectangle's text: the text cursor on its word, move in its corner.
    const word = await at(160, 440)
    await e.page.mouse.click(word.x, word.y)
    await e.until(async () => (await e.ask('file.inspect')).selection?.shapes?.[0] === rect)
    assert.equal(await cursorAt(e, word.x, word.y), 'text', 'on the text of a selected shape: the text cursor')
    const corner = await at(48, 388)
    assert.equal(await cursorAt(e, corner.x, corner.y), 'move', 'beside it: move')
    // Format Painter loaded: the copy pointer, until it paints.
    // Cmd+Shift+C picks the format up (the button sits in the row's overflow in Single Line).
    await e.page.keyboard.press('ControlOrMeta+Shift+c')
    await e.wait(300)
    assert.equal(await cursorAt(e, middle.x, middle.y), 'copy', 'Format Painter loaded: the copy pointer')
    await e.page.mouse.click(middle.x, middle.y)
    await e.wait(300)
    assert.equal(await cursorAt(e, middle.x, middle.y), 'move', 'painted once, the pointer is back')
  }],
  ['d7-handle-hit', async (e) => {
    const [id] = await fresh(e, [{ preset: 'rect', rect: [300, 150, 300, 200] }])
    const middle = await e.toScreen(450, 250)
    await e.page.mouse.click(middle.x, middle.y)
    await e.until(async () => (await e.state()).selected === 1)
    await e.wait(200)
    const before = await boxOf(e, id)
    const { resize } = await handlesDrawn(e)
    const east = resize.reduce((a, b) => (b.x > a.x ? b : a))
    // A near miss, 9 px right of the E handle: the selection stays, nothing moves.
    await e.page.mouse.click(east.x + 9, east.y)
    await e.wait(300)
    assert.equal((await e.state()).selected, 1, 'a press just past a handle keeps the selection')
    assert.deepEqual(await boxOf(e, id), before, 'and a click moves nothing')
    // 6 px off the E handle's centre, dragged 40 px right: the width grows by 40 px.
    await drag(e, { x: east.x, y: east.y + 6 }, { x: east.x + 40, y: east.y + 6 })
    const after = await boxOf(e, id)
    const grown = 40 / (await scale(e))
    assert(near(after.w - before.w, grown, 7), `the width grew by ${after.w - before.w} pt, about ${grown}`)
    assert.equal(after.x, before.x, 'the left edge stays')
    await undo(e)
    assert.deepEqual(await boxOf(e, id), before, 'one undo step')
  }],
  ['rotate-handle-grab', async (e) => {
    const [id] = await fresh(e, [{ preset: 'rect', rect: [330, 200, 300, 140] }])
    const middle = await e.toScreen(480, 270)
    await e.page.mouse.click(middle.x, middle.y)
    await e.until(async () => (await e.state()).selected === 1)
    const placed = async (label) => {
      await e.wait(250)
      const { resize, rotate } = await handlesDrawn(e)
      const top = Math.min(...resize.map((h) => h.y))
      const row = resize.filter((h) => h.y < top + 0.5).sort((a, b) => a.x - b.x)
      const north = row[1]
      assert(rotate, `${label}: a rotation handle`)
      assert(near(rotate.x, north.x, 0.75) && near(north.y - rotate.y, 18, 0.75), `${label}: the rotation handle is 18 px above the top edge (${north.y - rotate.y} px)`)
      return rotate
    }
    // Where it is drawn is where it is taken, at the fitted zoom and at 200%.
    for (const label of ['fitted', '200%']) {
      if (label === '200%') {
        await e.page.keyboard.press('ControlOrMeta+1')
        for (let step = 0; step < 10; step += 1) await e.page.keyboard.press('ControlOrMeta+Equal')
        await e.until(async () => (await e.zoom()) === 200)
      }
      const rotate = await placed(label)
      const frame = await boxOf(e, id)
      const s = await scale(e)
      // The frame's centre on screen, from the rotation handle straight below it.
      const c = { x: rotate.x, y: rotate.y + 18 + (frame.h / 2) * s }
      const radius = c.y - rotate.y
      const turn = (deg) => ({ x: c.x + radius * Math.sin((deg * Math.PI) / 180), y: c.y - radius * Math.cos((deg * Math.PI) / 180) })
      await drag(e, rotate, turn(50))
      const turned = await boxOf(e, id)
      assert(near(turned.rot, 50, 3), `${label}: dragging the drawn rotation handle turns the shape (${turned.rot}°)`)
      await undo(e)
      assert.equal((await boxOf(e, id)).rot, 0, `${label}: one undo step`)
    }
    await e.page.keyboard.press('ControlOrMeta+0')
    await e.wait(300)
  }],
  ['d1-drag-body', async (e) => {
    const [id] = await fresh(e, [{ preset: 'star5', rect: [330, 120, 300, 300] }])
    const before = await boxOf(e, id)
    const from = await e.toScreen(480, 290)
    await drag(e, from, { x: from.x + 120, y: from.y })
    const after = await boxOf(e, id)
    assert.equal((await e.state()).selected, 1, 'pressed and dragged: selected')
    assert(near(after.x - before.x, 120 / (await scale(e)), 8), `moved ${after.x - before.x} pt with the pointer`)
    assert.equal(after.y, before.y, 'only across')
    await undo(e)
    assert.deepEqual(await boxOf(e, id), before, 'one undo step')
  }],
  ['d3-shift-and-escape', async (e) => {
    const [id] = await fresh(e, [{ preset: 'rect', rect: [330, 200, 200, 100] }])
    const before = await boxOf(e, id)
    const from = await e.toScreen(430, 250)
    await drag(e, from, { x: from.x + 150, y: from.y + 40 }, { keys: ['Shift'] })
    const locked = await boxOf(e, id)
    assert(locked.x > before.x + 50 && locked.y === before.y, `Shift keeps the drag on one axis (${locked.x}, ${locked.y})`)
    await undo(e)
    assert.deepEqual(await boxOf(e, id), before, 'one undo step')
    // Escape in a drag puts the shape back and leaves no step: the redo above is still there.
    await drag(e, from, { x: from.x + 100, y: from.y + 60 }, {
      before: async () => {
        await e.page.keyboard.press('Escape')
        await e.wait(200)
      },
    })
    assert.deepEqual(await boxOf(e, id), before, 'Escape puts it back')
    await e.page.keyboard.press('ControlOrMeta+Shift+z')
    await e.wait(300)
    assert.deepEqual(await boxOf(e, id), locked, 'the cancelled drag left no step (the redo still redoes the Shift drag)')
  }],
  ['d6-resize-corner', async (e) => {
    const [id] = await fresh(e, [{ preset: 'rect', rect: [330, 200, 200, 100] }])
    const middle = await e.toScreen(430, 250)
    await e.page.mouse.click(middle.x, middle.y)
    await e.until(async () => (await e.state()).selected === 1)
    await e.wait(200)
    const before = await boxOf(e, id)
    const { resize } = await handlesDrawn(e)
    const se = resize.reduce((a, b) => (b.x + b.y > a.x + a.y ? b : a))
    await drag(e, se, { x: se.x + 60, y: se.y + 10 }, { keys: ['Shift'] })
    const kept = await boxOf(e, id)
    assert(kept.w > before.w + 20 && near(kept.w / kept.h, 2, 0.05), `Shift keeps the proportions (${kept.w} × ${kept.h})`)
    assert.equal(kept.x, before.x, 'the opposite corner stays')
    await undo(e)
    assert.deepEqual(await boxOf(e, id), before, 'one undo step')
    await e.wait(200)
    await drag(e, se, { x: se.x + 60, y: se.y + 30 })
    const free = await boxOf(e, id)
    assert(free.w > before.w + 20 && free.h > before.h + 10, `a corner changes both sides (${free.w} × ${free.h})`)
    await undo(e)
    assert.deepEqual(await boxOf(e, id), before, 'one undo step')
  }],
  ['d8-rotate-shift', async (e) => {
    const [id] = await fresh(e, [{ preset: 'rect', rect: [330, 200, 300, 140] }])
    const middle = await e.toScreen(480, 270)
    await e.page.mouse.click(middle.x, middle.y)
    await e.until(async () => (await e.state()).selected === 1)
    await e.wait(250)
    const { rotate } = await handlesDrawn(e)
    const radius = middle.y - rotate.y
    const turn = (deg) => ({ x: middle.x + radius * Math.sin((deg * Math.PI) / 180), y: middle.y - radius * Math.cos((deg * Math.PI) / 180) })
    await drag(e, rotate, turn(37), { keys: ['Shift'] })
    const stepped = (await boxOf(e, id)).rot
    assert(near(stepped, 30, 0.01), `Shift turns in 15° steps (${stepped}°)`)
    await undo(e)
    assert.equal((await boxOf(e, id)).rot, 0, 'one undo step')
  }],
  ['c4-shift-click', async (e) => {
    await fresh(e, [{ preset: 'rect', rect: [200, 200, 120, 100] }, { preset: 'ellipse', rect: [600, 200, 120, 100] }])
    const shiftClick = async (x, y) => {
      await e.page.keyboard.down('Shift')
      await e.click(x, y)
      await e.page.keyboard.up('Shift')
    }
    await e.click(260, 250)
    await shiftClick(660, 250)
    assert.equal((await e.state()).selected, 2, 'Shift+click adds')
    await shiftClick(660, 250)
    assert.equal((await e.state()).selected, 1, 'Shift+click again takes it out')
  }],
  ['c5-marquee', async (e) => {
    await fresh(e, [{ preset: 'rect', rect: [200, 200, 120, 100] }, { preset: 'ellipse', rect: [600, 200, 120, 100] }])
    await drag(e, await e.toScreen(150, 150), await e.toScreen(780, 350))
    assert.equal((await e.state()).selected, 2, 'a marquee around both selects both')
  }],
  ['f1-text-box-click', async (e) => {
    await fresh(e, [])
    await e.kit.getByTestId('slides-tool-textBox').click()
    await e.click(300, 300)
    assert((await e.state()).editing, 'one click with the Text Box tool: typing')
    await e.page.keyboard.type('Typed here')
    await e.wait(300)
    const shapes = (await e.ask('slide.inspect', { index: 1, select: false }))?.shapes ?? []
    assert(JSON.stringify(shapes).includes('Typed here'), 'the text went into the new box')
    await e.page.keyboard.press('Escape')
    await e.page.keyboard.press('Escape')
  }],
  ['triple-click-paragraph', async (e) => {
    const [id] = await fresh(e, [{ preset: 'rect', rect: [200, 150, 560, 240], text: 'One two three\nFour five six' }])
    await e.click(480, 270)
    await e.until(async () => (await e.state()).selected === 1)
    // On the first line, three clicks.
    void id
    const at = await e.toScreen(480, 255)
    await e.page.mouse.click(at.x, at.y, { clickCount: 3 })
    await e.wait(400)
    assert((await e.state()).editing, 'three clicks edit the text')
    const copied = await e.ask('edit.copy')
    assert.equal(copied?.text?.trim(), 'One two three', `three clicks select the paragraph (${JSON.stringify(copied)})`)
  }],
]

await segments('pointer', list, (which) => open({ which }))
