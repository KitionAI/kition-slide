// The slide's context menu (plan stage 6A; walkthrough C11, G9; WPS W-J3 to W-J9): a right-click
// chooses what it is for as the engine says (the object under the pointer, the selection kept, the
// caret kept, a table's cell), the menu holds WPS's rows in WPS's order, a choice runs its command,
// and Escape gives the slide its keyboard back. A person's mouse and keys only; the slide is read
// back from the engine.
// node tests/hand/context-menu.mjs [chromium|webkit] [segment…]
import assert from 'node:assert/strict'
import { open, segments } from './harness.mjs'

/** A new blank slide (made current) with `shapes` on it, nothing selected; answers their ids. */
async function fresh(e, shapes) {
  await e.agent([{ command: 'slide.new', params: { layout: 'Blank', at: 1 } }])
  await e.goto(1)
  const ids = []
  for (const shape of shapes) {
    const { table, textBox, ...params } = shape
    const answer = await e.ask(table ? 'table.insert' : textBox ? 'textBox.insert' : 'shape.insert', params)
    ids.push(answer?.id)
  }
  await e.click(930, 20)
  await e.until(async () => (await e.state()).selected === 0)
  return ids
}

const menu = (e) => e.kit.getByTestId('slides-context-menu')
const row = (e, id) => e.kit.getByTestId(`slides-cm-${id}`)

/** Right-clicks slide point (`x`, `y`) and waits for the menu. */
async function rightClick(e, x, y) {
  const at = await e.toScreen(x, y)
  await e.page.mouse.click(at.x, at.y, { button: 'right' })
  assert.ok(await e.until(() => menu(e).isVisible()), 'the context menu opens')
  await e.wait(150)
}

/** The selected shapes' ids. */
const selection = async (e) => (await e.ask('file.inspect'))?.selection?.shapes ?? []
/** The current slide's shape ids, bottom first. */
const order = async (e) => ((await e.ask('slide.inspect', { index: 1, select: false }))?.shapes ?? []).map((shape) => shape.id)

const TWO = [
  { preset: 'rect', rect: [100, 100, 200, 120], text: 'A' },
  { preset: 'ellipse', rect: [400, 100, 200, 120], text: 'B' },
]

await segments('context-menu', [
  ['c11-right-click-selects', async (e) => {
    const [a] = await fresh(e, TWO)
    await rightClick(e, 120, 110)
    assert.deepEqual(await selection(e), [a], 'the right-clicked shape is selected')
    assert.equal((await e.state()).editing, false, 'a right-click does not enter its text')
    // WPS's rows for a shape, in its order (W-J3, W-J4).
    for (const id of ['copy', 'cut', 'paste', 'pasteText', 'delete', 'group', 'ungroup', 'bringToFront', 'bringForward', 'sendToBack', 'sendBackward', 'editText', 'changeShape', 'link', 'setDefault', 'formatObject']) {
      assert.ok(await row(e, id).isVisible(), `the menu has ${id}`)
    }
    const labels = await menu(e).locator(':scope > .kui-menu-item, :scope > .slides-cm-pair > .kui-menu-item:first-child').allTextContents()
    assert.deepEqual(labels.slice(0, 4), ['Delete', 'Group', 'Bring to Front', 'Send to Back'], labels.join(' | '))
    assert.match(await row(e, 'link').textContent(), /K/, 'Link shows its shortcut')
    // The menu has the keyboard (not the shape's text, which takes typing on a selected shape).
    assert.match(await e.frame().evaluate(() => document.activeElement?.getAttribute('data-testid') ?? ''), /^slides-cm-/, 'a row of the menu has the keyboard')
    await e.page.keyboard.type('q')
    assert.equal((await e.ask('text.get', { id: a }))?.text, 'A', 'keys pressed in the menu are not typed on the slide')
    assert.ok(await menu(e).isVisible())
    // Escape closes it; the slide has the keyboard again, so Delete deletes the shape.
    await e.page.keyboard.press('Escape')
    assert.ok(await e.until(async () => !(await menu(e).isVisible())), 'Escape closes the menu')
    assert.deepEqual(await selection(e), [a], 'Escape in the menu keeps the selection')
    await e.page.keyboard.press('Delete')
    assert.ok(await e.until(async () => !(await order(e)).includes(a)), 'Delete after the menu deletes the shape')
  }],
  ['c11-keeps-several', async (e) => {
    const [a, b] = await fresh(e, TWO)
    await e.click(120, 110)
    await e.page.keyboard.down('Shift')
    await e.click(500, 190)
    await e.page.keyboard.up('Shift')
    assert.equal((await selection(e)).length, 2)
    await rightClick(e, 500, 200)
    assert.deepEqual([...(await selection(e))].sort(), [a, b].sort(), 'a right-click on one of them keeps both')
    assert.ok(await row(e, 'group').isEnabled(), 'Group is there for the two')
    await row(e, 'group').click()
    assert.ok(await e.until(async () => (await e.state()).selected === 1 && (await order(e)).length === 1), 'Group grouped them')
    assert.ok(await e.until(async () => !(await menu(e).isVisible())))
  }],
  ['c11-bring-to-front', async (e) => {
    const [low, high] = await fresh(e, [
      { preset: 'rect', rect: [100, 100, 200, 160] },
      { preset: 'rect', rect: [250, 150, 200, 160] },
    ])
    assert.deepEqual(await order(e), [low, high])
    await rightClick(e, 120, 120)
    assert.deepEqual(await selection(e), [low])
    await row(e, 'bringToFront').click()
    assert.ok(await e.until(async () => JSON.stringify(await order(e)) === JSON.stringify([high, low])), 'Bring to Front put it on top')
  }],
  ['c11-text-keeps-caret', async (e) => {
    const [box] = await fresh(e, [{ textBox: true, rect: [100, 100, 300, 60], text: 'Hello' }])
    await e.click(200, 113)
    assert.ok(await e.until(async () => (await e.state()).editing), 'a click on the words edits them')
    await e.page.keyboard.press('End')
    await e.page.keyboard.type('X')
    await rightClick(e, 160, 113)
    assert.equal((await e.state()).editing, true, 'a right-click in the text keeps it edited')
    for (const id of ['bulletsNumbering', 'exitText', 'link', 'formatObject']) assert.ok(await row(e, id).isVisible(), `the text menu has ${id}`)
    await e.page.keyboard.press('Escape')
    assert.ok(await e.until(async () => !(await menu(e).isVisible())))
    await e.page.keyboard.type('Y')
    assert.ok(await e.until(async () => (await e.ask('text.get', { id: box }))?.text === 'HelloXY'), 'the caret stayed where it was')
  }],
  ['c11-table-insert-row', async (e) => {
    const [table] = await fresh(e, [{ table: true, rows: 2, cols: 2, rect: [100, 100, 400, 80] }])
    await rightClick(e, 400, 160)
    const state = await e.inspected()
    assert.equal(state.editingText, true, 'the caret is in the cell right-clicked')
    for (const id of ['insert', 'deleteRows', 'deleteColumns', 'mergeCells', 'splitCells', 'link', 'selectTable']) assert.ok(await row(e, id).isVisible(), `the cell menu has ${id}`)
    await row(e, 'insert').hover()
    assert.ok(await e.until(() => row(e, 'insertRowBelow').isVisible()), 'Insert opens its submenu')
    await row(e, 'insertRowBelow').click()
    assert.ok(await e.until(async () => (await e.ask('table.state', { id: table }))?.rows === 3), 'a row was inserted')
    assert.ok(await e.until(async () => !(await menu(e).isVisible())))
  }],
  ['c11-empty-slide', async (e) => {
    await fresh(e, TWO)
    await e.click(120, 110)
    await rightClick(e, 800, 450)
    assert.equal((await e.state()).selected, 0, 'a right-click on the empty slide selects nothing')
    for (const id of ['background', 'layout', 'reset', 'gridGuides', 'ruler']) assert.ok(await row(e, id).isVisible(), `the slide menu has ${id}`)
    assert.equal(await row(e, 'copy').isEnabled(), false, 'nothing to copy')
    await row(e, 'ruler').click()
    assert.ok(await e.until(() => e.kit.getByTestId('slides-ruler-x').isVisible()), 'Ruler shows the rulers')
    // A press elsewhere closes a menu too.
    await rightClick(e, 800, 450)
    await e.click(700, 480)
    assert.ok(await e.until(async () => !(await menu(e).isVisible())), 'a press outside closes it')
    await rightClick(e, 800, 450)
    await row(e, 'ruler').click()
  }],
  ['c11-shift-f10', async (e) => {
    const [a] = await fresh(e, TWO)
    await e.click(120, 110)
    await e.page.keyboard.press('Shift+F10')
    assert.ok(await e.until(() => menu(e).isVisible()), 'Shift+F10 opens the menu')
    assert.deepEqual(await selection(e), [a], 'for the selection as it is')
    const box = await menu(e).boundingBox()
    const at = await e.toScreen(200, 160)
    assert.ok(Math.abs(box.x - at.x) < 4 && Math.abs(box.y - at.y) < 4, `it opens at the middle of the shape (${box.x},${box.y} for ${at.x},${at.y})`)
    // Down to Delete and Enter: the keys are the menu's.
    const focused = () => e.frame().evaluate(() => document.activeElement?.getAttribute('data-testid'))
    assert.equal(await focused(), 'slides-cm-copy', 'the first row has the keyboard')
    for (let index = 0; index < 6 && (await focused()) !== 'slides-cm-delete'; index += 1) await e.page.keyboard.press('ArrowDown')
    assert.equal(await focused(), 'slides-cm-delete')
    await e.page.keyboard.press('Enter')
    assert.ok(await e.until(async () => !(await order(e)).includes(a)), 'Delete from the keyboard deleted it')
  }],
], (which) => open({ which }))
