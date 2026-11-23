// Real wheel input against the Rust engine in the development host.
import assert from 'node:assert/strict'
import { mkdir } from 'node:fs/promises'
import { chromium, webkit } from 'playwright'
import { createServer } from 'vite'

const which = process.argv[2] ?? 'chromium'
const server = await createServer({ configFile: new URL('../vite.config.ts', import.meta.url).pathname, server: { port: 0, host: '127.0.0.1' }, logLevel: 'error' })
await server.listen()
const browser = await (which === 'webkit' ? webkit : chromium).launch()
const page = await browser.newPage({ viewport: { width: 1440, height: 900 } })
const errors = []
page.on('pageerror', (error) => errors.push(error.message))
const kit = page.frameLocator('#kit')
const surface = kit.getByTestId('kit-canvas-surface')
const current = async () => Number(await kit.locator('.slides-slide[data-current="true"]').getAttribute('data-index'))
const until = async (check) => {
  for (let attempt = 0; attempt < 60; attempt += 1) {
    if (await check()) return
    await page.waitForTimeout(100)
  }
  assert.fail('Timed out waiting for the editor')
}
const wheel = async (dy, dx = 0) => {
  const bounds = await surface.boundingBox()
  await page.mouse.move(bounds.x + bounds.width / 2, bounds.y + bounds.height / 2)
  await page.mouse.wheel(dx, dy)
  await page.waitForTimeout(450)
}
const pixels = async () => {
  await page.waitForTimeout(350)
  return surface.screenshot()
}
// The canvas's scrollbars: one along an axis only while the slide and its margin are longer than the view there.
const scrollbars = kit.getByTestId('slides-scrollbars').locator('[role="scrollbar"]')
const zoom = async () => Number((await kit.getByTestId('kit-zoom').textContent()).replace('%', ''))

try {
  await page.goto(`http://127.0.0.1:${server.httpServer.address().port}/dev.html?modes=classic&locale=en-US`)
  await page.waitForFunction(() => document.getElementById('status')?.textContent?.includes(' open in '), null, { timeout: 60000 })
  await until(async () => (await current()) === 0)
  const first = await pixels()
  await wheel(100)
  await until(async () => (await current()) === 1)
  const last = await pixels()
  await wheel(100)
  assert.equal(await current(), 1)
  assert.ok(last.equals(await pixels()), 'The last slide must stay in place at the end of the slides')
  await wheel(-100)
  await until(async () => (await current()) === 0)
  assert.ok(first.equals(await pixels()), 'Paging back must restore the same fitted slide geometry')
  await wheel(-100)
  await wheel(5)
  await wheel(0, 100)
  assert.equal(await current(), 0)
  assert.ok(first.equals(await pixels()), 'Boundary, tiny, and horizontal wheels must not move a fitted slide')
  assert.equal(await scrollbars.count(), 0, 'A fitted slide has no scrollbar')
  console.log('PASS fitted wheels page both ways without moving the canvas, including the first and last slides')

  // Pointer gestures cannot turn into a slide change midway through a drag.
  const bounds = await surface.boundingBox()
  await page.mouse.move(bounds.x + 10, bounds.y + 10)
  await page.mouse.down()
  await page.mouse.wheel(0, 100)
  await page.waitForTimeout(450)
  assert.equal(await current(), 0)
  await page.mouse.up()
  console.log('PASS a wheel during a pointer gesture does not page')

  await page.evaluate(async () => {
    await window.devHost.connection().ai.run([
      ...Array.from({ length: 12 }, () => ({ command: 'slide.new', params: {} })),
      { command: 'slide.go', params: { index: 0 } },
    ], false)
  })
  await until(async () => (await kit.locator('.slides-slide').count()) === 14 && (await current()) === 0)
  await wheel(100)
  await until(async () => (await current()) === 1)
  // A burst is throttled, matching the native canvas's 350 ms page gate.
  await surface.dispatchEvent('wheel', { deltaY: 100 })
  await surface.dispatchEvent('wheel', { deltaY: 100 })
  await surface.dispatchEvent('wheel', { deltaY: 100 })
  await until(async () => (await current()) === 2)
  await page.waitForTimeout(200)
  assert.equal(await current(), 2)
  await page.waitForTimeout(200)
  for (let index = 3; index <= 10; index += 1) {
    await wheel(100)
    await until(async () => (await current()) === index)
  }
  const selected = await kit.locator('.slides-slide[data-current="true"]').boundingBox()
  const list = await kit.locator('.slides-slides-list').boundingBox()
  assert.ok(selected.y >= list.y && selected.y + selected.height <= list.y + list.height + 1, 'The current thumbnail must follow paging into view')
  const canvasBeforeListScroll = await pixels()
  await page.mouse.move(list.x + list.width / 2, list.y + list.height / 2)
  await page.mouse.wheel(0, -1000)
  await page.waitForTimeout(450)
  assert.equal(await current(), 10)
  assert.ok(canvasBeforeListScroll.equals(await pixels()), 'Scrolling thumbnails must leave the slide canvas fixed')
  console.log('PASS page bursts are throttled, thumbnails follow selection, and the sidebar scrolls independently')

  await kit.getByTestId('slides-slide-0').click()
  await until(async () => (await current()) === 0)
  const beforeZoom = await zoom()
  await page.keyboard.down('Control')
  await wheel(-100)
  await page.keyboard.up('Control')
  assert.ok((await zoom()) > beforeZoom, 'Ctrl-wheel must keep zooming')
  assert.equal(await current(), 0)
  const beforePan = await pixels()
  await wheel(100)
  assert.equal(await current(), 0)
  assert.ok(!beforePan.equals(await pixels()), 'An enlarged slide must pan instead of page')
  await wheel(10000, 10000)
  const panEnd = await pixels()
  await wheel(10000, 10000)
  assert.ok(panEnd.equals(await pixels()), 'Panning must clamp to the slide edges')
  // The scrollbars lie over the canvas, so they are used only after the pixel comparison above.
  const shown = await scrollbars.count()
  assert.ok(shown >= 1, 'An enlarged slide has a scrollbar along each axis it is longer than the view on')
  for (let index = 0; index < shown; index += 1) {
    const bar = scrollbars.nth(index)
    const at = Number(await bar.getAttribute('aria-valuenow')), end = Number(await bar.getAttribute('aria-valuemax'))
    assert.ok(end > 0 && Math.abs(at - end) <= 1, `A scrollbar is at its end when the slide is panned to its far edge (${at} of ${end})`)
    await bar.focus()
    await page.keyboard.press('Home')
    await until(async () => Number(await bar.getAttribute('aria-valuenow')) === 0)
  }
  await kit.getByRole('button', { name: 'Fit', exact: true }).click()
  await until(async () => (await scrollbars.count()) === 0)
  await wheel(100)
  await until(async () => (await current()) === 1)
  console.log('PASS modifier zoom, bounded panning when enlarged, and paging after fit')
  await mkdir(new URL('../test-results/', import.meta.url), { recursive: true })
  await page.screenshot({ path: new URL(`../test-results/scroll-${which}.png`, import.meta.url).pathname })
  assert.deepEqual(errors, [])
} finally {
  await browser.close()
  await server.close()
}
