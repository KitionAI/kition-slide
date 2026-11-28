// The slide panel by hand (plan stage 7; walkthrough B2, B4, B6, B9, B12, A14; WPS W-K2 to W-K7):
// Shift and Cmd select several slides, a drag moves them while the others make room (no insertion
// line), Escape puts them back, Delete, Cmd+D and the menu act on every selected slide in one undo
// step, the sorter opens a slide on a double-click, and a typed character draws one thumbnail
// again. A person's mouse and keys only; the deck is read back from the engine.
// node tests/hand/thumbnails.mjs [chromium|webkit] [segment…]
import assert from 'node:assert/strict'
import { open, segments } from './harness.mjs'

const MOD = 'ControlOrMeta'

const thumb = (e, index) => e.kit.getByTestId(`slides-slide-${index}`)
/** The slide ids in deck order. */
const order = async (e) => ((await e.ask('file.inspect'))?.slides ?? []).map((slide) => slide.id)
/** The slides selected in the panel (indexes from 0), and the current one. */
const selection = async (e) => {
  const info = await e.inspected()
  return { slides: info.selectedSlides, current: info.slide }
}
const press = async (e, key, ms = 300) => {
  await e.page.keyboard.press(key)
  await e.wait(ms)
}
/** The centre of thumbnail `index` on the page. */
async function centre(e, index) {
  await thumb(e, index).scrollIntoViewIfNeeded()
  const box = await thumb(e, index).boundingBox()
  return { x: box.x + box.width / 2, y: box.y + box.height / 2, box }
}
/** Clicks thumbnail `index` with `modifiers` held. */
async function clickThumb(e, index, modifiers = []) {
  await thumb(e, index).scrollIntoViewIfNeeded()
  await thumb(e, index).click({ modifiers })
  await e.wait(250)
}

let prepared = false
/** Eight slides (the sample's two and six Title Only ones); the person on slide 1, the panel with the keyboard. */
async function prepare(e) {
  if (!prepared) {
    const have = (await e.state()).slides
    await e.agent(Array.from({ length: 8 - have }, (_, k) => ({ command: 'slide.new', params: { at: have + k, layout: 'titleOnly', title: `Slide ${have + k + 1}` } })))
    await e.until(async () => (await e.kit.locator('.slides-slide').count()) === 8)
    prepared = true
  }
  await clickThumb(e, 0)
  await e.until(async () => (await selection(e)).slides?.length === 1 && (await selection(e)).current === 0)
}

/** Presses thumbnail `from` and moves the pointer by (`dx`, `dy`) in steps, without releasing. */
async function dragBy(e, from, dx, dy) {
  const at = await centre(e, from)
  await e.page.mouse.move(at.x, at.y)
  await e.page.mouse.down()
  await e.page.mouse.move(at.x + 2, at.y + 3, { steps: 2 })
  await e.page.mouse.move(at.x + dx, at.y + dy, { steps: 12 })
  await e.wait(250)
  return at
}

const list = [
  ['b2-multi-select', async (e) => {
    await prepare(e)
    await clickThumb(e, 1)
    await clickThumb(e, 3, ['Shift'])
    assert.deepEqual(await selection(e), { slides: [1, 2, 3], current: 3 }, 'Shift+click selects the run from the slide last clicked')
    await clickThumb(e, 2, [MOD])
    assert.deepEqual((await selection(e)).slides, [1, 3], 'Cmd+click takes a slide away')
    await clickThumb(e, 5, [MOD])
    assert.deepEqual(await selection(e), { slides: [1, 3, 5], current: 5 }, 'Cmd+click adds one, and it is current')
    assert.equal(await e.kit.locator('.slides-slide[data-selected="true"]').count(), 3, 'every selected thumbnail is marked')
    await press(e, 'Shift+ArrowDown')
    assert.deepEqual(await selection(e), { slides: [5, 6], current: 6 }, 'Shift+Down extends from the slide last clicked')
    await press(e, `${MOD}+a`)
    assert.deepEqual((await selection(e)).slides, [0, 1, 2, 3, 4, 5, 6, 7], 'Cmd+A in the panel selects every slide')
    assert.equal((await e.state()).selected, 0, 'and no object on the slide')
    await clickThumb(e, 4)
    assert.deepEqual(await selection(e), { slides: [4], current: 4 }, 'a click alone selects one slide')
  }],
  ['b4-drag-reflow', async (e) => {
    await prepare(e)
    const before = await order(e)
    await clickThumb(e, 1)
    await clickThumb(e, 3, ['Shift'])
    // Slides 2 to 4 after slide 6: thumbnail 3 (the last of them) a little past the middle of slide 6's.
    const from = await centre(e, 3)
    const to = await centre(e, 5)
    const sixth = (await thumb(e, 5).boundingBox()).y
    await dragBy(e, 3, 0, to.y + 12 - from.y)
    const pressed = await thumb(e, 3).boundingBox()
    assert(Math.abs(pressed.y + pressed.height / 2 - (to.y + 12)) < 6, `the pressed thumbnail follows the pointer (${pressed.y} vs ${to.y})`)
    const moved = (await thumb(e, 5).boundingBox()).y
    assert(moved < sixth - 100, `slide 6 has made room live, moving up (${sixth} → ${moved})`)
    assert.equal(await e.kit.locator('.slides-slides-list [role="separator"], .slides-insertion').count(), 0, 'there is no insertion line')
    await e.page.mouse.up()
    await e.until(async () => (await order(e))[3] === before[1])
    assert.deepEqual(await order(e), [before[0], before[4], before[5], before[1], before[2], before[3], before[6], before[7]], 'the three slides moved together after slide 6')
    assert.deepEqual((await selection(e)).slides, [3, 4, 5], 'and stay selected')
    await press(e, `${MOD}+z`, 500)
    assert.deepEqual(await order(e), before, 'one undo puts them back')
  }],
  ['b4-drag-to-end', async (e) => {
    await prepare(e)
    // Twelve slides: more than the panel shows.
    await e.agent([8, 9, 10, 11].map((at) => ({ command: 'slide.new', params: { at, layout: 'titleOnly', title: `Slide ${at + 1}` } })))
    await e.until(async () => (await e.kit.locator('.slides-slide').count()) === 12)
    await clickThumb(e, 0)
    const before = await order(e)
    await clickThumb(e, 1)
    const listBox = await e.kit.locator('.slides-slides-list').boundingBox()
    const scrolled = () => e.kit.locator('.slides-slides-list').evaluate((element) => element.scrollTop)
    assert.equal(await scrolled(), 0)
    // Down to the panel's bottom edge: it scrolls to the end, and no further.
    const at = await centre(e, 1)
    await dragBy(e, 1, 0, listBox.y + listBox.height - 6 - at.y)
    await e.wait(1500)
    const most = await e.kit.locator('.slides-slides-list').evaluate((element) => element.scrollHeight - element.clientHeight)
    assert(await scrolled() > 0, 'the panel scrolls while the pointer is at its bottom edge')
    assert((await scrolled()) <= most + 1, 'and stops at its end')
    // Below the last thumbnail, past the panel's end.
    await e.page.mouse.move(at.x, listBox.y + listBox.height + 10, { steps: 4 })
    await e.wait(250)
    await e.page.mouse.up()
    try {
      await e.until(async () => (await order(e))[11] === before[1])
      assert.deepEqual(await order(e), [before[0], ...before.slice(2), before[1]], 'a drop past the last slide puts it at the end')
      await press(e, `${MOD}+z`, 500)
      assert.deepEqual(await order(e), before)
    } finally {
      const now = await order(e)
      await e.agent([{ command: 'slide.delete', params: { ids: now.filter((id) => before.slice(8).includes(id)) } }])
      await e.until(async () => (await e.kit.locator('.slides-slide').count()) === 8)
    }
  }],
  ['b4-escape-cancels', async (e) => {
    await prepare(e)
    const before = await order(e)
    const undo = (await e.ask('file.inspect'))?.undo ?? null
    await clickThumb(e, 2)
    const second = (await thumb(e, 3).boundingBox()).y
    await dragBy(e, 2, 0, 230)
    assert((await thumb(e, 3).boundingBox()).y < second - 50, 'the others made room')
    await press(e, 'Escape')
    await e.page.mouse.up()
    await e.wait(400)
    assert.deepEqual(await order(e), before, 'Escape leaves the order as it was')
    assert.equal((await e.ask('file.inspect'))?.undo ?? null, undo, 'and leaves no undo step')
    assert(Math.abs((await thumb(e, 3).boundingBox()).y - second) < 2, 'the thumbnails are back in place')
  }],
  ['delete-several', async (e) => {
    await prepare(e)
    const before = await order(e)
    await clickThumb(e, 2)
    await clickThumb(e, 4, ['Shift'])
    await press(e, 'Delete', 500)
    assert.deepEqual(await order(e), [...before.slice(0, 2), ...before.slice(5)], 'Delete takes the three selected slides')
    await press(e, `${MOD}+z`, 500)
    assert.deepEqual(await order(e), before, 'one undo brings all three back')
  }],
  ['b9-duplicate-key', async (e) => {
    await prepare(e)
    const before = await order(e)
    await clickThumb(e, 1)
    await press(e, `${MOD}+d`, 500)
    let now = await order(e)
    assert.equal(now.length, before.length + 1, 'Cmd+D duplicates the slide')
    assert.equal((await selection(e)).current, 2, 'the copy comes after it and is current')
    await press(e, `${MOD}+Shift+d`, 500)
    assert.equal((await order(e)).length, before.length + 2, 'Cmd+Shift+D too')
    await press(e, `${MOD}+z`, 400)
    await press(e, `${MOD}+z`, 400)
    assert.deepEqual(await order(e), before)
    await clickThumb(e, 3)
    await clickThumb(e, 5, [MOD])
    await press(e, `${MOD}+d`, 500)
    now = await order(e)
    assert.equal(now.length, before.length + 2, 'Cmd+D duplicates every selected slide')
    await press(e, `${MOD}+z`, 500)
    assert.deepEqual(await order(e), before, 'in one undo step')
  }],
  ['copy-paste-several', async (e) => {
    // The system clipboard's keys are tested where the browser sends their events with the
    // clipboard granted (Chromium); WebKit's stand-ins are clipboard.mjs's.
    if (e.which !== 'chromium') return
    await prepare(e)
    const before = await order(e)
    // Slides 6 then 3 chosen: copied in deck order, pasted after the slide clicked last.
    await clickThumb(e, 5)
    await clickThumb(e, 2, [MOD])
    await press(e, `${MOD}+c`, 500)
    await clickThumb(e, 7)
    await press(e, `${MOD}+v`, 800)
    const facts = await e.ask('file.inspect')
    assert.equal(facts.slides.length, before.length + 2, 'Cmd+V in the panel pastes both slides')
    const titles = facts.slides.map((slide) => slide.title)
    assert.deepEqual(titles.slice(8), [titles[2], titles[5]], 'in deck order, after the current slide')
    await press(e, `${MOD}+z`, 500)
    assert.deepEqual(await order(e), before, 'one undo takes both away')
  }],
  ['return-follows-layout', async (e) => {
    await prepare(e)
    await clickThumb(e, 4)
    const layout = (await e.ask('file.inspect')).slides[4].layout
    await press(e, 'Enter', 500)
    const facts = await e.ask('file.inspect')
    assert.equal(facts.slides.length, 9, 'Return adds a slide')
    assert.equal(facts.slides[5].layout, layout, 'after the current one, with its layout (WPS W-K2)')
    assert.equal((await selection(e)).current, 5)
    await press(e, `${MOD}+z`, 500)
  }],
  ['b6-menu', async (e) => {
    await prepare(e)
    await thumb(e, 2).click({ button: 'right' })
    const menu = e.kit.getByTestId('slides-slide-menu')
    assert(await e.until(() => menu.isVisible()), 'a right-click opens the slide menu')
    assert.equal((await selection(e)).current, 2, 'on the slide right-clicked')
    for (const id of ['copy', 'cut', 'paste', 'new', 'duplicate', 'delete', 'hide', 'addSection', 'layout', 'reset']) {
      assert(await e.kit.getByTestId(`slides-menu-${id}`).isVisible(), `the menu has ${id}`)
    }
    await e.kit.getByTestId('slides-menu-hide').click()
    await e.until(async () => (await e.ask('file.inspect')).slides[2].hidden === true)
    assert.equal((await e.ask('file.inspect')).slides[2].hidden, true, 'Hide Slide hides it')
    await press(e, `${MOD}+z`, 400)
    // Add Section, then its name is typed in place.
    await thumb(e, 3).click({ button: 'right' })
    await e.kit.getByTestId('slides-menu-addSection').click()
    const name = e.kit.getByTestId('slides-section-name')
    assert(await e.until(() => name.isVisible()), 'a new section asks for its name')
    await e.page.keyboard.type('Part B')
    await press(e, 'Enter', 500)
    const sections = (await e.ask('file.inspect')).sections.map((section) => section.name)
    assert(sections.includes('Part B'), `the section has its name (${sections})`)
    assert(await e.kit.getByText('Part B', { exact: true }).isVisible(), 'and the panel shows it above its first slide')
    await press(e, `${MOD}+z`, 400)
    await press(e, `${MOD}+z`, 400)
  }],
  ['b12-no-prompts', async (e) => {
    await prepare(e)
    await e.agent([{ command: 'slide.new', params: { at: 8, layout: 'titleAndContent' } }])
    await e.until(async () => (await e.kit.locator('.slides-slide').count()) === 9)
    await e.wait(800)
    // The new slide's placeholders are empty: its thumbnail is one color, no prompt or dashed outline.
    const colors = await e.frame().evaluate(() => {
      const canvas = document.querySelector('[data-testid="slides-slide-8"] canvas.slides-thumb')
      const data = canvas.getContext('2d').getImageData(0, 0, canvas.width, canvas.height).data
      const seen = new Set()
      for (let at = 0; at < data.length; at += 4) seen.add(`${data[at]},${data[at + 1]},${data[at + 2]}`)
      return seen.size
    })
    assert.equal(colors, 1, `an empty slide's thumbnail shows no prompt (${colors} colors)`)
    await e.agent([{ command: 'slide.delete', params: { index: 8 } }])
    await e.until(async () => (await e.kit.locator('.slides-slide').count()) === 8)
  }],
  ['redraw-one', async (e) => {
    await prepare(e)
    await e.goto(3)
    const box = await e.ask('textBox.insert', { rect: [100, 300, 300, 60], text: 'Hello' })
    assert(box?.id != null, 'a text box to type in')
    await e.wait(800)
    // Into the words, then one character.
    await e.click(130, 320)
    await e.until(async () => (await e.state()).editing)
    await e.wait(800)
    const drawn = Number(await e.kit.locator('.slides-slides-list').getAttribute('data-drawn'))
    await e.page.keyboard.type('x')
    await e.wait(1000)
    const after = Number(await e.kit.locator('.slides-slides-list').getAttribute('data-drawn'))
    assert.equal(after - drawn, 1, `one typed character draws one thumbnail again (${after - drawn})`)
    await press(e, 'Escape')
    await press(e, 'Escape')
  }],
  ['a14-sorter', async (e) => {
    await prepare(e)
    await e.kit.getByTestId('ribbon-tab-view').click()
    await e.kit.getByTestId('slides-view-sorter').click()
    assert(await e.until(() => e.kit.getByTestId('slides-sorter').isVisible()), 'the sorter shows')
    await e.wait(800)
    // Its thumbnails are drawn at the size they are shown, not stretched.
    const sharp = await e.frame().evaluate(() => {
      const canvas = document.querySelector('[data-testid="slides-slide-1"] canvas.slides-thumb')
      return { pixels: canvas.width, shown: canvas.getBoundingClientRect().width * devicePixelRatio }
    })
    assert(sharp.pixels >= sharp.shown - 1, `the sorter's thumbnails are sharp (${sharp.pixels} px for ${sharp.shown})`)
    await clickThumb(e, 1)
    await clickThumb(e, 2, ['Shift'])
    assert.deepEqual((await selection(e)).slides, [1, 2], 'Shift+click works in the sorter too')
    const before = await order(e)
    const from = await centre(e, 1)
    const to = await centre(e, 4)
    await dragBy(e, 1, to.x + 12 - from.x, to.y - from.y)
    await e.page.mouse.up()
    await e.until(async () => (await order(e))[0] !== before[0] || (await order(e))[1] !== before[1])
    assert.notDeepEqual(await order(e), before, 'a drag in the sorter moves the slides')
    await press(e, `${MOD}+z`, 500)
    assert.deepEqual(await order(e), before)
    await thumb(e, 3).dblclick()
    assert(await e.until(() => e.kit.getByTestId('kit-canvas-surface').isVisible()), 'a double-click opens the slide in the normal view')
    assert.equal((await selection(e)).current, 3, 'that slide')
    // Return in the sorter opens the current slide too.
    await e.kit.getByTestId('ribbon-tab-view').click()
    await e.kit.getByTestId('slides-view-sorter').click()
    await e.until(() => e.kit.getByTestId('slides-sorter').isVisible())
    await clickThumb(e, 5)
    await press(e, 'Enter', 500)
    assert(await e.kit.getByTestId('kit-canvas-surface').isVisible(), 'Return opens it')
    assert.equal((await selection(e)).current, 5)
  }],
]

await segments('thumbnails', list, async (which) => {
  const editor = await open({ which })
  if (which === 'chromium') await editor.page.context().grantPermissions(['clipboard-read', 'clipboard-write'])
  return editor
})
