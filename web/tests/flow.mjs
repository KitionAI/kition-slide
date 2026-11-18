// The editor's flow in a real browser, through the development host: node tests/flow.mjs [chromium|webkit]
// Starts the Vite dev server itself, opens dev.html in Chinese, and prints PASS or FAIL per step.
import { chromium, webkit } from 'playwright'
import { createServer } from 'vite'

const which = process.argv[2] ?? 'chromium'
const server = await createServer({ configFile: new URL('../vite.config.ts', import.meta.url).pathname, server: { port: 0, host: '127.0.0.1' }, logLevel: 'error' })
await server.listen()
const origin = `http://127.0.0.1:${server.httpServer.address().port}`
const browser = await (which === 'webkit' ? webkit : chromium).launch()
const page = await browser.newPage({ viewport: { width: 1440, height: 900 } })
const errors = []
let failures = 0
page.on('pageerror', (error) => errors.push(error.message))
const step = (name, ok, detail = '') => {
  if (!ok) failures += 1
  console.log(`${ok ? 'PASS' : 'FAIL'} ${name}${detail ? ` - ${detail}` : ''}`)
}
const until = async (check, tries = 60) => {
  for (let i = 0; i < tries; i += 1) {
    if (await check().catch(() => false)) return true
    await page.waitForTimeout(100)
  }
  return false
}

try {
  await page.goto(`${origin}/dev.html?modes=classic&locale=zh-CN`)
  await page.waitForFunction(() => document.getElementById('status')?.textContent?.includes(' open in '), null, { timeout: 60000 })
  const kit = page.frameLocator('#kit')
  await page.waitForTimeout(1500)
  const zoom = Number((await kit.getByTestId('kit-zoom').textContent())?.replace('%', ''))
  step('fits the slide in view', zoom > 40, `${zoom}%`)
  step('Chinese strings', (await kit.getByTestId('ribbon-tab-home').textContent()) === '开始')

  await kit.getByTestId('slides-slide-1').click()
  await until(async () => (await kit.getByTestId('slides-status').textContent())?.includes('2'))
  step('goes to slide 2 from its thumbnail', (await kit.getByTestId('slides-status').textContent())?.includes('2'), await kit.getByTestId('slides-status').textContent())

  // Slide 2 is fitted with its 40 pt margin; the first box is at slide (60..250, 220..340).
  const canvas = await kit.getByTestId('kit-canvas-surface').boundingBox()
  const zoomValue = Number((await kit.getByTestId('kit-zoom').textContent())?.replace('%', '')) / 100
  const slideToScreen = (x, y) => ({
    x: canvas.x + canvas.width / 2 - (1040 / 2) * zoomValue + (40 + x) * zoomValue,
    y: canvas.y + canvas.height / 2 - (620 / 2) * zoomValue + (40 + y) * zoomValue,
  })
  const box = slideToScreen(155, 280)
  await page.mouse.click(box.x, box.y)
  await until(async () => (await kit.locator('.kui-status-bar').textContent())?.includes('已选'))
  step('selects a shape with the engine tool', (await kit.locator('.kui-status-bar').textContent())?.includes('已选'))

  if (which === 'chromium') {
    const cdp = await page.context().newCDPSession(page)
    await cdp.send('Input.imeSetComposition', { text: 'bei', selectionStart: 3, selectionEnd: 3 })
    await cdp.send('Input.insertText', { text: '北极光' })
  } else {
    await page.keyboard.insertText('北极光')
  }
  const shapesText = async () =>
    page.evaluate(async () => {
      const result = await window.devHost.connection().ai.run([{ command: 'slide.inspect', params: { index: 1 } }], false)
      return JSON.stringify(result.completed[0].result.shapes.map((shape) => shape.text ?? ''))
    })
  await until(async () => (await shapesText()).includes('北极光'))
  step('types Chinese into the shape through the input method', (await shapesText()).includes('北极光'), await shapesText())

  await page.keyboard.press('Escape')
  await page.keyboard.press('Escape')
  await page.waitForTimeout(300)
  await page.mouse.click(box.x, box.y)
  await page.waitForTimeout(300)
  await kit.getByTestId('slides-cmd-format.bold').click()
  await until(async () => (await kit.getByTestId('slides-cmd-format.bold').getAttribute('aria-pressed')) === 'true')
  step('bolds the selection from the ribbon', (await kit.getByTestId('slides-cmd-format.bold').getAttribute('aria-pressed')) === 'true')

  await kit.getByTestId('slides-notes-text').click()
  await kit.getByTestId('slides-notes-text').fill('说明：极光的成因')
  await kit.getByTestId('slides-notes-text').blur()
  const notes = () => page.evaluate(async () => (await window.devHost.connection().ai.run([{ command: 'slide.inspect', params: { index: 1 } }], false)).completed[0].result.notes)
  await until(async () => (await notes()) === '说明：极光的成因')
  step('writes speaker notes', (await notes()) === '说明：极光的成因', await notes())

  const before = await page.evaluate(() => window.devHost.changes.length)
  const run = await page.evaluate(async () => window.devHost.connection().ai.run([{ command: 'shape.insert', params: { preset: 'ellipse', rect: [400, 60, 160, 100], text: 'Agent' } }], false))
  await page.waitForTimeout(600)
  step('the agent edits the live slides', run.completed[0].changed && (await page.evaluate(() => window.devHost.changes.at(-1).by)) === 'agent')
  step('the host hears of changes', (await page.evaluate(() => window.devHost.changes.length)) > before)

  await kit.getByTestId('slides-save').click()
  await until(async () => (await page.evaluate(() => window.devHost.files.get('Sample.kslide').version)) > 1)
  step('saves through the host', (await page.evaluate(() => window.devHost.files.get('Sample.kslide').version)) > 1)

  // File › Export: the engine draws the file, the host writes it next to the slides as a new file.
  const exported = (name) => page.evaluate((file) => window.devHost.files.get(file)?.bytes?.byteLength ?? 0, name)
  const head = (name, count) => page.evaluate(([file, n]) => [...new Uint8Array(window.devHost.files.get(file).bytes).slice(0, n)], [name, count])
  const toast = () => kit.locator('.kui-toast').last().textContent().catch(() => '')
  await kit.getByTestId('ribbon-tab-file').click()
  await kit.getByTestId('slides-export').click()
  await kit.getByTestId('slides-export-format').selectOption('pdf')
  await kit.getByTestId('slides-export-run').click()
  await until(async () => (await exported('Sample.pdf')) > 0, 200)
  step('exports the slides as PDF through the host', String.fromCharCode(...(await head('Sample.pdf', 5))) === '%PDF-')
  step('says it exported once the host wrote the file', (await toast()).includes('Sample.pdf'), await toast())
  const slides = await page.evaluate(async () => (await window.devHost.connection().ai.run([{ command: 'document.inspect', params: {} }], false)).completed[0].result.slides.length)
  await kit.getByTestId('slides-export').click()
  await kit.getByTestId('slides-export-format').selectOption('png')
  await kit.getByTestId('slides-export-scope').selectOption('all')
  await kit.getByTestId('slides-export-scale').selectOption('1')
  await kit.getByTestId('slides-export-run').click()
  await until(async () => (await exported(`Sample ${slides}.png`)) > 0, 200)
  step('exports every slide as a PNG', JSON.stringify(await head('Sample 1.png', 4)) === '[137,80,78,71]' && (await exported(`Sample ${slides}.png`)) > 0, `${slides} slides`)
  // The host refuses the write: its error is shown, and no success.
  await page.evaluate(() => {
    const files = window.devHost.files
    const set = files.set.bind(files)
    files.set = (name, file) => {
      if (name.endsWith('.jpg')) throw new Error('the workspace is read-only')
      return set(name, file)
    }
  })
  await kit.getByTestId('slides-export').click()
  await kit.getByTestId('slides-export-format').selectOption('jpg')
  await kit.getByTestId('slides-export-scope').selectOption('current')
  await kit.getByTestId('slides-export-run').click()
  await until(async () => (await toast()).includes('read-only'), 100)
  step('shows the host\'s error when it does not write the file', (await toast()).includes('read-only') && !(await toast()).includes('已导出'), await toast())
  step('the dialog stays open after a refusal', await kit.getByTestId('slides-export-dialog').isVisible())
  await kit.getByTestId('slides-export-dialog').getByRole('button', { name: '取消' }).click()

  await kit.getByTestId('ribbon-tab-slideShow').click()
  await kit.getByTestId('slides-show-start').click()
  await until(async () => kit.getByTestId('slides-show').isVisible())
  step('starts the slide show', await kit.getByTestId('slides-show').isVisible())
  await page.keyboard.press('Escape')
  await page.waitForTimeout(500)
  step('ends the slide show', !(await kit.getByTestId('slides-show').isVisible().catch(() => false)))
} catch (error) {
  failures += 1
  console.log(`FAIL ${error instanceof Error ? error.message.split('\n')[0] : String(error)}`)
} finally {
  if (errors.length) console.log(`page errors:\n${errors.slice(0, 8).join('\n')}`)
  await browser.close()
  await server.close()
}
process.exit(failures ? 1 : 0)
