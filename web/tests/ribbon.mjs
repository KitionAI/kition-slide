// Ribbon and view parity in both browser engines, against the real Rust/Wasm document.
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
const until = async (check) => {
  for (let attempt = 0; attempt < 80; attempt += 1) { if (await check().catch(() => false)) return; await page.waitForTimeout(100) }
  assert.fail('Timed out waiting for the ribbon or view')
}
const tab = async (id) => { await kit.getByTestId(`ribbon-tab-${id}`).click(); await page.waitForTimeout(150) }
const run = (command, params = {}) => page.evaluate(async ({ command, params }) => (await window.devHost.connection().ai.run([{ command, params }], false)).completed[0].result, { command, params })
const visible = async (testId) => until(() => kit.getByTestId(testId).isVisible())
const capture = async (name) => page.screenshot({ path: new URL(`../test-results/ribbon-${which}-${name}.png`, import.meta.url).pathname })

try {
  await mkdir(new URL('../test-results/', import.meta.url), { recursive: true })
  await page.goto(`http://127.0.0.1:${server.httpServer.address().port}/dev.html?modes=classic&locale=en-US`)
  await page.waitForFunction(() => document.getElementById('status')?.textContent?.includes(' open in '), null, { timeout: 60000 })
  await visible('kit-canvas-surface')
  for (const id of ['home', 'insert', 'draw', 'design', 'transitions', 'animations', 'slideShow', 'record', 'review', 'view']) {
    await tab(id)
    assert.ok(await kit.locator('[role="tabpanel"] button').count() > 1, `${id} must contain its native controls`)
    await capture(id)
  }
  for (const label of ['Normal', 'Outline View', 'Slide Sorter', 'Notes Page', 'Reading View', 'Slide Master', 'Handout Master', 'Notes Master', 'Notes', 'Zoom', 'Fit', 'Grayscale', 'Dark Mode']) assert.equal(await kit.getByRole('tabpanel').getByRole('button', { name: label, exact: true }).count(), 1, label)
  assert.equal(await kit.getByRole('button', { name: 'Handout Master', exact: true }).isDisabled(), true)
  await kit.getByTestId('slides-view-sorter').click()
  await visible('slides-sorter')
  await kit.getByTestId('slides-slide-1').click()
  await until(async () => (await kit.getByTestId('slides-status').textContent()) === 'Slide 2 of 2')
  await kit.getByTestId('slides-view-outline').click()
  await until(async () => (await kit.locator('.slides-outline-text').first().textContent().catch(() => null)) != null)
  assert.equal(await kit.locator('.slides-outline-text').count(), 2)
  await kit.getByTestId('slides-view-notesPage').click()
  await visible('slides-notes-page')
  await kit.getByTestId('slides-notes-text').fill('Speaker notes from the notes page')
  await kit.getByTestId('slides-notes-text').blur()
  await until(async () => (await run('slide.inspect', { index: 1 })).notes === 'Speaker notes from the notes page')
  await kit.getByTestId('slides-view-normal').click()
  await visible('kit-canvas-surface')
  for (const control of ['ruler', 'gridlines', 'guides']) await kit.getByTestId(`slides-view-${control}`).check()
  await visible('slides-ruler-x')
  await kit.getByTestId('slides-view-grayscale').click()
  assert.equal(await kit.locator('.slides-canvas-view').getAttribute('data-grayscale'), 'true')
  await kit.getByTestId('slides-view-dark').click()
  assert.equal(await kit.locator('.slides-editor').getAttribute('data-dark'), 'true')
  await capture('view-options')
  await kit.getByTestId('slides-view-dark').click()
  await kit.getByTestId('slides-view-master').click()
  await visible('slides-master-list')
  await visible('ribbon-tab-slideMaster')
  await kit.locator('.slides-layout-item').first().click()
  await until(async () => (await run('file.inspect')).selection.target.kind === 'master')
  await kit.locator('.slides-layout-item').nth(1).click()
  await until(async () => (await run('file.inspect')).selection.target.kind === 'layout')
  await tab('slideMaster')
  const layouts = await kit.locator('.slides-layout-item').count()
  await kit.getByTestId('slides-cmd-master.insertLayout').click()
  await until(async () => (await kit.locator('.slides-layout-item').count()) === layouts + 1)
  await kit.getByTestId('slides-cmd-view.closeMaster').click()
  await visible('slides-slides')
  console.log('PASS every native View control, real view switching, notes editing, guides, master rendering and layout commands')

  for (const [command, params, context] of [
    ['shape.insert', { preset: 'rect', rect: [100, 100, 150, 100] }, 'shapeFormat'],
    ['table.insert', { rows: 2, cols: 2 }, 'tableDesign'],
    ['chart.insert', { type: 'line' }, 'chartDesign'],
  ]) {
    await run(command, params)
    // The person clicks the new object (the topmost): an agent's insertion selects it only in the
    // agent's own editor state (KHP C3b), and the contextual tab follows the person's selection.
    const shapes = (await run('slide.inspect')).shapes
    const [x, y, w, h] = shapes[shapes.length - 1].box
    const bounds = await kit.getByTestId('kit-canvas-surface').boundingBox()
    const zoom = Number((await kit.getByTestId('kit-zoom').textContent()).replace('%', '')) / 100
    await page.mouse.click(bounds.x + bounds.width / 2 + (x + w / 2 - 480) * zoom, bounds.y + bounds.height / 2 + (y + h / 2 - 270) * zoom)
    await visible(`ribbon-tab-${context}`)
    await tab(context)
    assert.ok(await kit.locator('[role="tabpanel"] button').count() > 2)
    await capture(context)
    if (context === 'shapeFormat') {
      // Art Text Styles › Transform: the shared menu bends the selected shape's text.
      await kit.getByTestId('slide-drawing-transform').click()
      await kit.getByTestId('slide-drawing-warp-textArchUp').click()
      await until(async () => (await run('drawing.describe')).objects.some((o) => o.text?.warp?.preset === 'textArchUp'))
    }
  }
  await visible('ribbon-tab-chartDesign')
  await kit.getByTestId('slides-chart-type').click()
  await kit.getByTestId('slides-chart-type-pie').click()
  await until(async () => (await run('slide.inspect')).shapes.some((shape) => shape.chart?.type?.toLowerCase().includes('pie')))
  console.log('PASS object-specific Shape, Table and Chart contextual menus expose engine commands')

  await tab('review')
  await kit.getByRole('button', { name: 'Check Accessibility', exact: true }).click()
  await visible('slides-pane-accessibility')
  await kit.getByRole('button', { name: 'Close', exact: true }).click()
  await kit.getByRole('button', { name: 'Show Comments', exact: true }).click()
  await visible('slides-pane-comments')
  await run('comment.add', { text: 'Review comment' })
  await until(async () => (await kit.getByTestId('slides-pane-comments').textContent()).includes('Review comment'))
  // A comment through the panel: New Comment opens its composer; a reply goes under the thread.
  await kit.getByTestId('slides-new-comment').click()
  await kit.getByTestId('slides-comments-new-input').fill('Typed comment')
  await kit.getByTestId('slides-comments-new-send').click()
  await until(async () => (await run('comment.threads')).some((thread) => thread.comments[0].body === 'Typed comment'))
  await kit.getByTestId('slides-comments-reply-input').first().fill('A reply')
  await kit.getByTestId('slides-comments-reply-send').first().click()
  await until(async () => (await run('comment.threads')).some((thread) => thread.comments.some((comment) => comment.body === 'A reply')))
  await until(async () => (await kit.getByTestId('slides-comments-comment').count()) >= 3)
  await tab('home')
  await kit.getByRole('button', { name: 'Design Ideas', exact: true }).click()
  await until(async () => (await kit.locator('.slides-design-idea').count()) > 0)
  await capture('design-ideas')
  await kit.getByRole('button', { name: 'Close', exact: true }).click()
  await tab('view')
  await kit.getByTestId('slides-view-reading').click()
  await visible('slides-show')
  assert.equal(await kit.getByTestId('slides-show').getAttribute('data-reading'), 'true')
  await page.keyboard.press('Escape')
  await until(async () => !(await kit.getByTestId('slides-show').isVisible()))
  assert.deepEqual(errors, [])
  console.log('PASS query results and comments stay visible in task panes, design previews render, and Reading View returns to the editor')

  // A commenter: the document is read-only, yet the comment commands run (kit.json
  // collaboration.comments.commands) and every other edit is refused.
  await page.goto(`http://127.0.0.1:${server.httpServer.address().port}/dev.html?modes=classic&locale=en-US&role=commenter`)
  await page.waitForFunction(() => document.getElementById('status')?.textContent?.includes(' open in '), null, { timeout: 60000 })
  await visible('kit-canvas-surface')
  const refusal = (command) => page.evaluate(async (command) => {
    try {
      await window.devHost.connection().ai.run([{ command, params: {} }], false)
      return null
    } catch (error) {
      return String(error?.message ?? error)
    }
  }, command)
  assert.match((await refusal('slide.new')) ?? 'ran', /read-only/)
  assert.match((await refusal('edit.undo')) ?? 'ran', /read-only/)
  await run('comment.add', { text: 'From a commenter' })
  await until(async () => (await run('comment.threads')).some((thread) => thread.comments[0].body === 'From a commenter' && thread.comments[0].author === 'dev-commenter'))
  await tab('review')
  await kit.getByRole('button', { name: 'Show Comments', exact: true }).click()
  await visible('slides-pane-comments')
  await kit.getByTestId('slides-new-comment').click()
  await kit.getByTestId('slides-comments-new-input').fill('Typed by a commenter')
  await kit.getByTestId('slides-comments-new-send').click()
  await until(async () => (await run('comment.threads')).some((thread) => thread.comments[0].body === 'Typed by a commenter'))
  assert.equal((await run('file.inspect')).slides.length, 2, 'a commenter adds no slide')
  assert.deepEqual(errors, [])
  console.log('PASS a commenter writes comments in a read-only document and nothing else')
} finally { await browser.close(); await server.close() }
