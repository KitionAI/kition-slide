// Ink in a real browser, through the development host: node tests/ink.mjs [chromium|webkit]
// Picks a pen from the shared Draw controls, draws a stroke with the pointer, undoes and redoes
// it, erases it with the eraser's drag (one step), saves to .pptx, reopens, and checks that a
// viewer cannot draw.
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
const strokes = async () => (await run('drawing.describe', {})).objects.filter((o) => o.kind === 'ink').flatMap((o) => o.ink)
const open = async (query = '') => {
  await page.goto(`${origin}/dev.html?modes=classic&locale=en-US${query}`)
  await page.waitForFunction(() => document.getElementById('status')?.textContent?.includes(' open in '), null, { timeout: 60000 })
}
// The page point of slide point (x, y): the slide (960 × 540) is centred in the canvas at its zoom.
const at = async (x, y) => {
  const bounds = await kit.getByTestId('kit-canvas-surface').boundingBox()
  const zoom = Number((await kit.getByTestId('kit-zoom').textContent()).replace('%', '')) / 100
  return { x: bounds.x + bounds.width / 2 + (x - 480) * zoom, y: bounds.y + bounds.height / 2 + (y - 270) * zoom }
}
const drag = async (from, to) => {
  const a = await at(...from)
  const b = await at(...to)
  await page.mouse.move(a.x, a.y)
  await page.mouse.down()
  for (let i = 1; i <= 6; i += 1) await page.mouse.move(a.x + ((b.x - a.x) * i) / 6, a.y + ((b.y - a.y) * i) / 6)
  await page.mouse.up()
}

try {
  await open()
  await run('slide.new', { layout: 'blank' })
  const slide = (await run('document.inspect')).selection.slide
  await kit.getByTestId('ribbon-tab-draw').click()
  await kit.getByTestId('slides-ink-pen').click()
  await until(async () => (await run('draw.state')).tool === 'pen', 'the pen is in hand')
  await kit.getByTestId('slides-ink-weight').click()
  await kit.getByTestId('slides-ink-weight-3').click()
  await until(async () => (await run('draw.state')).width === 3, 'the pen is 3 pt')
  await drag([300, 200], [500, 240])
  await until(async () => (await strokes()).length === 1, 'one stroke on release')
  const [stroke] = await strokes()
  assert.equal(stroke.width, 3)
  assert.ok(Math.abs(stroke.points[0][0] - 300) < 4 && Math.abs(stroke.points.at(-1)[0] - 500) < 4, JSON.stringify(stroke.points))
  await kit.getByTestId('slides-undo').click()
  await until(async () => (await strokes()).length === 0, 'undo takes the stroke back')
  await kit.getByTestId('slides-redo').click()
  await until(async () => (await strokes()).length === 1, 'redo puts it back')
  console.log('PASS draw a stroke with the shared pen, with undo')

  await kit.getByTestId('slides-ink-highlighter').click()
  await until(async () => (await run('draw.state')).tool === 'highlighter', 'the highlighter is in hand')
  await drag([300, 350], [500, 350])
  await until(async () => (await strokes()).some((s) => s.tool === 'highlighter'), 'a highlighter stroke')
  await kit.getByTestId('slides-ink-eraser').click()
  await until(async () => (await run('draw.state')).tool === 'eraser', 'the eraser is in hand')
  await drag([400, 170], [400, 260])
  await until(async () => (await strokes()).length === 1 && (await strokes())[0].tool === 'highlighter', 'the eraser takes the pen stroke it crosses')
  // The whole drag was one step: one undo brings the stroke back.
  await kit.getByTestId('slides-undo').click()
  await until(async () => (await strokes()).length === 2, 'one undo for the eraser drag')
  await kit.getByTestId('slides-redo').click()
  console.log('PASS the highlighter, and the eraser drag in one step')

  await kit.getByTestId('slides-ink-select').click()
  const { data } = await run('file.saveBytes', { format: 'pptx' })
  const reopen = (name) => page.evaluate(async ({ data, name }) => {
    const bytes = Uint8Array.from(atob(data), (c) => c.charCodeAt(0))
    await window.devHost.open(name, bytes.buffer)
  }, { data, name })
  await reopen('Ink.pptx')
  await until(async () => (await run('document.inspect')).slides.length > slide, 'the presentation reopens')
  await run('slide.go', { index: slide })
  await until(async () => (await strokes()).length === 1 && (await strokes())[0].tool === 'highlighter', 'the stroke reopens as ink')
  console.log('PASS save as .pptx and reopen')

  await open('&role=viewer')
  await reopen('Ink.pptx')
  await run('slide.go', { index: slide })
  await until(async () => (await strokes()).length === 1, 'the viewer sees the ink')
  await assert.rejects(() => run('drawing.ink', { strokes: [{ points: [[10, 10], [20, 20]] }] }), /read-only|not allowed/i)
  assert.deepEqual(errors, [])
  console.log('PASS a viewer cannot draw')
} catch (error) {
  // Outside the repository, which commits no screenshots.
  const shot = join(tmpdir(), `kition-slide-ink-${which}-failure.png`)
  await page.screenshot({ path: shot })
  console.log(`screenshot: ${shot}`)
  throw error
} finally {
  await browser.close()
  await server.close()
}
