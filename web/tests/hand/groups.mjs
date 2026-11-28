// Selecting inside groups and scaling several objects, as WPS does them (plan stage 10; walkthrough
// C9, R8; WPS W-A12): a click selects the group, a click on a member of the selected group selects
// that member (the group keeps a thin frame, the member has its own handles), which can be dragged
// and formatted alone; a double-click on a member's words edits them; Escape goes back to the
// group. A handle of one of several selected objects scales them all. A locked object's handles
// are grey and take nothing. A person's mouse and keys only; the shapes are read back from the
// engine, the frames and handles from what is drawn.
// node tests/hand/groups.mjs [chromium|webkit] [segment…]
import assert from 'node:assert/strict'
import { open, segments } from './harness.mjs'

const near = (a, b, within) => Math.abs(a - b) <= within

/** A new blank slide (made current) with `shapes` on it grouped as `groups` (lists of indexes into `shapes`); nothing selected. Answers the shapes' ids and the groups'. */
async function fresh(e, shapes, groups = []) {
  await e.agent([{ command: 'slide.new', params: { layout: 'Blank', at: 1 } }])
  await e.goto(1)
  const ids = []
  for (const shape of shapes) ids.push((await e.ask('shape.insert', shape))?.id)
  const made = []
  for (const members of groups) made.push((await e.ask('arrange.group', { ids: members.map((index) => ids[index]) }))?.id)
  await e.click(930, 20)
  await e.until(async () => (await e.state()).selected === 0)
  return { ids, groups: made }
}

const selection = async (e) => (await e.ask('file.inspect'))?.selection?.shapes ?? []
const selectionIs = async (e, want, why) => {
  await e.until(async () => JSON.stringify(await selection(e)) === JSON.stringify(want), 30)
  assert.deepEqual(await selection(e), want, why)
}
/** Shape `id`'s frame on the slide (a group member's too: these groups do not scale their members). */
const boxOf = async (e, id) => {
  const shapes = (await e.ask('slide.inspect', { index: (await e.state()).slide, select: false }))?.shapes ?? []
  const find = (list) => {
    for (const shape of list) {
      if (shape.id === id) return shape
      const inner = find(shape.children ?? [])
      if (inner) return inner
    }
    return null
  }
  const [x, y, w, h] = find(shapes)?.box ?? []
  return { x, y, w, h }
}
const childrenOf = async (e, id) => {
  const shapes = (await e.ask('slide.inspect', { index: (await e.state()).slide, select: false }))?.shapes ?? []
  return (shapes.find((shape) => shape.id === id)?.children ?? []).map((child) => child.id)
}
const scale = async (e) => (await e.zoom()) / 100
const press = async (e, key, ms = 250) => {
  await e.page.keyboard.press(key)
  await e.wait(ms)
}
const undo = (e) => press(e, 'ControlOrMeta+z', 350)

/** Presses at page point `from`, moves by (`dx`, `dy`) page pixels in steps with `keys` held, releases. */
async function drag(e, from, dx, dy, { keys = [] } = {}) {
  await e.page.mouse.move(from.x, from.y)
  for (const key of keys) await e.page.keyboard.down(key)
  await e.page.mouse.down()
  await e.page.mouse.move(from.x + dx / 2, from.y + dy / 2, { steps: 6 })
  await e.page.mouse.move(from.x + dx, from.y + dy, { steps: 6 })
  await e.wait(150)
  await e.page.mouse.up()
  for (const key of keys) await e.page.keyboard.up(key)
  await e.wait(350)
}

/** The circles of the last frame drawn over the slide, each with its fill, and the strokes' colours. */
async function drawnShapes(e) {
  const ops = await e.drawn()
  const circles = []
  const strokes = []
  let path = null
  for (const op of ops) {
    if (op.op === 'begin') path = { arcs: [], fill: null }
    else if (op.op === 'stroke') {
      strokes.push(op.style)
      if (path?.arcs.length === 1) circles.push({ at: path.arcs[0].at, r: path.arcs[0].r, fill: path.fill })
      path = null
    } else if (!path) continue
    else if (op.op === 'arc') path.arcs.push(op)
    else if (op.op === 'fill') path.fill = op.style
  }
  return { circles, strokes }
}
const handlesOf = (shapes) => shapes.circles.filter((circle) => Math.abs(circle.r - 4.5) < 0.01)
/** Whether a resize handle is drawn at slide point (`x`, `y`). */
async function handleAt(e, shapes, x, y) {
  const box = await e.surface()
  const want = await e.toScreen(x, y)
  return handlesOf(shapes).some((circle) => near(circle.at[0], want.x - box.x, 1.5) && near(circle.at[1], want.y - box.y, 1.5))
}
const GROUP_FRAME = /#808080|128, 128, 128/

const list = [
  // C9, W-A12: one click selects the group, the next on a member that member, with its own
  // handles and the group's thin frame; a click on another member selects that one.
  ['c9-click-member', async (e) => {
    const { ids: [a, b], groups: [g] } = await fresh(e, [{ preset: 'rect', rect: [100, 100, 200, 100] }, { preset: 'rect', rect: [400, 250, 200, 100] }], [[0, 1]])
    await e.click(200, 150)
    await selectionIs(e, [g], 'the first click selects the group')
    let shapes = await drawnShapes(e)
    assert(await handleAt(e, shapes, 600, 350), "the group's corner has a handle")
    assert(!shapes.strokes.some((style) => GROUP_FRAME.test(style)), 'no thin frame while the group itself is selected')
    await e.click(200, 150)
    await selectionIs(e, [a], 'a click on a member of the selected group selects the member')
    await e.wait(200)
    shapes = await drawnShapes(e)
    assert.equal(handlesOf(shapes).length, 8, `the member has its own eight handles (${handlesOf(shapes).length})`)
    assert(await handleAt(e, shapes, 300, 200), "on the member's corner")
    assert(!(await handleAt(e, shapes, 600, 350)), "not on the group's")
    assert(shapes.strokes.some((style) => GROUP_FRAME.test(style)), `the group keeps a thin grey frame (${[...new Set(shapes.strokes)].join(', ')})`)
    await e.click(500, 300)
    await selectionIs(e, [b], 'a click on another member selects that one')
    await e.click(930, 20)
    await selectionIs(e, [], 'a click on the empty slide selects nothing')
    await e.click(500, 300)
    await selectionIs(e, [g], 'and the next click the group again')
  }],
  // C9: the selected member moves alone (the other member stays, the group holds both), one undo
  // step; a drag on the selected group moves the whole group.
  ['c9-drag-member', async (e) => {
    const { ids: [a, b], groups: [g] } = await fresh(e, [{ preset: 'rect', rect: [100, 100, 200, 100] }, { preset: 'rect', rect: [400, 250, 200, 100] }], [[0, 1]])
    await e.click(200, 150)
    await e.click(200, 150)
    await selectionIs(e, [a], 'the member is selected')
    const [beforeA, beforeB] = [await boxOf(e, a), await boxOf(e, b)]
    const s = await scale(e)
    await drag(e, await e.toScreen(200, 150), 60 * s, 30 * s)
    const [afterA, afterB] = [await boxOf(e, a), await boxOf(e, b)]
    assert(near(afterA.x - beforeA.x, 60, 6) && near(afterA.y - beforeA.y, 30, 6), `the member moved (${afterA.x - beforeA.x}, ${afterA.y - beforeA.y})`)
    assert.deepEqual(afterB, beforeB, 'the other member stays')
    assert.deepEqual(await childrenOf(e, g), [a, b], 'both are still in the group')
    await selectionIs(e, [a], 'the member stays selected')
    await undo(e)
    assert.deepEqual(await boxOf(e, a), beforeA, 'one undo puts it back')
    // Escape: the group; a drag on it moves both.
    await press(e, 'Escape')
    await selectionIs(e, [g], 'Escape goes back to the group')
    await drag(e, await e.toScreen(200, 150), 30 * s, 0)
    const [movedA, movedB] = [await boxOf(e, a), await boxOf(e, b)]
    assert(movedA.x > beforeA.x + 20 && near(movedB.x - beforeB.x, movedA.x - beforeA.x, 0.01), `the whole group moves (${movedA.x - beforeA.x}, ${movedB.x - beforeB.x})`)
    await selectionIs(e, [g], 'and stays selected')
    await press(e, 'Escape')
    await selectionIs(e, [], 'a second Escape selects nothing')
  }],
  // C9: a selected member is formatted alone, from the ribbon.
  ['c9-format-member', async (e) => {
    const { ids: [a, b] } = await fresh(e, [{ preset: 'rect', rect: [100, 100, 200, 100] }, { preset: 'rect', rect: [400, 250, 200, 100] }], [[0, 1]])
    const fill = async (id) => JSON.stringify((await e.ask('shape.inspect', { id }))?.shape?.fill ?? null)
    const plain = await fill(b)
    await e.click(200, 150)
    await e.click(200, 150)
    await selectionIs(e, [a], 'the member is selected')
    await e.kit.getByTestId('ribbon-tab-shapeFormat').click().catch(() => undefined)
    await e.kit.getByTestId('slides-shape-fill').first().click()
    await e.wait(300)
    await e.kit.getByTestId('slides-fills').locator('[data-color="accent2"]').click()
    await e.wait(400)
    assert.match(await fill(a), /accent2/i, `the member takes the fill (${await fill(a)})`)
    assert.equal(await fill(b), plain, 'the other member keeps its own')
  }],
  // C9, R-E5: a double-click on a member's words edits them; Escape leaves the text with the
  // member selected, then goes back to the group, then to nothing.
  ['c9-double-click-text', async (e) => {
    const { ids: [a], groups: [g] } = await fresh(e, [{ preset: 'rect', rect: [100, 100, 300, 100], text: 'Hello' }, { preset: 'rect', rect: [500, 250, 200, 100] }], [[0, 1]])
    const at = await e.toScreen(250, 150)
    await e.page.mouse.dblclick(at.x, at.y)
    await e.wait(400)
    assert(await e.until(async () => (await e.state()).editing), "the member's text is edited")
    await e.page.keyboard.type('Hi')
    await e.wait(300)
    assert.equal((await e.ask('text.get', { id: a }))?.text, 'Hi', 'the selected word is typed over, in the member')
    await press(e, 'Escape')
    assert.equal((await e.state()).editing, false, 'Escape leaves the text')
    await selectionIs(e, [a], 'with the member selected')
    await press(e, 'Escape')
    await selectionIs(e, [g], 'then the group')
    await press(e, 'Escape')
    await selectionIs(e, [], 'then nothing')
  }],
  // R8, R-D17: a handle of one of several selected objects scales them all by the same ratio, each
  // about its own opposite corner, in one undo step.
  ['r8-multi-resize', async (e) => {
    const { ids: [a, b] } = await fresh(e, [{ preset: 'rect', rect: [100, 100, 200, 100] }, { preset: 'rect', rect: [500, 200, 100, 150] }])
    await e.click(200, 150)
    await e.page.keyboard.down('Shift')
    await e.click(550, 275)
    await e.page.keyboard.up('Shift')
    await selectionIs(e, [a, b], 'both are selected')
    const [beforeA, beforeB] = [await boxOf(e, a), await boxOf(e, b)]
    const s = await scale(e)
    await drag(e, await e.toScreen(300, 200), 100 * s, 50 * s)
    const [afterA, afterB] = [await boxOf(e, a), await boxOf(e, b)]
    const kx = afterA.w / beforeA.w
    const ky = afterA.h / beforeA.h
    assert(kx > 1.3 && ky > 1.3, `the handle's object grew (${kx} × ${ky})`)
    assert(near(afterB.w / beforeB.w, kx, 0.01) && near(afterB.h / beforeB.h, ky, 0.01), `the other by the same ratio (${afterB.w / beforeB.w} × ${afterB.h / beforeB.h})`)
    assert.equal(afterB.x, beforeB.x, "its own top left stays")
    assert.equal(afterB.y, beforeB.y, 'on both axes')
    await undo(e)
    assert.deepEqual([await boxOf(e, a), await boxOf(e, b)], [beforeA, beforeB], 'one undo step for both')
  }],
  // R-C25: a locked object's handles are grey, without a rotation handle, and a drag from one
  // changes nothing.
  ['locked-grey-handles', async (e) => {
    const { ids: [a] } = await fresh(e, [{ preset: 'rect', rect: [300, 200, 200, 100] }])
    await e.click(400, 250)
    await selectionIs(e, [a], 'the shape is selected')
    await e.agent([{ command: 'shape.lock', params: { ids: [a], locked: true } }])
    assert.equal((await e.ask('shape.inspect', { id: a }))?.shape?.locked, true, 'and then locked')
    await selectionIs(e, [a], 'it stays selected')
    // The pointer over the slide: the overlays are drawn for the selection as it is now.
    const over = await e.toScreen(400, 400)
    await e.page.mouse.move(over.x, over.y, { steps: 3 })
    await e.wait(400)
    const shapes = await drawnShapes(e)
    const handles = handlesOf(shapes)
    assert.equal(handles.length, 8, `eight handles (${handles.length})`)
    assert(handles.every((circle) => circle.fill === '#a0a0a0'), `all grey (${[...new Set(handles.map((circle) => circle.fill))].join(', ')})`)
    assert(!shapes.circles.some((circle) => circle.r > 5), 'no rotation handle')
    const before = await boxOf(e, a)
    await drag(e, await e.toScreen(500, 300), 40, 30)
    assert.deepEqual(await boxOf(e, a), before, 'a drag from its corner changes nothing')
  }],
]

await segments('groups', list, (which) => open({ which }))
