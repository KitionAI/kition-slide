// A selected object's handles show on any slide (plan 0A; walkthrough C2; WPS W-B1): eight white
// circles with a dark ring, a grey rotation handle, a yellow diamond for each adjust handle. Read
// from what is drawn over the slide, not from pixels.
// node tests/hand/handles.mjs [chromium|webkit] [segment…]
import assert from 'node:assert/strict'
import { open, segments } from './harness.mjs'

/** The circles and diamonds of the last frame: each path with its fill and stroke. */
function shapesOf(ops) {
  const out = []
  let path = null
  for (const op of ops) {
    if (op.op === 'begin') path = { arcs: [], lines: 0, fill: null, stroke: null }
    else if (!path) continue
    else if (op.op === 'arc') path.arcs.push(op)
    else if (op.op === 'line') path.lines += 1
    else if (op.op === 'fill') path.fill = op.style
    else if (op.op === 'stroke') {
      path.stroke = op.style
      out.push(path)
      path = null
    }
  }
  return out
}

const list = [
  ['c2-handles-visible', async (e) => {
    const { agent, until, click, drawn, wait } = e
    // A star has an adjust handle as well.
    await agent([{ command: 'slide.new', params: { layout: 'Blank', at: 2 } }, { command: 'shape.insert', params: { preset: 'star5', rect: [330, 120, 300, 300] } }])
    await e.goto(2)
    await click(480, 280)
    assert(await until(async () => (await e.state()).selected === 1), 'the shape is selected')
    await wait(300)
    const paths = shapesOf(await drawn())
    const resize = paths.filter((path) => path.arcs.length === 1 && Math.abs(path.arcs[0].r - 4.5) < 0.01)
    assert.equal(resize.length, 8, `eight round resize handles (${resize.length})`)
    for (const handle of resize) {
      assert.equal(handle.fill, '#ffffff', 'a resize handle is white')
      assert.notEqual(handle.stroke, handle.fill, `a resize handle's ring (${handle.stroke}) differs from its fill`)
    }
    const rotate = paths.filter((path) => path.arcs.length === 1 && path.arcs[0].r > 5 && path.fill && path.fill !== '#ffffff')
    assert.equal(rotate.length, 1, 'one grey rotation handle')
    const diamonds = paths.filter((path) => path.arcs.length === 0 && path.lines === 3 && path.fill === '#ffd23f')
    assert.equal(diamonds.length, 1, 'one yellow adjust diamond')
    assert.notEqual(diamonds[0].stroke, diamonds[0].fill, 'the diamond has a ring')
  }],
]

await segments('handles', list, (which) => open({ which }))
