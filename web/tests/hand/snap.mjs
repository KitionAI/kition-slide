// Modifier keys, arrow keys and snapping, as WPS does them (plan stage 8; walkthrough D2, D5;
// WPS W-A2, W-A7, W-A8, W-B3–W-B6, W-C3, W-C4, W-D1–W-D4, W-E8, W-I1, W-I3, W-I4). A person's mouse
// and keys only; the shapes are read back from the engine, the guides from what is drawn.
// node tests/hand/snap.mjs [chromium|webkit] [segment…]
import assert from 'node:assert/strict'
import { open, segments } from './harness.mjs'

const near = (a, b, within) => Math.abs(a - b) <= within

/** A new blank slide (made current) with `rects` on it, nothing selected; answers their ids. */
async function fresh(e, rects) {
  await e.agent([{ command: 'slide.new', params: { layout: 'Blank', at: 1 } }])
  await e.goto(1)
  const ids = []
  for (const rect of rects) ids.push((await e.ask('shape.insert', { preset: 'rect', rect }))?.id)
  await e.click(930, 20)
  await e.until(async () => (await e.state()).selected === 0)
  return ids
}

/** Shape `id`'s frame in slide points. */
const boxOf = async (e, id) => {
  const box = (await e.ask('shape.inspect', { id }))?.box
  return box ? { ...box, rot: box.rot ?? 0 } : box
}
const scale = async (e) => (await e.zoom()) / 100
const shapesOn = async (e) => (await e.ask('slide.inspect', { index: (await e.state()).slide, select: false }))?.shapes ?? []

/** Selects shape `id` by a click at slide point (`x`, `y`) on it. */
async function select(e, x, y) {
  await e.click(x, y)
  assert(await e.until(async () => (await e.state()).selected === 1), 'the shape is selected')
  await e.wait(150)
}

/** Presses at page point `from`, moves by (`dx`, `dy`) page pixels in steps with `keys` held, releases. */
async function drag(e, from, dx, dy, { keys = [], steps = 8, during } = {}) {
  await e.page.mouse.move(from.x, from.y)
  for (const key of keys) await e.page.keyboard.down(key)
  await e.page.mouse.down()
  await e.page.mouse.move(from.x + dx / 2, from.y + dy / 2, { steps })
  await e.page.mouse.move(from.x + dx, from.y + dy, { steps })
  await e.wait(150)
  if (during) await during()
  await e.page.mouse.up()
  for (const key of keys) await e.page.keyboard.up(key)
  await e.wait(300)
}

const press = async (e, key, ms = 250) => {
  await e.page.keyboard.press(key)
  await e.wait(ms)
}
const undo = (e) => press(e, 'ControlOrMeta+z', 350)

/** Opens View › Grid and Guides…, sets the boxes in `change`, and presses OK. */
async function gridSettings(e, change) {
  await e.kit.getByTestId('ribbon-tab-view').click()
  await e.kit.getByTestId('slides-view-gridSettings').click()
  await e.kit.getByTestId('slides-grid-dialog').waitFor()
  for (const [key, on] of Object.entries(change)) await e.kit.getByTestId(`slides-grid-${key}`).setChecked(on)
  await e.kit.getByTestId('slides-grid-apply').click()
  await e.wait(400)
}

/** Whether a dashed guide is drawn down the slide at slide x `x` (the last frame drawn). */
async function guideAt(e, x) {
  const box = await e.surface()
  const want = (await e.toScreen(x, 0)).x - box.x
  const ops = await e.drawn()
  for (let index = 0; index + 1 < ops.length; index += 1) {
    const [a, b] = [ops[index], ops[index + 1]]
    if (a.op === 'move' && b.op === 'line' && near(a.at[0], want, 1.5) && near(b.at[0], want, 1.5) && Math.abs(b.at[1] - a.at[1]) > 20) return true
  }
  return false
}

const list = [
  // Cmd on a corner handle grows the shape about its centre (WPS W-B4).
  ['cmd-corner-centre', async (e) => {
    const [id] = await fresh(e, [[300, 200, 160, 100]])
    await select(e, 320, 220)
    const before = await boxOf(e, id)
    const se = await e.toScreen(460, 300)
    await drag(e, se, 40, 40, { keys: ['Meta'] })
    const after = await boxOf(e, id)
    assert(after.w > before.w + 40 && after.h > before.h + 40, `both sides grew (${after.w} × ${after.h})`)
    assert(near(after.x + after.w / 2, 380, 0.01) && near(after.y + after.h / 2, 250, 0.01), `the centre stays (${after.x + after.w / 2}, ${after.y + after.h / 2})`)
    await undo(e)
    assert.deepEqual(await boxOf(e, id), before, 'one undo step')
  }],
  // Alt on a corner handle: no snapping, the size follows the pointer, the opposite corner stays (WPS W-B5).
  ['alt-corner-free', async (e) => {
    const [id] = await fresh(e, [[300, 200, 160, 100]])
    await select(e, 320, 220)
    const before = await boxOf(e, id)
    const se = await e.toScreen(460, 300)
    await drag(e, se, 19, 11, { keys: ['Alt'] })
    const after = await boxOf(e, id)
    const s = await scale(e)
    assert(near(after.w - before.w, 19 / s, 0.5) && near(after.h - before.h, 11 / s, 0.5), `the size follows the pointer (${after.w - before.w} × ${after.h - before.h} pt for ${19 / s} × ${11 / s})`)
    assert.equal(after.x, before.x, 'the opposite corner stays')
    assert.equal(after.y, before.y, 'on both axes')
  }],
  // Cmd pressed in the middle of a drag counts at once, and so does letting it go.
  ['modifier-mid-drag', async (e) => {
    const [id] = await fresh(e, [[300, 200, 160, 100]])
    await select(e, 320, 220)
    const se = await e.toScreen(460, 300)
    await e.page.mouse.move(se.x, se.y)
    await e.page.mouse.down()
    await e.page.mouse.move(se.x + 30, se.y + 30, { steps: 8 })
    await e.wait(150)
    await e.page.keyboard.down('Meta')
    await e.wait(300)
    const centred = await boxOf(e, id)
    assert(near(centred.x + centred.w / 2, 380, 0.01), `Cmd pressed mid-drag: the centre stays (${centred.x + centred.w / 2})`)
    await e.page.keyboard.up('Meta')
    await e.wait(300)
    const cornered = await boxOf(e, id)
    assert.equal(cornered.x, 300, `Cmd let go: the opposite corner is the anchor again (${cornered.x})`)
    await e.page.mouse.up()
    await e.wait(300)
  }],
  // Snap to grid off: a drag of 5 px moves 5 px, nothing holds it (walkthrough D2, WPS W-C3);
  // the arrow keys move 0.75 pt (W-D2). On again: one grid step.
  ['grid-off-follows', async (e) => {
    const [id] = await fresh(e, [[100, 100, 160, 100]])
    await gridSettings(e, { snapToGrid: false })
    const before = await boxOf(e, id)
    const from = await e.toScreen(120, 120)
    await drag(e, from, 5, 0)
    const moved = await boxOf(e, id)
    const s = await scale(e)
    assert(near(moved.x - before.x, 5 / s, 0.5), `a 5 px drag moved ${moved.x - before.x} pt (${5 / s} expected)`)
    assert.equal(moved.y, before.y, 'only across')
    await press(e, 'ArrowRight')
    assert(near((await boxOf(e, id)).x - moved.x, 0.75, 1e-6), 'grid off: an arrow key moves 0.75 pt')
    await press(e, 'Alt+ArrowRight')
    assert(near((await boxOf(e, id)).x - moved.x, 1.5, 1e-6), 'Alt+arrow: the same')
    const kept = await e.frame().evaluate(() => JSON.parse(window.localStorage.getItem('kition-slide.grid-guides') ?? 'null'))
    assert.equal(kept?.snapToGrid, false, 'the setting is kept on this machine')
    await gridSettings(e, { snapToGrid: true })
    await select(e, 140, 130)
    const x = (await boxOf(e, id)).x
    await press(e, 'ArrowRight')
    { const now = (await boxOf(e, id)).x; assert(near(now - x, 6, 1e-6), `grid on: one 6 pt grid step (${x} → ${now}, ${JSON.stringify(await e.state())})`) }
  }],
  // The spacing set in the dialog is the one the grid snaps to, the arrow keys step and the gridlines are drawn at (W-C4).
  ['grid-spacing', async (e) => {
    const [id] = await fresh(e, [[100, 100, 160, 100]])
    await e.kit.getByTestId('ribbon-tab-view').click()
    await e.kit.getByTestId('slides-view-gridSettings').click()
    await e.kit.getByTestId('slides-grid-dialog').waitFor()
    await e.kit.getByTestId('slides-grid-spacing').fill('1')
    await e.kit.getByTestId('slides-grid-gridlines').setChecked(true)
    await e.kit.getByTestId('slides-grid-apply').click()
    await e.wait(400)
    try {
      await select(e, 120, 120)
      const x = (await boxOf(e, id)).x
      await press(e, 'ArrowRight')
      const spacing = 72 / 2.54
      assert(near((await boxOf(e, id)).x - x, spacing, 1e-3), `an arrow key moves the 1 cm spacing (${(await boxOf(e, id)).x - x} pt)`)
      // The gridlines: vertical lines one spacing apart on screen.
      const s = await scale(e)
      const ops = await e.frame().evaluate(() => window.__hand?.frame ?? [])
      const verticals = ops.filter((op, index) => op.op === 'move' && ops[index + 1]?.op === 'line' && Math.abs(ops[index + 1].at[0] - op.at[0]) < 0.01 && Math.abs(ops[index + 1].at[1] - op.at[1]) > 200 * s).map((op) => op.at[0]).sort((a, b) => a - b)
      assert(verticals.length > 20, `gridlines are drawn (${verticals.length})`)
      const gaps = verticals.slice(1, 6).map((v, index) => v - verticals[index])
      // The zoom shown is rounded to a whole percent: the gaps are equal and one spacing at about that zoom.
      assert(gaps.every((gap) => near(gap, gaps[0], 0.05)), `the gridlines are evenly spaced (${gaps.join(', ')})`)
      assert(near(gaps[0], spacing * s, spacing * 0.01), `one spacing apart on screen (${gaps[0]} for ${spacing * s})`)
    } finally {
      // Back to the defaults for the segments after.
      await e.kit.getByTestId('ribbon-tab-view').click()
      await e.kit.getByTestId('slides-view-gridSettings').click()
      await e.kit.getByTestId('slides-grid-defaults').click()
      await e.kit.getByTestId('slides-grid-apply').click()
      await e.wait(400)
    }
  }],
  // Near another shape's centre line the dragged shape snaps to it and the guide is drawn (W-C1).
  ['guide-centre', async (e) => {
    const [a, b] = await fresh(e, [[100, 100, 80, 60], [400, 320, 100, 50]])
    await gridSettings(e, { snapToGrid: false })
    const from = await e.toScreen(120, 110)
    const s = await scale(e)
    // A's centre goes to 448, 2 pt from B's (450).
    let drawn = false
    await drag(e, from, 308 * s, 0, { during: async () => { drawn = await guideAt(e, 450) } })
    assert(drawn, 'the centre line is drawn while the shape is near it')
    const moved = await boxOf(e, a)
    assert(near(moved.x + moved.w / 2, 450, 0.01), `the centres line up (${moved.x + moved.w / 2})`)
    assert.deepEqual((await boxOf(e, b)).x, 400, 'the other shape stays')
    await gridSettings(e, { snapToGrid: true })
  }],
  // A shape dragged near the spacing of the two before it lands at that spacing, and the gaps are marked (W-C2).
  ['equal-spacing', async (e) => {
    const [, , c] = await fresh(e, [[100, 100, 60, 60], [200, 100, 60, 60], [330, 100, 60, 60]])
    await gridSettings(e, { snapToGrid: false })
    const from = await e.toScreen(360, 110)
    const s = await scale(e)
    let marked = 0
    await drag(e, from, -28 * s, 0, {
      during: async () => {
        const box = await e.surface()
        const ops = await e.drawn()
        const y = (await e.toScreen(0, 130)).y - box.y
        // Lines across the gaps, halfway down the shapes.
        marked = ops.filter((op, index) => op.op === 'move' && ops[index + 1]?.op === 'line' && near(op.at[1], y, 1.5) && near(ops[index + 1].at[1], y, 1.5) && near(Math.abs(ops[index + 1].at[0] - op.at[0]), 40 * s, 1.5)).length
      },
    })
    assert(marked >= 2, `the two equal gaps are marked (${marked})`)
    assert.equal((await boxOf(e, c)).x, 300, 'the third shape lands 40 pt after the second')
    await gridSettings(e, { snapToGrid: true })
  }],
  // Alt drags a copy that lands at the pointer, off the grid; the original stays (W-A7).
  ['alt-drag-copy', async (e) => {
    const [id] = await fresh(e, [[101, 101, 160, 100]])
    const before = await boxOf(e, id)
    const from = await e.toScreen(120, 120)
    const s = await scale(e)
    await drag(e, from, Math.round(37 * s), 0, { keys: ['Alt'] })
    const shapes = await shapesOn(e)
    assert.equal(shapes.length, 2, 'a copy')
    assert.deepEqual(await boxOf(e, id), before, 'the original stays')
    const copy = shapes.find((shape) => shape.id !== id)
    assert(near(copy.box[0] - before.x, Math.round(37 * s) / s, 0.5), `the copy is where the pointer put it (${copy.box[0] - before.x} pt)`)
    assert.equal(copy.box[1], before.y, 'and not pulled onto the grid')
  }],
  // Arrow keys with snap to grid on (the default): one grid step, Shift and Alt the same, Cmd 10 pt (W-D1, W-D3, W-D4).
  ['arrow-steps', async (e) => {
    const [id] = await fresh(e, [[100, 100, 160, 100]])
    await select(e, 120, 120)
    const step = async (key) => {
      const x = (await boxOf(e, id)).x
      await press(e, key)
      return (await boxOf(e, id)).x - x
    }
    assert.equal(await step('ArrowRight'), 6, 'one grid step')
    assert.equal(await step('Shift+ArrowRight'), 6, 'Shift: the same')
    assert.equal(await step('Alt+ArrowRight'), 6, 'Alt: the same')
    assert.equal(await step('Meta+ArrowRight'), 10, 'Cmd: 10 pt')
  }],
  // Paste and Cmd+D offset 10 pt; Cmd+D on a copy the person moved repeats the move (W-I1, W-I3, W-I4).
  ['paste-duplicate-offset', async (e) => {
    const [id] = await fresh(e, [[102, 102, 160, 100]])
    await select(e, 120, 120)
    await press(e, 'ControlOrMeta+c', 400)
    await press(e, 'ControlOrMeta+v', 600)
    assert(await e.until(async () => (await shapesOn(e)).length === 2), 'pasted')
    const pasted = (await shapesOn(e)).at(-1)
    assert.deepEqual(pasted.box.slice(0, 2), [112, 112], 'a paste lands 10 pt across and down')
    await undo(e)
    await select(e, 120, 120)
    await press(e, 'ControlOrMeta+d', 500)
    const first = (await shapesOn(e)).at(-1)
    assert.deepEqual(first.box.slice(0, 2), [112, 112], 'Cmd+D: 10 pt across and down')
    for (let index = 0; index < 5; index += 1) await press(e, 'ArrowRight', 150)
    await press(e, 'ControlOrMeta+d', 500)
    await press(e, 'ControlOrMeta+d', 500)
    const shapes = await shapesOn(e)
    assert.equal(shapes.length, 4)
    const [, moved, second, third] = shapes.map((shape) => shape.box)
    assert.deepEqual([second[0] - moved[0], second[1] - moved[1]], [40, 10], 'the next Cmd+D repeats the copy\'s offset')
    assert.deepEqual([third[0] - second[0], third[1] - second[1]], [40, 10], 'and the one after')
    assert.equal(id, shapes[0].id)
  }],
  // A free rotation lands on a whole degree (W-B6).
  ['rotate-integer', async (e) => {
    const [id] = await fresh(e, [[330, 200, 300, 140]])
    await select(e, 350, 220)
    const centre = await e.toScreen(480, 270)
    const top = await e.toScreen(480, 200)
    const rotate = { x: top.x, y: top.y - 18 }
    const r = centre.y - rotate.y
    const angle = (23.4 * Math.PI) / 180
    await drag(e, rotate, r * Math.sin(angle), r - r * Math.cos(angle))
    const turned = (await boxOf(e, id)).rot
    assert(Number.isInteger(turned) && near(turned, 23, 2), `a whole degree (${turned}°)`)
  }],
]

await segments('snap', list, (which) => open({ which }))
