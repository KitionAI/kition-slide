// Escape in Focus cancels first (KitPointer 4.4): a shape tool gives way to Select and Focus stays;
// with nothing to cancel, Escape leaves Focus. A selected shape is let go of first as well.
// node tests/focus-escape.mjs [chromium|webkit]
import assert from 'node:assert/strict'
import { chromium, webkit } from 'playwright'
import { createServer } from 'vite'

const which = process.argv[2] ?? 'chromium'
const server = await createServer({ configFile: new URL('../vite.config.ts', import.meta.url).pathname, server: { port: 0, host: '127.0.0.1' }, logLevel: 'error' })
await server.listen()
const browser = await (which === 'webkit' ? webkit : chromium).launch()
const page = await browser.newPage({ viewport: { width: 1165, height: 900 } })
try {
  await page.goto(`http://127.0.0.1:${server.httpServer.address().port}/dev.html?modes=classic,focus&locale=en-US`)
  await page.waitForFunction(() => document.getElementById('status')?.textContent?.includes(' open in '), null, { timeout: 90_000 })
  const kit = page.frameLocator('#kit')
  const app = kit.locator('.kui-app')
  const mode = () => app.getAttribute('data-mode')
  const bar = () => kit.locator('.kui-focus-bar').textContent()
  const escape = async () => {
    await page.keyboard.press('Escape')
    await page.waitForTimeout(400)
  }

  // A shape tool: Escape puts it down (Select again) and stays in Focus; the next one leaves.
  await app.focus()
  await page.keyboard.press('r')
  await page.waitForTimeout(300)
  await page.keyboard.press('ControlOrMeta+Backslash')
  assert.equal(await mode(), 'focus', 'Cmd+\\ enters Focus')
  assert.match(await bar(), /Rectangle/, 'the Focus bar names the shape tool')
  await escape()
  assert.equal(await mode(), 'focus', 'an Escape that put the tool down stays in Focus')
  assert.match(await bar(), /Select/, 'Select is the tool again')
  await escape()
  assert.equal(await mode(), 'classic', 'an Escape with nothing to cancel leaves Focus')

  // A selected shape (its text edited after the click): each Escape lets go of one step first.
  const surface = kit.getByTestId('kit-canvas-surface')
  const box = await surface.boundingBox()
  assert(box, 'the canvas is drawn')
  await page.mouse.click(box.x + box.width / 2, box.y + box.height * 0.4)
  await page.waitForTimeout(400)
  await page.keyboard.press('ControlOrMeta+Backslash')
  assert.equal(await mode(), 'focus', 'Cmd+\\ enters Focus with a shape selected')
  let presses = 0
  while ((await mode()) === 'focus' && presses < 4) {
    await escape()
    presses += 1
  }
  assert.equal(await mode(), 'classic', 'Escape leaves Focus once nothing is selected')
  assert(presses >= 2, `the selection was let go of before Focus was left (${presses} presses)`)
  console.log(`PASS kit.slide focus escape (${which})`)
} finally {
  await browser.close()
  await server.close()
}
