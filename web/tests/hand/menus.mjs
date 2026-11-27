// The ribbon's menus in the default Single line mode (plan 0A; walkthrough F2, H3, A7): each of the
// eight menus a row button opens shows its items, and choosing one does what it says.
// node tests/hand/menus.mjs [chromium|webkit] [segment…]
import assert from 'node:assert/strict'
import { open, segments } from './harness.mjs'

/** Opens the menu of the row button `testId` on tab `tab`; answers its rows' sizes. */
async function openMenu(e, tab, testId) {
  const { kit, wait } = e
  await kit.getByTestId(`ribbon-tab-${tab}`).click()
  await wait(250)
  assert.equal(await kit.locator('.kui-app').getAttribute('data-mode'), 'line', 'the editor is in Single line')
  const trigger = kit.locator('.kui-line-inline').getByTestId(testId)
  assert.equal(await trigger.count(), 1, `${testId} is in the row itself`)
  await trigger.click()
  const menu = kit.locator('.kui-line-inline .kui-popover')
  await e.until(() => menu.isVisible())
  const box = await menu.boundingBox()
  assert(box && box.height > 20, `${testId}: the menu has a height (${box?.height})`)
  const rows = await menu.locator('button').evaluateAll((items) => items.map((item) => item.getBoundingClientRect().height))
  assert(rows.length > 0 && rows.every((height) => height > 0), `${testId}: every row shows (${rows.length} rows, heights ${[...new Set(rows)].join('/')})`)
  return menu
}

async function shapeCount(e) {
  return ((await e.ask('slide.inspect', { index: (await e.state()).slide, select: false }))?.shapes ?? []).length
}

const list = [
  ['f2-single-line-menus', async (e) => {
    const { kit, wait, until, agent } = e
    // A shape selected, so Font Color has something to colour.
    await agent([{ command: 'slide.new', params: { layout: 'Blank', at: 2 } }, { command: 'shape.insert', params: { preset: 'rect', rect: [100, 100, 200, 120], text: 'Box' } }])
    await e.goto(2)
    await e.click(200, 160)
    assert(await until(async () => (await e.state()).selected === 1), 'the box is selected')

    // Home › Font Color: colours the selected shape's text.
    let menu = await openMenu(e, 'home', 'slides-font-color')
    await menu.getByRole('button', { name: '#FF0000', exact: true }).click()
    await wait(300)
    assert.equal(await kit.locator('.kui-line-inline .kui-popover').count(), 0, 'Font Color closes on a choice')

    // Home › Shapes: picks the oval tool.
    menu = await openMenu(e, 'home', 'slides-shapes')
    await menu.getByTestId('slides-shape-ellipse').click()
    await wait(200)
    assert.equal(await kit.locator('.kui-line-inline .kui-popover').count(), 0, 'Shapes closes on a choice')
    await e.page.keyboard.press('Escape')
    await wait(200)

    // Insert › Table, Shapes, Chart.
    const before = await shapeCount(e)
    menu = await openMenu(e, 'insert', 'slides-table')
    await menu.locator('button').first().click()
    await until(async () => (await shapeCount(e)) === before + 1)
    assert.equal(await shapeCount(e), before + 1, 'Insert › Table inserts a table')
    await openMenu(e, 'insert', 'slides-shapes')
    await e.page.keyboard.press('Escape')
    menu = await openMenu(e, 'insert', 'slides-chart-insert')
    await menu.locator('button').first().click()
    await until(async () => (await shapeCount(e)) === before + 2, 80)
    assert.equal(await shapeCount(e), before + 2, 'Insert › Chart inserts a chart')

    // Chart Design › Change Chart Type, with the chart just inserted selected.
    await until(() => kit.getByTestId('ribbon-tab-chartDesign').isVisible())
    menu = await openMenu(e, 'chartDesign', 'slides-chart-type')
    await e.page.keyboard.press('Escape')
    await wait(200)

    // View › Zoom: 200%.
    menu = await openMenu(e, 'view', 'slides-view-zoom')
    await menu.locator('button', { hasText: /^200%$/ }).click()
    await until(async () => (await e.zoom()) === 200)
    assert.equal(await e.zoom(), 200, 'View › Zoom › 200%')

    // Design › Colors.
    await openMenu(e, 'design', 'slides-theme-colors')
    await e.page.keyboard.press('Escape')
  }],
]

await segments('menus', list, (which) => open({ which }))
