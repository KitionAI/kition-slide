// The slide's context menu (plan stage 6A; walkthrough C11, G9; WPS W-J3 to W-J9): a right-click
// chooses what it is for as the engine says (the object under the pointer, the selection kept, the
// caret kept, a table's cell), the menu holds WPS's rows in WPS's order, a choice runs its command,
// and Escape gives the slide its keyboard back. The floating toolbars (plan stage 6B; WPS W-J1,
// W-A13): selected words bring the text bar up, several objects the align bar; neither takes the
// keyboard, and a key, a scroll or Escape puts them away. A person's mouse and keys only; the
// slide is read back from the engine.
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

/** The floating toolbars (plan stage 6B). */
const mini = (e) => e.kit.getByTestId('slides-mini')
const alignBar = (e) => e.kit.getByTestId('slides-align-bar')
/** A slide point on "world" in a 400 × 120 rectangle at (100, 100) holding "Hello world", centred. */
const WORLD = [340, 160]
/** The text of the bold runs of `inspected` (`shape.inspect`), joined. */
function boldRuns(inspected) {
  const runs = []
  const walk = (node) => {
    if (Array.isArray(node)) node.forEach(walk)
    else if (node && typeof node === 'object') {
      if (typeof node.text === 'string' && node.props?.bold === true) runs.push(node.text)
      Object.values(node).forEach(walk)
    }
  }
  walk(inspected?.shape?.text)
  return runs.join('')
}

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
  ['w-j1-mini-bold', async (e) => {
    const [shape] = await fresh(e, [{ preset: 'rect', rect: [100, 100, 400, 120], text: 'Hello world' }])
    // A double click on "world" selects it, and the text bar comes up over it (WPS W-F6, W-J1).
    const at = await e.toScreen(WORLD[0], WORLD[1])
    await e.page.mouse.dblclick(at.x, at.y)
    assert.ok(await e.until(() => mini(e).isVisible()), 'the text bar comes up over the selected word')
    const inspected = await e.inspected()
    assert.equal(inspected.editingText, true)
    assert.equal(inspected.textSelected, true, 'the word is selected')
    const bar = await mini(e).boundingBox()
    assert.ok(bar.y + bar.height <= at.y, `the bar is above the word (${bar.y + bar.height} over ${at.y})`)
    for (const id of ['font', 'size', 'bold', 'italic', 'underline', 'color', 'alignLeft', 'alignCenter', 'alignRight', 'bullets']) {
      assert.ok(await e.kit.getByTestId(`slides-mini-${id}`).isVisible(), `the bar has ${id}`)
    }
    // Bold from the bar: the text keeps the keyboard and its selection, the bar stays.
    const bold = await e.kit.getByTestId('slides-mini-bold').boundingBox()
    await e.page.mouse.click(bold.x + bold.width / 2, bold.y + bold.height / 2)
    assert.ok(await e.until(async () => (await e.kit.getByTestId('slides-mini-bold').getAttribute('aria-pressed')) === 'true'), 'Bold is on for the word')
    assert.equal(await e.active(), 'textarea.kui-text-sink', 'the text still has the keyboard')
    assert.equal((await e.inspected()).textSelected, true, 'the word is still selected')
    assert.ok(await mini(e).isVisible(), 'the bar stays after a choice')
    // Typing replaces the word in bold, and puts the bar away.
    await e.page.keyboard.type('Zed')
    assert.ok(await e.until(async () => (await e.ask('text.get', { id: shape }))?.text === 'Hello Zed'), 'the typed word replaced the selected one')
    assert.ok(await e.until(async () => !(await mini(e).isVisible())), 'typing puts the bar away')
    assert.ok(boldRuns(await e.ask('shape.inspect', { id: shape })).includes('Zed'), 'the typed characters are bold')
    assert.ok(!boldRuns(await e.ask('shape.inspect', { id: shape })).includes('Hello'), 'the other word is not')
  }],
  ['w-j1-drag-select', async (e) => {
    await fresh(e, [{ preset: 'rect', rect: [100, 100, 400, 120], text: 'Hello world' }])
    // A drag through the words selects them (WPS W-F3); the bar comes up when it ends, not before.
    const from = await e.toScreen(WORLD[0] - 70, WORLD[1])
    const to = await e.toScreen(WORLD[0] + 20, WORLD[1])
    await e.page.mouse.move(from.x, from.y)
    await e.page.mouse.down()
    await e.page.mouse.move((from.x + to.x) / 2, to.y, { steps: 4 })
    await e.page.mouse.move(to.x, to.y, { steps: 4 })
    await e.wait(250)
    assert.equal(await mini(e).isVisible(), false, 'no bar while the drag goes on')
    await e.page.mouse.up()
    assert.ok(await e.until(() => mini(e).isVisible()), 'the bar comes up when the drag ends')
    // Its size list sets the size of the selection; the text keeps the keyboard.
    await e.kit.getByTestId('slides-mini-size').click()
    await e.kit.getByTestId('slides-mini-size-40').click()
    assert.ok(await e.until(async () => (await e.ask('format.state'))?.size === 40), 'the size list sets 40 pt')
    assert.equal(await e.active(), 'textarea.kui-text-sink', 'the text still has the keyboard')
    // Escape puts the bar away and goes on to the text (its first Escape leaves the editing).
    await e.page.keyboard.press('Escape')
    assert.ok(await e.until(async () => !(await mini(e).isVisible())), 'Escape puts the bar away')
    assert.ok(await e.until(async () => (await e.state()).editing === false), 'and leaves the text, as without the bar')
    // A caret alone has no bar.
    await e.click(WORLD[0], WORLD[1])
    assert.ok(await e.until(async () => (await e.state()).editing))
    await e.wait(250)
    assert.equal(await mini(e).isVisible(), false, 'a caret has no bar')
  }],
  ['w-a13-align-bar', async (e) => {
    const [a, b] = await fresh(e, [
      { preset: 'rect', rect: [100, 100, 160, 100] },
      { preset: 'ellipse', rect: [400, 180, 160, 100] },
    ])
    // One object: no bar beside it (WPS's button column, W-A14, is left out).
    await e.click(180, 150)
    assert.equal((await e.state()).selected, 1)
    await e.wait(250)
    assert.equal(await alignBar(e).isVisible(), false, 'a single object has no bar')
    // Two: the align bar comes up above them.
    await e.page.keyboard.down('Shift')
    await e.click(480, 230)
    await e.page.keyboard.up('Shift')
    assert.ok(await e.until(() => alignBar(e).isVisible()), 'two objects bring the align bar up')
    const box = await alignBar(e).boundingBox()
    const top = await e.toScreen(100, 100)
    assert.ok(box.y + box.height <= top.y, 'above the objects')
    for (const id of ['left', 'center', 'right', 'top', 'middle', 'bottom', 'distribute-horizontal', 'distribute-vertical', 'group']) {
      assert.ok(await e.kit.getByTestId(`slides-align-bar-${id}`).isVisible(), `the bar has ${id}`)
    }
    await e.kit.getByTestId('slides-align-bar-top').click()
    const tops = async () => (await Promise.all([a, b].map((id) => e.ask('shape.inspect', { id })))).map((item) => item?.shape?.box?.[1] ?? item?.box?.[1])
    assert.ok(await e.until(async () => { const [ya, yb] = await tops(); return ya === yb }), `Align Top lined their tops up (${await tops()})`)
    assert.ok(await e.until(() => alignBar(e).isVisible()), 'the bar stays for the same objects')
    // A key puts it away; the nudge it makes does not bring it back.
    await e.page.keyboard.press('ArrowRight')
    assert.ok(await e.until(async () => !(await alignBar(e).isVisible())), 'a key puts the bar away')
    await e.wait(400)
    assert.equal(await alignBar(e).isVisible(), false, 'and it stays away for them')
    // Another selection brings it up again: nothing, then Select All. Group groups them, and one
    // group has no bar.
    await e.click(800, 450)
    assert.equal((await e.state()).selected, 0)
    await e.page.keyboard.press('ControlOrMeta+a')
    assert.ok(await e.until(() => alignBar(e).isVisible()), 'Select All brings the align bar up')
    await e.kit.getByTestId('slides-align-bar-group').click()
    assert.ok(await e.until(async () => (await e.state()).selected === 1 && (await order(e)).length === 1), 'Group grouped them')
    assert.ok(await e.until(async () => !(await alignBar(e).isVisible())), 'a group alone has no bar')
  }],
  ['w-j1-scroll-hides', async (e) => {
    await fresh(e, [{ preset: 'rect', rect: [100, 100, 400, 120], text: 'Hello world' }])
    const at = await e.toScreen(WORLD[0], WORLD[1])
    await e.page.mouse.dblclick(at.x, at.y)
    assert.ok(await e.until(() => mini(e).isVisible()))
    // A small scroll that keeps the slide (and the selection) where they are.
    await e.page.mouse.move(at.x, at.y + 200)
    await e.page.mouse.wheel(0, 8)
    assert.ok(await e.until(async () => !(await mini(e).isVisible())), 'a scroll puts the bar away')
    assert.equal((await e.inspected()).textSelected, true, 'the selection stays')
  }],
], (which) => open({ which }))
