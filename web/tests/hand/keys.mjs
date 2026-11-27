// The keyboard works from the first key, and keeps working (plan 0A; walkthrough K1, K2, K4, K7,
// A12, C7, I3, G6, G7, B7; WPS W-A11, W-H3, W-Q1): a person's keys and clicks only, the engine's
// state read back through the development host.
// node tests/hand/keys.mjs [chromium|webkit] [segment…]
import assert from 'node:assert/strict'
import { open, segments } from './harness.mjs'

const press = async (e, key, ms = 300) => {
  await e.page.keyboard.press(key)
  await e.wait(ms)
}

/** Clicks the pasteboard beside the slide (nothing there): nothing selected, the editor has the keyboard. */
async function clickBeside(e) {
  const box = await e.surface()
  await e.page.mouse.click(box.x + 8, box.y + box.height / 2)
  await e.wait(300)
  await e.until(async () => (await e.state()).selected === 0)
}

const showing = (e) => e.kit.getByTestId('slides-show').isVisible().catch(() => false)

async function shapesOn(e, index) {
  return (await e.ask('slide.inspect', { index, select: false }))?.shapes ?? []
}

let prepared = false
/** Six slides; the person on slide 1, nothing selected. */
async function prepare(e) {
  if (!prepared) {
    await e.agent([2, 3, 4, 5].map((at) => ({ command: 'slide.new', params: { at } })))
    await e.until(async () => (await e.kit.locator('.slides-slide').count()) === 6)
    prepared = true
  }
  await e.goto(1)
  await clickBeside(e)
}

const list = [
  ['k1-first-key', async (e) => {
    // Nothing clicked yet: the slides just opened.
    assert.equal(await e.active(), 'div.kui-app', 'the editor has the keyboard after opening')
    await press(e, 'PageDown', 100)
    await e.until(async () => (await e.state()).slide === 1, 40)
    assert.equal((await e.state()).slide, 1, `PageDown with nothing clicked goes to slide 2 ${JSON.stringify(await e.frame().evaluate(() => window.__TRACE_KEYS))} ${JSON.stringify(await e.page.evaluate(() => window.__TRACE_KEYS))}`)
    await press(e, 'PageUp', 100)
    await e.until(async () => (await e.state()).slide === 0, 40)
    assert.equal((await e.state()).slide, 0, 'and PageUp back')
  }],
  ['a12-home-end', async (e) => {
    await prepare(e)
    await press(e, 'End')
    assert.equal((await e.state()).slide, 5, 'End goes to the last slide')
    await press(e, 'Home')
    assert.equal((await e.state()).slide, 0, 'Home goes to the first slide')
    await press(e, 'ArrowRight')
    assert.equal((await e.state()).slide, 1, '→ goes to the next slide')
    await press(e, 'ArrowDown')
    assert.equal((await e.state()).slide, 2, '↓ goes to the next slide')
    await press(e, 'ArrowUp')
    await press(e, 'ArrowLeft')
    assert.equal((await e.state()).slide, 0, '↑ and ← go back')
    // In the slide list as well.
    await e.goto(2)
    await press(e, 'End')
    assert.equal((await e.state()).slide, 5, 'End in the slide list')
    await press(e, 'Home')
    assert.equal((await e.state()).slide, 0, 'Home in the slide list')
  }],
  ['c7-tab-selects', async (e) => {
    await e.goto(0)
    await clickBeside(e)
    const shapes = await shapesOn(e, 0)
    await press(e, 'Tab')
    assert.equal((await e.state()).selected, 1, 'Tab with nothing selected selects an object')
    assert.deepEqual((await e.ask('file.inspect')).selection?.shapes, [shapes[0].id], 'the first one')
    assert(['div.kui-app', 'textarea.kui-text-sink'].includes(await e.active()), 'the keyboard stays on the slide')
    await press(e, 'Escape')
    await press(e, 'Escape')
    await press(e, 'Shift+Tab')
    assert.deepEqual((await e.ask('file.inspect')).selection?.shapes, [shapes.at(-1).id], 'Shift+Tab selects the last one')
  }],
  ['q1-delete-nothing', async (e) => {
    await prepare(e)
    const before = (await e.state()).slides
    await press(e, 'Delete')
    await press(e, 'Backspace')
    assert.equal((await e.state()).slides, before, 'Delete with nothing selected deletes no slide')
  }],
  ['k7-show-with-selection', async (e) => {
    await e.goto(1)
    await clickBeside(e)
    await press(e, 'Tab')
    assert.equal((await e.state()).selected, 1)
    await press(e, 'ControlOrMeta+Enter', 600)
    assert(await showing(e), 'Cmd+Enter with an object selected starts the show')
    await press(e, 'Escape', 600)
    assert(!(await showing(e)), 'Escape ends it')
  }],
  ['k2-focus-after-show', async (e) => {
    await prepare(e)
    await press(e, 'F5', 600)
    assert(await showing(e), 'F5 starts the show')
    await press(e, 'Escape', 600)
    assert(!(await showing(e)), 'Escape ends it')
    assert.equal(await e.active(), 'div.kui-app', 'the editor has the keyboard after the show')
    const at = (await e.state()).slide
    await press(e, 'PageDown', 500)
    assert.equal((await e.state()).slide, at + 1, 'PageDown works right after the show')
    await press(e, 'Shift+F5', 600)
    assert(await showing(e), 'Shift+F5 starts the show')
    await press(e, 'Escape', 600)
  }],
  ['i3-redo-key', async (e) => {
    await e.goto(1)
    await clickBeside(e)
    const count = (await shapesOn(e, 1)).length
    await press(e, 'Tab')
    await press(e, 'Delete', 500)
    assert.equal((await shapesOn(e, 1)).length, count - 1, 'Delete deletes the selected object')
    await press(e, 'ControlOrMeta+z', 500)
    assert.equal((await shapesOn(e, 1)).length, count, 'Cmd+Z brings it back')
    await press(e, 'ControlOrMeta+Shift+z', 500)
    assert.equal((await shapesOn(e, 1)).length, count - 1, 'Cmd+Shift+Z redoes')
    await press(e, 'ControlOrMeta+z', 500)
    await press(e, 'ControlOrMeta+y', 500)
    assert.equal((await shapesOn(e, 1)).length, count - 1, 'Cmd+Y still redoes')
    await press(e, 'ControlOrMeta+z', 500)
  }],
  ['g6-link-url', async (e) => {
    await prepare(e)
    await press(e, 'Tab')
    const id = (await e.ask('file.inspect')).selection.shapes[0]
    await press(e, 'ControlOrMeta+k', 500)
    const dialog = e.kit.getByTestId('slides-link-dialog')
    assert(await dialog.isVisible(), 'Cmd+K opens the link dialog')
    await e.page.keyboard.type('example.com/notes')
    await press(e, 'Enter', 500)
    assert(!(await dialog.isVisible().catch(() => false)), 'Enter applies and closes it')
    const linked = await e.ask('shape.inspect', { id })
    assert.equal(linked?.shape?.click?.action?.url, 'https://example.com/notes', 'the shape links to the address typed')
    await press(e, 'ControlOrMeta+k', 500)
    assert.equal(await e.kit.getByTestId('slides-link-url').inputValue(), 'https://example.com/notes', 'the dialog shows the link the shape has')
    await e.kit.getByTestId('slides-link-to-slide').check()
    await e.kit.getByTestId('slides-link-slide').selectOption('3')
    await e.kit.getByTestId('slides-link-apply').click()
    await e.wait(400)
    const facts = await e.ask('file.inspect')
    assert.equal((await e.ask('shape.inspect', { id }))?.shape?.click?.action?.slide, facts.slides[3].id, 'the shape links to slide 4')
    // New Comment by its key: the comment composer, no raw error.
    await press(e, 'ControlOrMeta+Alt+m', 500)
    const toasts = await e.kit.locator('.kui-toast').allTextContents().catch(() => [])
    assert(!toasts.some((text) => /invalid parameters/.test(text)), `no raw error (${toasts.join(' | ')})`)
  }],
  ['g7-paste-button', async (e) => {
    await e.goto(1)
    await clickBeside(e)
    const count = (await shapesOn(e, 1)).length
    await press(e, 'Tab')
    await press(e, 'ControlOrMeta+c')
    await e.kit.getByTestId('slides-cmd-edit.paste').click()
    await e.wait(500)
    assert.equal(await e.kit.locator('.kui-dialog').count(), 0, 'Paste opens no form')
    assert.equal((await shapesOn(e, 1)).length, count + 1, 'Paste pastes')
    await press(e, 'ControlOrMeta+z', 500)
  }],
  ['k4-escape-panel', async (e) => {
    await e.goto(1)
    await clickBeside(e)
    await press(e, 'Tab')
    const more = e.kit.locator('.kui-line-more').first()
    await more.click()
    await e.wait(300)
    assert.equal(await more.getAttribute('aria-expanded'), 'true', 'a group\'s panel is open')
    await press(e, 'Escape')
    assert.notEqual(await more.getAttribute('aria-expanded'), 'true', 'Escape closes the panel')
    assert.equal((await e.state()).selected, 1, 'and leaves the selection alone')
  }],
  ['b7-thumb-menu-close', async (e) => {
    await prepare(e)
    const menu = e.kit.getByTestId('slides-slide-menu')
    const thumb = e.kit.getByTestId('slides-slide-2')
    // The pointer leaving it keeps it; Escape closes it and the slide list has the keyboard.
    await thumb.click({ button: 'right' })
    assert(await menu.isVisible(), 'a right click opens the thumbnail\'s menu')
    const box = await e.surface()
    await e.page.mouse.move(box.x + box.width / 2, box.y + box.height / 2, { steps: 5 })
    await e.wait(200)
    assert(await menu.isVisible(), 'the menu stays when the pointer leaves it')
    await press(e, 'Escape')
    assert(!(await menu.isVisible().catch(() => false)), 'Escape closes it')
    assert.equal(await e.active(), 'div.slides-slides-list', 'the slide list has the keyboard')
    // A press elsewhere and the wheel close it.
    await thumb.click({ button: 'right' })
    await e.wait(200)
    await e.page.mouse.click(box.x + 8, box.y + 8)
    await e.wait(200)
    assert(!(await menu.isVisible().catch(() => false)), 'a press elsewhere closes it')
    await thumb.click({ button: 'right' })
    await e.wait(200)
    await e.page.mouse.move(box.x + box.width / 2, box.y + box.height / 2)
    await e.page.mouse.wheel(0, 40)
    await e.wait(200)
    assert(!(await menu.isVisible().catch(() => false)), 'the wheel closes it')
    // Duplicate, then Delete takes the copy away again: the keyboard was not lost.
    const slides = (await e.state()).slides
    await thumb.click({ button: 'right' })
    await e.kit.getByTestId('slides-menu-duplicate').click()
    await e.until(async () => (await e.state()).slides === slides + 1)
    assert.equal((await e.state()).slides, slides + 1, 'Duplicate adds a slide')
    assert(!(await menu.isVisible().catch(() => false)), 'and closes the menu')
    await press(e, 'Delete', 500)
    assert.equal((await e.state()).slides, slides, 'Delete right after removes it')
    // Near the window's bottom the menu stays inside the window.
    const viewport = e.page.viewportSize()
    await e.page.mouse.click(box.x - 100, viewport.height - 70, { button: 'right' })
    if (await menu.isVisible().catch(() => false)) {
      const shown = await menu.boundingBox()
      assert(shown.y + shown.height <= viewport.height, `the menu stays inside the window (${shown.y + shown.height} > ${viewport.height})`)
      await press(e, 'Escape')
    }
  }],
]

await segments('keys', list, (which) => open({ which }))
