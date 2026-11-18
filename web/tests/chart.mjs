// Charts in a real browser, through the development host: node tests/chart.mjs [chromium|webkit]
// Inserts a chart from the shared Insert › Chart menu, changes its type and elements from the
// shared Chart Design controls, edits its data in the grid, undoes, saves to .pptx, reopens, edits
// it again, and checks that a viewer cannot change it.
import assert from 'node:assert/strict'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { chromium, webkit } from 'playwright'
import { createServer } from 'vite'

const which = process.argv[2] ?? 'chromium'
const server = await createServer({ configFile: new URL('../vite.config.ts', import.meta.url).pathname, server: { port: 0, host: '127.0.0.1' }, logLevel: 'error' })
await server.listen()
const origin = `http://127.0.0.1:${server.httpServer.address().port}`
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
const charts = async () => (await run('drawing.describe', {})).objects.filter((o) => o.kind === 'chart')
const chart = async () => (await charts())[0]?.chart
const open = async (query = '') => {
  await page.goto(`${origin}/dev.html?modes=classic&locale=en-US${query}`)
  await page.waitForFunction(() => document.getElementById('status')?.textContent?.includes(' open in '), null, { timeout: 60000 })
}

try {
  await open()
  await run('slide.new', { layout: 'blank' })
  const slide = (await run('document.inspect')).selection.slide
  await tab('insert')
  await kit.getByTestId('slides-chart-insert').click()
  await kit.getByTestId('slides-chart-insert-line').click()
  await until(async () => (await chart())?.kind === 'line', 'a line chart is inserted and selected')
  console.log('PASS insert a chart from the shared menu')

  await until(() => kit.getByTestId('ribbon-tab-chartDesign').isVisible(), 'Chart Design shows for the chart')
  await tab('chartDesign')
  await kit.getByTestId('slides-chart-type').click()
  await kit.getByTestId('slides-chart-type-bar').click()
  await until(async () => (await chart()).kind === 'bar', 'Change Chart Type')
  await kit.getByTestId('slides-chart-legend').click()
  await kit.getByTestId('slides-chart-legend-top').click()
  await until(async () => (await chart()).legend === 'top', 'the legend moves')
  await kit.getByTestId('slides-chart-data-labels').click()
  await until(async () => (await chart()).dataLabels === true, 'data labels')
  console.log('PASS type and elements from Chart Design')
  // Chart Elements: a trendline and error bars on every series.
  await kit.getByTestId('slides-chart-trendline').click()
  await kit.getByTestId('slides-chart-trendline-linear').click()
  await until(async () => (await chart()).series.every((s) => s.trendline?.kind === 'linear'), 'a linear trendline')
  await kit.getByTestId('slides-chart-error-bars').click()
  await kit.getByTestId('slides-chart-error-bars-stdErr').click()
  await until(async () => (await chart()).series.every((s) => s.errorBars?.kind === 'stdErr'), 'standard error bars')
  console.log('PASS a trendline and error bars from Chart Elements')

  // Format: the value axis's scale, where labels sit, and a series' outline.
  await kit.getByTestId('slides-chart-format-axis').click()
  await kit.getByTestId('slides-chart-axis-min').fill('0')
  await kit.getByTestId('slides-chart-axis-max').fill('20')
  await kit.getByTestId('slides-chart-axis-numberFormat').fill('0.0')
  await kit.getByTestId('slides-chart-axis-ok').click()
  await until(async () => (await chart()).yAxis?.max === 20 && (await chart()).yAxis?.numberFormat === '0.0', 'Format Axis sets the scale')
  await kit.getByTestId('slides-chart-label-position').click()
  await kit.getByTestId('slides-chart-label-position-outEnd').click()
  await until(async () => (await chart()).dataLabelPosition === 'outEnd', 'labels outside the bars')
  await kit.getByTestId('slides-chart-format-series').click()
  assert.equal(await kit.getByTestId('slides-chart-series-marker').count(), 0, 'bars take no markers')
  await kit.getByTestId('slides-chart-series-width').fill('2')
  await kit.getByTestId('slides-chart-series-ok').click()
  await until(async () => (await chart()).series[0].lineWidth === 2, 'Format Series sets the outline')
  await kit.getByTestId('slides-undo').click()
  await until(async () => (await chart()).series[0].lineWidth == null, 'undo the outline')
  await kit.getByTestId('slides-redo').click()
  await until(async () => (await chart()).series[0].lineWidth === 2, 'redo the outline')
  console.log('PASS axis, label position and series look from Format, with undo')

  await kit.getByTestId('slides-chart-edit-data').click()
  await until(() => kit.getByTestId('slides-chart-data').isVisible(), 'the data grid opens')
  await kit.getByTestId('slides-chart-data-category-0').fill('North')
  await kit.getByTestId('slides-chart-data-cell-0-0').fill('12.5')
  await kit.getByTestId('slides-chart-data-series-1').fill('Plan')
  await kit.getByTestId('slides-chart-data-cell-1-1').fill('oops')
  assert.equal(await kit.getByTestId('slides-chart-data-ok').isDisabled(), true, 'a value that is not a number is refused')
  await kit.getByTestId('slides-chart-data-cell-1-1').fill('')
  await kit.getByTestId('slides-chart-data-ok').click()
  await until(async () => {
    const c = await chart()
    return c.categories[0] === 'North' && c.series[0].values[0] === 12.5 && c.series[1].name === 'Plan' && c.series[1].values[1] === null
  }, 'the data change')
  // One step: the person's Undo takes the whole data edit back.
  await kit.getByTestId('slides-undo').click()
  await until(async () => (await chart()).categories[0] !== 'North', 'undo takes the data back')
  await kit.getByTestId('slides-redo').click()
  await until(async () => (await chart()).categories[0] === 'North', 'redo puts it back')
  console.log('PASS edit data in the grid, with undo')

  // The agent's path is the same command.
  const id = (await charts())[0].id
  await run('drawing.chart', { id, title: 'Revenue 收入' })
  await until(async () => (await chart()).title === 'Revenue 收入', 'an agent sets the title')
  const before = await chart()

  // Save as .pptx, reopen, and edit again.
  const { data } = await run('file.saveBytes', { format: 'pptx' })
  await page.evaluate(async (data) => {
    const bytes = Uint8Array.from(atob(data), (c) => c.charCodeAt(0))
    await window.devHost.open('Chart.pptx', bytes.buffer)
  }, data)
  await until(async () => (await run('document.inspect')).slides.length >= 2, 'the presentation reopens')
  await run('slide.go', { index: slide })
  await until(async () => (await charts()).length === 1, 'the chart reopens')
  const reopened = await chart()
  assert.deepEqual({ ...reopened, series: reopened.series.map(({ name, values }) => ({ name, values })) }, { ...before, series: before.series.map(({ name, values }) => ({ name, values })) }, 'the chart reads back as it was saved')
  await run('drawing.chart', { id: (await charts())[0].id, kind: 'column', gridlines: false })
  await until(async () => (await chart()).kind === 'column' && (await chart()).gridlines === false, 'the reopened chart changes again')
  console.log('PASS save as .pptx, reopen and edit again')

  // A viewer cannot change it.
  const saved = await run('file.saveBytes', { format: 'pptx' })
  await open('&role=viewer')
  await page.evaluate(async (data) => {
    const bytes = Uint8Array.from(atob(data), (c) => c.charCodeAt(0))
    await window.devHost.open('Chart-view.pptx', bytes.buffer)
  }, saved.data)
  await run('slide.go', { index: slide })
  await until(async () => (await charts()).length === 1, 'the viewer sees the chart')
  const viewId = (await charts())[0].id
  await assert.rejects(() => run('drawing.chart', { id: viewId, title: 'No' }), /read-only|not allowed/i)
  assert.deepEqual(errors, [])
  console.log('PASS a viewer cannot edit the chart')
} catch (error) {
  // Outside the repository, which commits no screenshots.
  const shot = join(tmpdir(), `kition-slide-chart-${which}-failure.png`)
  await page.screenshot({ path: shot })
  console.log(`screenshot: ${shot}`)
  throw error
} finally {
  await browser.close()
  await server.close()
}
