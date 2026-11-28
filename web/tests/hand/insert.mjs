// Inserting by hand, as WPS does (plan stage 9; walkthrough F4, F7, F9; WPS W-E1, W-E2, W-E4–W-E7,
// W-E9, W-E10): every shape in the gallery is drawn by dragging it out (a click makes it 72 × 72 pt),
// the gallery shows WPS's nine groups of icons, a shape locks for drawing one after another, the Text
// Box tool's click makes a 320 pt box and its drag sets the width, a double-click on the empty slide
// makes a text box, a pasted picture goes into the slide's empty content placeholder whole, and
// Insert › Table is a grid to sweep over. A person's mouse and keys only; what changed is read back
// from the engine.
// node tests/hand/insert.mjs [chromium|webkit] [segment…]
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { open, segments } from './harness.mjs'

const near = (a, b, within, what) => assert(Math.abs(a - b) <= within, `${what}: ${a} (want ${b} ± ${within})`)
const press = async (e, key, ms = 250) => {
  await e.page.keyboard.press(key)
  await e.wait(ms)
}

/** A new slide of `layout` (made current) with nothing selected. */
async function fresh(e, layout = 'Blank') {
  await e.agent([{ command: 'slide.new', params: { layout, at: 1 } }])
  await e.goto(1)
  await e.click(930, 20)
  await e.until(async () => (await e.state()).selected === 0)
}

const shapesOn = async (e) => (await e.ask('slide.inspect', { index: (await e.state()).slide, select: false }))?.shapes ?? []
/** The shapes on the slide now that were not in `before`. */
const added = async (e, before) => (await shapesOn(e)).filter((shape) => !before.some((old) => old.id === shape.id))

/** Presses at slide point `from`, moves to slide point `to` in steps, releases. */
async function drag(e, from, to) {
  const [a, b] = [await e.toScreen(...from), await e.toScreen(...to)]
  await e.page.mouse.move(a.x, a.y)
  await e.page.mouse.down()
  for (let step = 1; step <= 8; step += 1) await e.page.mouse.move(a.x + ((b.x - a.x) * step) / 8, a.y + ((b.y - a.y) * step) / 8)
  await e.page.mouse.up()
  await e.wait(400)
}

/** Opens the Shapes gallery on the tab shown; answers it. */
async function gallery(e, tab = null) {
  if (tab) {
    await e.kit.getByTestId(`ribbon-tab-${tab}`).click()
    await e.wait(250)
  }
  await e.kit.locator('[data-testid="slides-shapes"]:visible').first().click()
  const menu = e.kit.getByTestId('slides-shape-gallery')
  await menu.waitFor()
  return menu
}

/** The picture another app put on the clipboard, pasted with the keyboard on the editor (as the browser sends it). */
const pastePicture = (e, width, height) =>
  e.frame().evaluate(async ({ width, height }) => {
    const canvas = document.createElement('canvas')
    canvas.width = width
    canvas.height = height
    canvas.getContext('2d').fillRect(0, 0, width, height)
    const blob = await new Promise((resolve) => canvas.toBlob(resolve, 'image/png'))
    const data = new DataTransfer()
    data.items.add(new File([blob], 'image.png', { type: 'image/png' }))
    const event = new ClipboardEvent('paste', { clipboardData: data, bubbles: true, cancelable: true, composed: true })
    if (event.clipboardData !== data) Object.defineProperty(event, 'clipboardData', { value: data })
    ;(document.activeElement ?? document.body).dispatchEvent(event)
    return event.defaultPrevented
  }, { width, height })

const list = [
  // F4: a heart from the gallery is dragged out to the dragged rectangle; a click makes 72 × 72 pt
  // with its top left at the click; the tool goes back to Select with the new shape selected (W-E7, W-E9).
  ['f4-preset-drag', async (e) => {
    await fresh(e)
    let before = await shapesOn(e)
    await (await gallery(e, 'insert')).getByTestId('slides-shape-heart').click()
    await e.wait(200)
    await drag(e, [200, 150], [380, 300])
    let made = await added(e, before)
    assert.equal(made.length, 1, 'one shape')
    assert.equal(made[0].preset, 'heart', 'a heart')
    const [x, y, w, h] = made[0].box
    near(x, 200, 1.5, 'x')
    near(y, 150, 1.5, 'y')
    near(w, 180, 1.5, 'the dragged width')
    near(h, 150, 1.5, 'the dragged height')
    assert.equal((await e.state()).selected, 1, 'the new heart is selected')
    // The tool is Select again: a drag on the empty slide draws nothing.
    before = await shapesOn(e)
    await drag(e, [500, 400], [560, 440])
    assert.equal((await added(e, before)).length, 0, 'back to Select after one shape')
    await e.click(930, 20)
    before = await shapesOn(e)
    await (await gallery(e, 'insert')).getByTestId('slides-shape-flowChartDecision').click()
    await e.wait(200)
    await e.click(600, 120)
    await e.wait(300)
    made = await added(e, before)
    assert.equal(made[0]?.preset, 'flowChartDecision')
    near(made[0].box[0], 600, 1.5, 'its left at the click')
    near(made[0].box[1], 120, 1.5, 'its top at the click')
    assert.deepEqual(made[0].box.slice(2), [72, 72], 'a click: 72 × 72 pt')
  }],
  // The gallery: WPS's nine groups, every shape an icon drawn from its geometry, named in the tooltip.
  ['gallery-groups', async (e) => {
    await fresh(e)
    const menu = await gallery(e, 'insert')
    const groups = await menu.locator('[role="group"]').evaluateAll((items) => items.map((item) => item.getAttribute('aria-label')))
    assert.deepEqual(groups, ['Lines', 'Rectangles', 'Basic Shapes', 'Block Arrows', 'Equation Shapes', 'Flowchart', 'Stars and Banners', 'Callouts', 'Action Buttons'])
    const presets = await e.ask('shape.presets')
    const cells = await menu.locator('.slides-shapes-gallery-cell').evaluateAll((items) => items.map((item) => ({ id: item.dataset.testid, label: item.getAttribute('aria-label'), paths: [...item.querySelectorAll('path')].filter((path) => (path.getAttribute('d') ?? '').length > 2).length, icon: !!item.querySelector('svg') })))
    assert.equal(cells.length, presets.length + 3, 'every preset and the three freeform tools')
    for (const cell of cells) {
      assert(cell.icon, `${cell.id} has an icon`)
      assert(cell.label && !cell.label.startsWith('shape.'), `${cell.id} is named (${cell.label})`)
    }
    for (const preset of presets) assert((cells.find((cell) => cell.id === `slides-shape-${preset.name}`)?.paths ?? 0) > 0, `${preset.name} is drawn from its geometry`)
    await press(e, 'Escape')
    // Chinese names for every shape too.
    const strings = (locale, file) => JSON.parse(readFileSync(new URL(`../../src/i18n/locales/${locale}/${file}.json`, import.meta.url), 'utf8'))
    const zh = { ...strings('zh-CN', 'format'), ...strings('zh-CN', 'shapes') }
    for (const name of [...presets.map((preset) => preset.name), 'curve', 'freeform', 'scribble']) assert(zh[`shape.${name}`], `${name} has a Chinese name`)
  }],
  // W-E10: a right-click on a gallery shape offers Lock Drawing Mode; locked, shape after shape is drawn; Escape ends it.
  ['lock-drawing', async (e) => {
    await fresh(e)
    let before = await shapesOn(e)
    const menu = await gallery(e, 'insert')
    await menu.getByTestId('slides-shape-ellipse').click({ button: 'right' })
    await e.kit.getByTestId('slides-shape-lock').click()
    await e.wait(250)
    await drag(e, [100, 100], [180, 160])
    await drag(e, [300, 100], [380, 160])
    await e.click(500, 120)
    const made = await added(e, before)
    assert.equal(made.length, 3, `three ovals with the tool locked (${made.length})`)
    assert(made.every((shape) => shape.preset === 'ellipse'))
    await press(e, 'Escape', 300)
    before = await shapesOn(e)
    await e.click(930, 20)
    await drag(e, [600, 380], [700, 440])
    assert.equal((await added(e, before)).length, 0, 'after Escape the tool is Select')
  }],
  // W-E1, W-E2, W-E4: the Text Box tool's click makes a box 320 pt wide, one line high, typed in at
  // once, wrapping; its drag makes it as wide as the drag; Escape with the tool chosen puts it down.
  ['e1-text-box-width', async (e) => {
    await fresh(e)
    await e.kit.getByTestId('ribbon-tab-insert').click()
    await e.kit.getByTestId('slides-insert-textBox').click()
    await e.click(100, 100)
    assert((await e.state()).editing, 'typing in the new box')
    await e.page.keyboard.type('A box as wide as WPS makes one, with words enough to wrap onto a second line')
    await e.wait(400)
    let box = (await shapesOn(e)).find((shape) => shape.kind === 'textBox')
    assert(box, 'a text box')
    near(box.box[0], 100, 1.5, 'its left at the click')
    near(box.box[1], 100, 1.5, 'its top at the click')
    assert.equal(box.box[2], 320, 'exactly 320 pt wide')
    assert(box.box[3] > 40, `the text wraps and the box grows down (${box.box[3]})`)
    await press(e, 'Escape')
    await press(e, 'Escape')
    await e.kit.getByTestId('ribbon-tab-insert').click()
    await e.kit.getByTestId('slides-insert-textBox').click()
    const before = await shapesOn(e)
    await drag(e, [100, 300], [230, 320])
    await e.page.keyboard.type('Narrow')
    await e.wait(300)
    box = (await added(e, before))[0]
    near(box?.box[2] ?? 0, 130, 1.5, 'the dragged width')
    assert(box.box[3] < 40, `one line high (${box.box[3]})`)
    await press(e, 'Escape')
    await press(e, 'Escape')
    // Chosen and put down with Escape: a click makes nothing.
    await e.kit.getByTestId('ribbon-tab-insert').click()
    await e.kit.getByTestId('slides-insert-textBox').click()
    await press(e, 'Escape')
    const now = await shapesOn(e)
    await e.click(600, 450)
    assert.equal((await added(e, now)).length, 0, 'Escape put the tool down')
  }],
  // W-E5: a double-click on the empty slide makes a text box there, typed in at once.
  ['e5-double-click-text-box', async (e) => {
    await fresh(e)
    const before = await shapesOn(e)
    const at = await e.toScreen(240, 380)
    await e.page.mouse.dblclick(at.x, at.y)
    await e.wait(300)
    assert((await e.state()).editing, 'typing in a new box')
    await e.page.keyboard.type('Two clicks')
    await e.wait(300)
    const made = await added(e, before)
    assert.equal(made.length, 1)
    assert.equal(made[0].text, 'Two clicks')
    near(made[0].box[0], 240, 1.5, 'its left at the double-click')
    near(made[0].box[1], 380, 1.5, 'its top at the double-click')
    assert.equal(made[0].box[2], 320, 'exactly 320 pt wide')
    await press(e, 'Escape')
    await press(e, 'Escape')
  }],
  // F7: a pasted picture goes into the slide's empty content placeholder whole, keeping its
  // proportions, in its middle; the placeholder's prompt goes; one Cmd+Z brings the placeholder back.
  ['f7-picture-placeholder', async (e) => {
    await fresh(e, 'titleAndContent')
    const content = (await shapesOn(e)).find((shape) => shape.placeholder === 'obj')
    assert(content, 'an empty content placeholder')
    assert(await pastePicture(e, 480, 320), 'the editor took the paste')
    assert(await e.until(async () => (await shapesOn(e)).some((shape) => shape.kind === 'picture')), 'a picture')
    const shapes = await shapesOn(e)
    const picture = shapes.find((shape) => shape.kind === 'picture')
    assert.equal(picture.id, content.id, 'the placeholder became the picture')
    assert(!shapes.some((shape) => shape.kind === 'placeholder' && shape.placeholder === 'obj'), 'no empty content placeholder (no prompt) is left')
    const [x, y, w, h] = picture.box
    const [px, py, pw, ph] = content.box
    near(w / h, 1.5, 0.01, 'its proportions')
    assert(w <= pw + 0.5 && h <= ph + 0.5, 'inside the placeholder')
    assert(Math.abs(w - pw) < 0.5 || Math.abs(h - ph) < 0.5, 'as large as fits')
    near(x + w / 2, px + pw / 2, 0.5, 'in its middle across')
    near(y + h / 2, py + ph / 2, 0.5, 'and down')
    await press(e, 'ControlOrMeta+z', 500)
    assert((await shapesOn(e)).some((shape) => shape.id === content.id && shape.kind === 'placeholder'), 'one Cmd+Z brings the placeholder back')
  }],
  // F9: Insert › Table is a grid to sweep over: three rows by four columns, then a click; Insert Table… asks.
  ['f9-table-grid', async (e) => {
    await fresh(e)
    let before = await shapesOn(e)
    await e.kit.getByTestId('ribbon-tab-insert').click()
    await e.kit.getByTestId('slides-table').first().click()
    const grid = e.kit.getByTestId('slides-table-grid')
    await grid.waitFor()
    await grid.locator('[data-cell="3x4"]').hover()
    await e.wait(150)
    assert.equal(await e.kit.getByTestId('slides-table-size').textContent(), '3 × 4 Table', 'the heading says the size')
    assert.equal(await grid.locator('[data-lit]').count(), 12, 'twelve cells light up')
    await grid.locator('[data-cell="3x4"]').click()
    assert(await e.until(async () => (await added(e, before)).length === 1), 'a table')
    let table = (await added(e, before))[0]
    assert.deepEqual([table.table?.rows, table.table?.cols], [3, 4], 'three rows, four columns')
    await e.click(930, 20)
    before = await shapesOn(e)
    await e.kit.getByTestId('ribbon-tab-insert').click()
    await e.kit.getByTestId('slides-table').first().click()
    await e.kit.getByTestId('slides-table-dialog-open').click()
    await e.kit.getByTestId('slides-table-dialog-cols').fill('6')
    await e.kit.getByTestId('slides-table-dialog-rows').fill('2')
    await e.kit.getByTestId('slides-table-dialog-apply').click()
    assert(await e.until(async () => (await added(e, before)).length === 1), 'a table from the dialog')
    table = (await added(e, before))[0]
    assert.deepEqual([table.table?.rows, table.table?.cols], [2, 6], 'two rows, six columns')
  }],
]

await segments('insert', list, (which) => open({ which }))
