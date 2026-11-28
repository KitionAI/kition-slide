// Comment threads a host keeps (Kit UI protocol 2.6 section 7k), in a real browser:
// node tests/comments.mjs [chromium|webkit]. A mocked host (tests/comments/host.html) serves
// `comment.create` and lists `comment-anchors`; the editor sends anchors, marks the threads the
// host shows on the canvas and in the thumbnails, and tells the host which one was clicked.
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
const hostState = () => page.evaluate(() => ({ created: window.commentsHost.created, activated: window.commentsHost.activated }))
const command = (name, params = {}) => page.evaluate(async ([c, p]) => (await window.commentsHost.connection.ai.run([{ command: c, params: p }], false)).completed[0]?.result ?? null, [name, params])
const near = (a, b, within = 24) => Math.abs(a - b) <= within

async function openAs(role) {
  await page.goto(`${origin}/tests/comments/host.html?modes=classic${role ? `&role=${role}` : ''}`)
  await page.waitForFunction(() => document.getElementById('status')?.textContent?.includes(' open in '), null, { timeout: 60000 })
  await page.waitForTimeout(1500)
  return page.frameLocator('#kit')
}

try {
  let kit = await openAs(null)
  await kit.getByTestId('slides-slide-1').click()
  await until(async () => (await kit.getByTestId('slides-status').textContent())?.includes('2'))

  // Slide 2 is fitted with its 40 pt margin; the first box is at slide (60..250, 220..340).
  const zoomOf = async () => Number((await kit.getByTestId('kit-zoom').textContent())?.replace('%', '')) / 100
  const slideToScreen = async (x, y) => {
    const canvas = await kit.getByTestId('kit-canvas-surface').boundingBox()
    const zoom = await zoomOf()
    return { x: canvas.x + canvas.width / 2 - (1040 / 2) * zoom + (40 + x) * zoom, y: canvas.y + canvas.height / 2 - (620 / 2) * zoom + (40 + y) * zoom }
  }
  const box = await slideToScreen(155, 280)
  await page.mouse.click(box.x, box.y)
  await until(async () => (await kit.locator('.kui-status-bar').textContent())?.includes('selected'))

  // Create: the selection goes to the host as an anchor.
  await kit.getByTestId('ribbon-tab-review').click()
  const button = kit.getByTestId('slides-new-comment')
  step('offers Comment to a person who may comment', await button.isEnabled())
  await button.click()
  await until(async () => (await hostState()).created.length === 1)
  const first = (await hostState()).created[0]
  step('sends comment.create with the shape and its slide', first?.ids?.length === 2 && first.page === 1 && !first.ids.some((id) => id.startsWith('pt:')), JSON.stringify(first))
  step('writes no thread into the file', ((await command('comment.threads')) ?? []).length === 0)

  // Marker: at the shape's top-right corner on the canvas, and on its thumbnail.
  const marker = kit.getByTestId('slides-comment-marker-thread-1')
  await until(async () => marker.isVisible())
  step('marks the new thread on the canvas', await marker.isVisible())
  const corner = await slideToScreen(250, 220)
  let placed = await marker.boundingBox()
  step('the marker sits at the shape\'s corner', placed && near(placed.x, corner.x) && near(placed.y + placed.height, corner.y), JSON.stringify({ placed, corner }))
  step('marks the thread on its thumbnail', await kit.getByTestId('slides-slide-1').getByTestId('slides-comment-marker-thumb-thread-1').isVisible())

  // Click: the host hears which thread to open.
  await marker.click()
  await until(async () => (await hostState()).activated.length === 1)
  step('a click sends comments/activated for that thread', JSON.stringify((await hostState()).activated) === '["thread-1"]', JSON.stringify((await hostState()).activated))

  // The shape moves: the marker follows.
  const zoom = await zoomOf()
  await command('shape.move', { dx: 100, dy: 0 })
  await until(async () => near((await marker.boundingBox()).x, placed.x + 100 * zoom, 4))
  const moved = await marker.boundingBox()
  step('the marker follows the shape when it moves', near(moved.x, placed.x + 100 * zoom, 4), `${placed.x} → ${moved.x}`)

  // The slide moves to the front: the thread goes with it.
  await command('slide.move', { from: 1, to: 0 })
  await until(async () => kit.getByTestId('slides-slide-0').getByTestId('slides-comment-marker-thumb-thread-1').isVisible())
  step('the thumbnail marker follows its slide when slides are reordered', await kit.getByTestId('slides-slide-0').getByTestId('slides-comment-marker-thumb-thread-1').isVisible())
  step('comments/locate finds the anchor after the reorder', JSON.stringify(await page.evaluate((anchor) => window.commentsHost.locate([anchor]), first)) === '[true]')

  // A point on the slide, nothing selected.
  await kit.getByTestId('slides-slide-0').click()
  await until(async () => (await kit.getByTestId('slides-status').textContent())?.startsWith('Slide 1'))
  const empty = await slideToScreen(800, 480)
  await page.mouse.click(empty.x, empty.y)
  await page.waitForTimeout(400)
  await button.click()
  await until(async () => (await hostState()).created.length === 2)
  const second = (await hostState()).created[1]
  const point = second?.ids?.[1]?.match(/^pt:(-?\d+),(-?\d+)$/)
  step('a comment on an empty spot anchors the point on the slide', second?.ids?.length === 2 && point && near(Number(point[1]), 800, 6) && near(Number(point[2]), 480, 6), JSON.stringify(second))
  const pinned = await kit.getByTestId('slides-comment-marker-thread-2').boundingBox()
  step('marks it at that point', pinned && near(pinned.x, empty.x) && near(pinned.y + pinned.height, empty.y), JSON.stringify({ pinned, empty }))

  // The shape is deleted: the thread stays on its slide, and the host hears it is not found.
  // A click on the shape away from its words selects it (on its words it would edit them, WPS W-F2).
  const movedShape = await slideToScreen(185, 232)
  await page.mouse.click(movedShape.x, movedShape.y)
  await until(async () => (await kit.locator('.kui-status-bar').textContent())?.includes('selected'))
  await command('edit.delete')
  await until(async () => JSON.stringify(await page.evaluate((anchor) => window.commentsHost.locate([anchor]), first)) === '[false]')
  step('comments/locate reports a deleted shape\'s anchor as not found', JSON.stringify(await page.evaluate((anchors) => window.commentsHost.locate(anchors), [first, second])) === '[false,true]')
  step('its marker stays on the slide', await marker.isVisible())

  // File comments keep working beside the host's: the pane shows none, nothing was written.
  step('the editor kept no thread of its own', ((await command('comment.threads')) ?? []).length === 0)

  // Saved, then opened again by a viewer: the host keeps the replica with the file (as a
  // co-editing host keeps its room), so the threads' anchors are found again. No Comment for a
  // viewer (the host offers no comment.create), but the markers are shown and open their threads.
  const saved = await page.evaluate(() => window.commentsHost.connection.save())
  step('saves through the host', saved.saved === true, JSON.stringify(saved))
  await page.evaluate(() => window.commentsHost.reopen('viewer'))
  await page.waitForFunction(() => document.getElementById('status')?.textContent?.includes(' open in '), null, { timeout: 60000 })
  await page.waitForTimeout(1500)
  await kit.getByTestId('ribbon-tab-review').click()
  step('a viewer is not offered Comment', !(await kit.getByTestId('slides-new-comment').isEnabled()))
  await page.evaluate(([a, b]) => window.commentsHost.show([{ id: 'gone', anchor: a, resolved: true, count: 3 }, { id: 'kept', anchor: b, resolved: false, count: 1 }]), [first, second])
  const kept = kit.getByTestId('slides-comment-marker-kept')
  await until(async () => kept.isVisible())
  step('a viewer sees the host\'s threads after reopening', (await kept.isVisible()) && (await kit.getByTestId('slides-comment-marker-gone').getAttribute('data-resolved')) === 'true')
  step('comments/locate answers the viewer\'s replica', JSON.stringify(await page.evaluate((anchors) => window.commentsHost.locate(anchors), [first, second])) === '[false,true]')
  await kept.click()
  await until(async () => (await hostState()).activated.includes('kept'))
  step('a viewer opens a thread from its marker', (await hostState()).activated.includes('kept'))
} catch (error) {
  failures += 1
  console.log(`FAIL ${error instanceof Error ? error.message.split('\n')[0] : String(error)}`)
} finally {
  if (errors.length) console.log(`page errors:\n${errors.slice(0, 8).join('\n')}`)
  await browser.close()
  await server.close()
}
process.exit(failures ? 1 : 0)
