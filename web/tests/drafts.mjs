// An agent's draft and a restored version in the editor (Kit UI protocol sections 7d and 7m): the
// agent makes a draft, the person edits the document meanwhile, the person accepts the draft and
// one Undo takes the whole draft back (the person's edit stays); a checkpoint restored over a later
// edit is one Undo step too. Through the development host with `?checkpoints=1`:
// node tests/drafts.mjs [chromium|webkit]
import assert from 'node:assert/strict'
import { chromium, webkit } from 'playwright'
import { createServer } from 'vite'

const which = process.argv[2] ?? 'chromium'
const server = await createServer({ configFile: new URL('../vite.config.ts', import.meta.url).pathname, server: { port: 0, host: '127.0.0.1' }, logLevel: 'error' })
await server.listen()
const browser = await (which === 'webkit' ? webkit : chromium).launch()
const page = await browser.newPage({ viewport: { width: 1440, height: 900 } })
const kit = page.frameLocator('#kit')
const errors = []
page.on('pageerror', (error) => errors.push(error.message))
const run = (command, params = {}, options = {}) =>
  page.evaluate(async ({ command, params, options }) => (await window.devHost.connection().ai.run([{ command, params }], false, options)).completed[0].result, { command, params, options })
const until = async (check, message) => {
  for (let n = 0; n < 200; n += 1) {
    if (await check().catch(() => false)) return
    await page.waitForTimeout(100)
  }
  assert.fail(message)
}
const opened = () => page.waitForFunction(() => document.getElementById('status')?.textContent?.includes(' open in '), null, { timeout: 60000 })
/** The first slide as an agent reads it, in the document or (`draft`) in the agent's draft. */
const first = (draft = false) => run('slide.inspect', { index: 0 }, draft ? { draft: true } : {})
const has = (slide, text) => slide.shapes.some((shape) => (shape.text ?? '').includes(text))
const notes = async () => (await first()).notes ?? ''
/** The person types `text` into the first slide's notes and leaves them. */
const typeNotes = async (text) => {
  await kit.getByTestId('slides-notes-text').click()
  await kit.getByTestId('slides-notes-text').fill(text)
  await kit.getByTestId('slides-notes-text').blur()
  await until(async () => (await notes()) === text, `the notes read ${text}`)
}
const undo = async () => {
  await until(() => kit.getByTestId('slides-undo').isEnabled(), 'Undo is enabled')
  await kit.getByTestId('slides-undo').click()
}
const path = 'Sample.kslide'
const version = () => page.evaluate((path) => window.devHost.files.get(path)?.version ?? 0, path)
let failures = 0

try {
  await page.goto(`http://127.0.0.1:${server.httpServer.address().port}/dev.html?modes=classic&locale=en-US&checkpoints=1`)
  await opened()
  await page.waitForTimeout(1000)
  const shapesBefore = (await first()).shapes.length

  // The agent drafts a shape; the document does not show it.
  await run('shape.insert', { preset: 'rect', rect: [60, 60, 200, 80], text: 'From the draft' }, { draft: true })
  assert.ok(has(await first(true), 'From the draft'), 'the draft holds the agent\'s shape')
  assert.ok(!has(await first(), 'From the draft'), 'the document does not, yet')
  // The person edits the document meanwhile.
  await typeNotes('Meanwhile')
  // The person accepts the draft (the host passes the agent's origin): both edits are kept.
  const merged = await page.evaluate(() => window.devHost.connection().draft.merge({ kind: 'agent', principal: 'agent:test' }))
  assert.equal(merged.merged, true, 'the draft merged')
  await until(async () => has(await first(), 'From the draft'), 'the merged shape shows')
  assert.equal((await first()).shapes.length, shapesBefore + 1)
  assert.equal(await notes(), 'Meanwhile', 'the edit made meanwhile stays')
  // One Undo of the person takes the whole draft back and leaves the person's edit.
  await undo()
  await until(async () => !has(await first(), 'From the draft'), 'Undo takes the draft back')
  assert.equal((await first()).shapes.length, shapesBefore)
  assert.equal(await notes(), 'Meanwhile', 'the person\'s edit stays')
  console.log(`PASS ${which}: a merged draft is one step of the person's Undo`)

  // A checkpoint of the saved document; an edit after it; the checkpoint restored; Undo.
  const saved = await version()
  await kit.getByTestId('slides-save').click()
  await until(async () => (await version()) > saved, 'saved')
  await until(() => page.evaluate((path) => window.devHost.history.replica(path) !== null, path), 'the replica is stored with the save')
  const checkpoint = await page.evaluate(() => window.devHost.recordCheckpoint({ origin: 'save' }))
  assert.ok(checkpoint?.id, 'a checkpoint is recorded')
  await typeNotes('After the checkpoint')
  await page.evaluate((id) => window.devHost.restoreCheckpoint(id), checkpoint.id)
  await until(async () => (await notes()) === 'Meanwhile', 'the restore gives back the checkpoint\'s notes')
  assert.equal((await first()).shapes.length, shapesBefore)
  await undo()
  await until(async () => (await notes()) === 'After the checkpoint', 'Undo takes the restore back')
  assert.equal((await first()).shapes.length, shapesBefore)
  console.log(`PASS ${which}: a restored version is one step of the person's Undo`)
  assert.deepEqual(errors, [], 'no page errors')
} catch (error) {
  failures += 1
  console.log(`FAIL ${error instanceof Error ? error.message.split('\n')[0] : String(error)}`)
} finally {
  if (errors.length) console.log(`page errors:\n${errors.slice(0, 8).join('\n')}`)
  await browser.close()
  await server.close()
}
process.exit(failures ? 1 : 0)
