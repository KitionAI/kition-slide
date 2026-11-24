// A file changed outside Kition keeps its stored replica (Kit UI protocol UI-R1): the engine
// merges the newer file into it as one `import` change, so the document keeps its identity and
// its objects their ids. Runs for the kit's own .kslide and for a PPTX, through the development
// host with `?replica=1&replica-merges-file=1`: node tests/replica.mjs [chromium|webkit]
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
const run = (command, params = {}) => page.evaluate(async ({ command, params }) => (await window.devHost.connection().ai.run([{ command, params }], false)).completed[0].result, { command, params })
const until = async (check, message) => {
  for (let n = 0; n < 200; n += 1) {
    if (await check().catch(() => false)) return
    await page.waitForTimeout(100)
  }
  assert.fail(message)
}
const opened = () => page.waitForFunction(() => document.getElementById('status')?.textContent?.includes(' open in '), null, { timeout: 60000 })
const version = (path) => page.evaluate((path) => window.devHost.files.get(path)?.version ?? 0, path)
const docId = () => page.evaluate(() => window.devHost.connection().replica.info())
/** Every shape of every slide (an agent's read of each slide). */
const shapes = async () => {
  const count = (await run('document.inspect')).slides.length
  const all = []
  for (let index = 0; index < count; index += 1) all.push(...(await run('slide.inspect', { index })).shapes)
  return all
}
const shapeWith = async (text) => (await shapes()).find((shape) => (shape.text ?? '').includes(text))
const save = async (path) => {
  const before = await version(path)
  await kit.getByTestId('slides-save').click()
  await until(async () => (await version(path)) > before, `${path} saved`)
  await until(async () => page.evaluate((path) => window.devHost.history.replica(path) !== null, path), `${path}: its replica stored with the save`)
}
let failures = 0

/** Open `path`, edit, save; change the file outside; reopen: the stored replica is kept and used. */
async function changedOutside(path, format) {
  await run('shape.insert', { preset: 'rect', rect: [60, 60, 200, 80], text: 'Kept before' })
  await save(path)
  const kept = await shapeWith('Kept before')
  assert.ok(kept, `${path}: the edit is in the document`)
  const id = await docId()
  assert.ok(id, `${path}: the saved document has a replica`)

  // Another program's save: the same presentation with one more shape, at a new file version.
  await run('shape.insert', { preset: 'ellipse', rect: [400, 60, 160, 100], text: 'Changed outside' })
  const { data } = await run('file.saveBytes', { format })
  const stored = await page.evaluate((path) => window.devHost.history.replica(path)?.fileStamp, path)
  await page.evaluate(async (data) => {
    const bytes = Uint8Array.from(atob(data), (c) => c.charCodeAt(0))
    await window.devHost.reopenChanged(bytes.buffer)
  }, data)
  await opened()
  await until(async () => Boolean(await shapeWith('Changed outside')), `${path}: the outside change shows after reopening`)
  assert.equal(await page.evaluate((path) => window.devHost.history.replica(path)?.fileStamp, path), stored, `${path}: the stored replica is still the one saved before the change`)
  assert.equal(await docId(), id, `${path}: the stored replica opened: the document keeps its identity`)
  assert.equal((await shapeWith('Kept before'))?.id, kept.id, `${path}: an object from before keeps its id`)

  // Editing and saving again continues the same replica.
  await run('shape.insert', { preset: 'rect', rect: [60, 300, 200, 80], text: 'After the change' })
  await save(path)
  assert.equal(await docId(), id, `${path}: the next save continues the same replica`)
  assert.notEqual(await page.evaluate((path) => window.devHost.history.replica(path)?.fileStamp, path), stored, `${path}: the replica is stored with the new file`)
  console.log(`PASS ${path}: a file changed outside merges into its stored replica`)
}

try {
  await page.goto(`http://127.0.0.1:${server.httpServer.address().port}/dev.html?modes=classic&locale=en-US&replica=1&replica-merges-file=1`)
  await opened()
  await page.waitForTimeout(1000)
  await changedOutside('Sample.kslide', 'kslide')

  const { data } = await run('file.saveBytes', { format: 'pptx' })
  await page.evaluate(async (data) => {
    const bytes = Uint8Array.from(atob(data), (c) => c.charCodeAt(0))
    await window.devHost.open('Deck.pptx', bytes.buffer)
  }, data)
  await opened()
  await page.waitForTimeout(1000)
  await changedOutside('Deck.pptx', 'pptx')
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
