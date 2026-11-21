// Real pointer, keyboard, ribbon and PPTX persistence against the worker engine.
import assert from 'node:assert/strict'
import { mkdir } from 'node:fs/promises'
import { chromium, webkit } from 'playwright'
import { createServer } from 'vite'

const which = process.argv[2] ?? 'chromium'
const server = await createServer({ configFile: new URL('../vite.config.ts', import.meta.url).pathname, server: { port: 0, host: '127.0.0.1' }, logLevel: 'error' })
await server.listen()
const browser = await (which === 'webkit' ? webkit : chromium).launch()
const page = await browser.newPage({ viewport: { width: 1800, height: 1000 } })
const kit = page.frameLocator('#kit')
const errors = []
page.on('pageerror', (error) => errors.push(error.message))
const run = (command, params = {}) => page.evaluate(async ({ command, params }) => (await window.devHost.connection().ai.run([{ command, params }], false)).completed[0].result, { command, params })
const until = async (check, message) => {
  for (let n = 0; n < 100; n += 1) { if (await check().catch(() => false)) return; await page.waitForTimeout(100) }
  assert.fail(message)
}
const tab = (name) => kit.getByTestId(`ribbon-tab-${name}`).click()
const state = () => run('table.state')
const tables = async () => (await run('slide.inspect')).shapes.filter((s) => s.kind === 'table')
const cellText = async (r, c) => (await tables())[0].table.cells[r][c]
const clickCell = async (r, c, shift = false) => {
  const table = (await tables())[0]
  const bounds = await kit.getByTestId('kit-canvas-surface').boundingBox()
  const zoom = Number((await kit.getByTestId('kit-zoom').textContent()).replace('%', '')) / 100
  const [x, y, w, h] = table.box
  const point = {
    x: bounds.x + bounds.width / 2 + (x + (c + 0.4) * w / table.table.cols - 480) * zoom,
    y: bounds.y + bounds.height / 2 + (y + (r + 0.4) * h / table.table.rows - 270) * zoom,
  }
  if (shift) await page.keyboard.down('Shift')
  await page.mouse.click(point.x, point.y)
  if (shift) await page.keyboard.up('Shift')
  else await until(() => kit.getByTestId('kit-text-sink').evaluate((element) => element.ownerDocument.activeElement === element), 'canvas text input takes focus')
}

try {
  await page.goto(`http://127.0.0.1:${server.httpServer.address().port}/dev.html?modes=classic&locale=en-US`)
  await page.waitForFunction(() => document.getElementById('status')?.textContent?.includes(' open in '), null, { timeout: 60000 })
  await run('slide.new', { layout: 'blank' })
  const tableSlide = (await run('document.inspect')).selection.slide
  await tab('insert')
  await kit.getByTestId('slides-table').click()
  await kit.getByRole('menuitem', { name: 'Insert Table 3 × 3', exact: true }).click()
  await until(async () => (await tables()).length === 1, 'table insertion')
  await clickCell(0, 0)
  await until(async () => (await run('document.inspect')).selection.text?.cell?.join() === '0,0', 'pointer enters cell text')
  if (which === 'chromium') {
    const cdp = await page.context().newCDPSession(page)
    await cdp.send('Input.imeSetComposition', { text: 'rev', selectionStart: 3, selectionEnd: 3 })
    assert.equal(await cellText(0, 0), '', 'composition stays transient until committed')
    await cdp.send('Input.insertText', { text: 'Revenue' })
  } else {
    await page.keyboard.insertText('Revenue')
  }
  await until(async () => (await cellText(0, 0)) === 'Revenue', 'typing in the first cell')
  await page.keyboard.press('Tab')
  await page.keyboard.insertText('Quarter')
  await until(async () => (await cellText(0, 1)) === 'Quarter', 'Tab enters the next cell')
  await page.keyboard.press('Shift+Tab')
  await page.keyboard.insertText('Sales')
  await until(async () => (await cellText(0, 0)) === 'Sales', 'Shift+Tab selects previous text')

  console.log('PASS pointer cell input, composition and Tab navigation')
  await tab('tableLayout')
  const panel = await kit.getByRole('tabpanel').boundingBox()
  const lastSizeButton = await kit.getByTestId('slides-cmd-table.distributeColumns').boundingBox()
  assert.ok(lastSizeButton.y + lastSizeButton.height <= panel.y + panel.height, 'size controls are not clipped')
  await kit.getByTestId('slides-cmd-table.insertRowBelow').click()
  await until(async () => (await state()).rows === 4, 'insert row with no parameter dialog')
  assert.equal(await kit.getByRole('dialog').count(), 0)
  // Ribbon, pointer and menu edits are the person's steps, so the frame's Undo and Redo take them
  // back (an agent's edit.undo takes back only the agent's steps: KitModel 1.1 M12a).
  await kit.getByTestId('slides-cmd-table.deleteRow').click()
  await until(async () => (await state()).rows === 3, 'delete row')
  await kit.getByTestId('slides-undo').click()
  await until(async () => (await state()).rows === 4, 'undo row deletion')
  await kit.getByTestId('slides-redo').click()
  await until(async () => (await state()).rows === 3, 'redo row deletion')
  await kit.getByTestId('slides-cmd-table.insertColumnRight').click()
  await until(async () => (await state()).cols === 4, 'insert column')
  await kit.getByTestId('slides-cmd-table.deleteColumn').click()
  await until(async () => (await state()).cols === 3, 'delete column')

  await clickCell(0, 0)
  await page.keyboard.insertText('Editable')
  await until(async () => (await cellText(0, 0)).includes('Editable'), 'typing after ribbon commands')
  await clickCell(0, 1, true)
  await until(async () => (await state()).canMerge, 'Shift-click selects a cell range')
  await kit.getByTestId('slides-cmd-table.merge').click()
  await until(async () => (await state()).canSplit, 'merge selected cells')
  await kit.getByTestId('slides-table-align-center').click()
  await until(async () => (await state()).anchor === 'center', 'cell vertical alignment')
  await tab('tableDesign')
  await kit.getByTestId('slides-table-style').selectOption('grid')
  await kit.getByTestId('slides-table-shading').click()
  await kit.getByTestId('slides-table-colors').getByRole('button', { name: '#FFFF00', exact: true }).click()
  await tab('tableDesign')
  await until(async () => (await state()).style === 'grid', 'table style')

  console.log('PASS native table layout and design controls, undo/redo and merged selection')
  // Export through the live engine, then open as a real PPTX through the host.
  const { data } = await run('file.saveBytes', { format: 'pptx' })
  await page.evaluate(async (data) => {
    const bytes = Uint8Array.from(atob(data), (c) => c.charCodeAt(0))
    await window.devHost.open('Editable-table.pptx', bytes.buffer)
  }, data)
  await until(async () => (await run('document.inspect')).slides.length === 3, 'PPTX reopened')
  await run('slide.go', { index: tableSlide })
  // The person clicks into the cell: an agent's edit.select and text.edit move only the agent's
  // own editor state, for that run (KHP C3b).
  await clickCell(0, 1)
  assert.equal((await state()).canSplit, true)
  assert.equal((await state()).anchor, 'center')
  assert.match(await cellText(0, 0), /Editable/)
  await tab('tableLayout')
  await kit.getByTestId('slides-cmd-table.split').click()
  await until(async () => !(await state()).canSplit, 'split remains available after PPTX reopen')
  await clickCell(1, 1)
  await page.keyboard.insertText('Reopened')
  await until(async () => (await cellText(1, 1)).includes('Reopened'), 'reopened cells remain editable')
  // Grid lines drag with feedback; one drag is one undo step.
  const before = (await tables())[0]
  const surface = await kit.getByTestId('kit-canvas-surface').boundingBox()
  const scale = Number((await kit.getByTestId('kit-zoom').textContent()).replace('%', '')) / 100
  const toScreen = (x, y) => ({ x: surface.x + surface.width / 2 + (x - 480) * scale, y: surface.y + surface.height / 2 + (y - 270) * scale })
  const line = toScreen(before.box[0] + before.table.widths[0], before.box[1] + before.box[3] - 6)
  await page.mouse.move(line.x, line.y)
  await until(async () => (await kit.getByTestId('kit-canvas-surface').evaluate((element) => element.style.cursor)) === 'ew-resize', 'column line shows a resize cursor')
  await page.mouse.down()
  await page.mouse.move(line.x + 20, line.y, { steps: 3 })
  await page.mouse.move(line.x + 40, line.y, { steps: 3 })
  await page.mouse.up()
  await until(async () => Math.abs((await tables())[0].table.widths[0] - before.table.widths[0] - 40 / scale) < 1.5, 'dragging a column line resizes the columns')
  const after = (await tables())[0]
  assert.ok(Math.abs(after.table.widths.reduce((a, b) => a + b) - before.table.widths.reduce((a, b) => a + b)) < 0.01, 'the table keeps its width')
  await kit.getByTestId('slides-undo').click()
  await until(async () => (await tables())[0].table.widths[0] === before.table.widths[0], 'one undo restores the columns')
  console.log('PASS dragging grid lines with resize feedback and a single undo step')

  // Arrow keys leave a cell at its edge; Delete on a cell range clears text, not the table.
  await clickCell(2, 0)
  await page.keyboard.insertText('Left')
  await page.keyboard.press('ArrowRight')
  await page.keyboard.insertText('Next')
  await until(async () => (await cellText(2, 1)) === 'Next', 'ArrowRight at the end of a cell enters the next cell')
  await page.keyboard.press('Shift+ArrowLeft')
  await page.keyboard.press('Shift+ArrowLeft')
  await page.keyboard.press('Shift+ArrowLeft')
  await page.keyboard.press('Shift+ArrowLeft')
  await page.keyboard.press('Shift+ArrowLeft')
  await until(async () => (await run('document.inspect')).selection.cells != null, 'Shift+Arrow past the cell edge selects cells')
  await page.keyboard.press('Delete')
  await until(async () => (await cellText(2, 0)) === '' && (await cellText(2, 1)) === '', 'Delete clears the selected cells')
  assert.equal((await tables()).length, 1, 'the table itself stays')
  console.log('PASS arrow navigation across cells and clearing a cell range')

  // Borders on chosen edges with the pen's color and weight.
  await clickCell(1, 1)
  await tab('tableDesign')
  await kit.getByTestId('slides-table-pen-weight').selectOption('3')
  await kit.getByTestId('slides-table-pen-color').click()
  await kit.getByTestId('slides-table-pen-colors').getByRole('button', { name: '#FF0000', exact: true }).click()
  await kit.getByTestId('slides-table-borders').click()
  await kit.getByRole('menuitem', { name: 'Top Border', exact: true }).click()
  await until(async () => JSON.stringify((await state()).borders) === JSON.stringify([null, null, 3, null]), 'a top border with the pen weight')
  await kit.getByTestId('slides-table-borders').click()
  await kit.getByRole('menuitem', { name: 'No Border', exact: true }).click()
  await until(async () => JSON.stringify((await state()).borders) === JSON.stringify([0, 0, 0, 0]), 'No Border removes every line of the range')
  await kit.getByTestId('slides-undo').click()
  await until(async () => (await state()).borders[2] === 3, 'border edits undo')
  console.log('PASS per-edge borders with pen color and weight')

  await kit.getByTestId('slides-save').click()
  await until(async () => (await page.evaluate(() => window.devHost.files.get('Editable-table.pptx')?.version)) > 1, 'save via host')
  await mkdir(new URL('../test-results/', import.meta.url), { recursive: true })
  await page.screenshot({ path: new URL(`../test-results/table-${which}.png`, import.meta.url).pathname })
  const saved = await page.evaluate(() => Array.from(new Uint8Array(window.devHost.files.get('Editable-table.pptx').bytes)))
  await page.goto(`http://127.0.0.1:${server.httpServer.address().port}/dev.html?modes=classic&locale=en-US&role=viewer`)
  await page.waitForFunction(() => document.getElementById('status')?.textContent?.includes(' open in '), null, { timeout: 60000 })
  await page.evaluate(async (saved) => window.devHost.open('Read-only-table.pptx', new Uint8Array(saved).buffer), saved)
  // One agent run: its selection lasts that run (KHP C3b).
  const id = (await tables())[0].id
  const reopened = await page.evaluate(async (steps) => (await window.devHost.connection().ai.run(steps, false)).completed.at(-1).result, [
    { command: 'slide.go', params: { index: tableSlide } },
    { command: 'edit.select', params: { ids: [id] } },
    { command: 'table.selectCells', params: { from: [1, 1], to: [1, 1] } },
    { command: 'table.state', params: {} },
  ])
  assert.equal(reopened.borders[2], 3, 'the border survives saving and reopening')
  // A read-only document shows only the viewing tabs: no Table Layout, so no table edits.
  await page.waitForTimeout(300)
  assert.equal(await kit.getByTestId('ribbon-tab-tableLayout').count(), 0, 'no Table Layout tab in a read-only document')
  await assert.rejects(() => run('table.insertRowBelow'), /read-only/)
  assert.deepEqual(errors, [])
  console.log('PASS table insertion, pointer typing, navigation, row/column edits, undo/redo, merge/split, formatting and editable PPTX persistence')
} catch (error) {
  await mkdir(new URL('../test-results/', import.meta.url), { recursive: true })
  await page.screenshot({ path: new URL(`../test-results/table-${which}-failure.png`, import.meta.url).pathname })
  throw error
} finally {
  await browser.close()
  await server.close()
}
