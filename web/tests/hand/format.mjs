// Align, arrange and size without parameter forms (plan stage 1; walkthrough G5, D13, G8, G11, G2;
// WPS W-A12, W-A15, W-P1, W-P2): a person's mouse and keys only, in the default Single line mode;
// the shapes are read back from the engine.
// node tests/hand/format.mjs [chromium|webkit] [segment…]
import assert from 'node:assert/strict'
import { open, segments } from './harness.mjs'

/** An 8 × 6 px picture made for this test. */
const PNG = 'iVBORw0KGgoAAAANSUhEUgAAAAgAAAAGCAIAAABxZ0isAAAAEUlEQVR4nGM4EaCBFTEMpAQAs5U8AfXQ54MAAAAASUVORK5CYII='
const CM = 72 / 2.54
const near = (a, b, within = 0.05) => Math.abs(a - b) <= within

const press = async (e, key, ms = 300) => {
  await e.page.keyboard.press(key)
  await e.wait(ms)
}
/** The tab shown, and the display mode the ribbon is in. */
const activeTab = (e) => e.kit.locator('.kui-ribbon-tab[aria-selected="true"]').getAttribute('data-tab')
const mode = (e) => e.kit.locator('.kui-app').getAttribute('data-mode')
const forms = (e) => e.kit.locator('.kui-dialog').count()

/** A new blank slide 2 (made current) with `shapes` on it, nothing selected; answers their ids. */
async function fresh(e, shapes) {
  await e.agent([{ command: 'slide.new', params: { layout: 'Blank', at: 1 } }])
  await e.goto(1)
  const ids = []
  for (const shape of shapes) {
    const answer = shape.picture ? await e.ask('image.insert', { data: PNG, name: 'test.png', rect: shape.rect }) : await e.ask('shape.insert', shape)
    ids.push(answer?.id)
  }
  await e.click(930, 20)
  await e.until(async () => (await e.state()).selected === 0)
  return ids
}

/** Shape `id`'s box in slide points, `rot` 0 when upright. */
const boxOf = async (e, id) => {
  const box = (await e.ask('shape.inspect', { id }))?.box
  return box ? { ...box, rot: box.rot ?? 0 } : box
}
const shapesOn = async (e) => (await e.ask('slide.inspect', { index: (await e.state()).slide, select: false }))?.shapes ?? []

/** Clicks slide point (`x`, `y`) with Shift held: adds to the selection. */
async function shiftClick(e, x, y) {
  await e.page.keyboard.down('Shift')
  await e.click(x, y)
  await e.page.keyboard.up('Shift')
}

/** Opens the menu of the button `testId` in the ribbon's row and clicks its row `item`. */
async function choose(e, testId, item) {
  await e.kit.getByTestId(testId).first().click()
  await e.wait(200)
  await e.kit.getByTestId(item).click()
  await e.wait(400)
}

/** The shown tab becomes `id` (a contextual tab follows the selection a moment later). */
async function tabIs(e, id, why) {
  await e.until(async () => (await activeTab(e)) === id, 30)
  assert.equal(await activeTab(e), id, why)
}

const list = [
  ['g5-align-direct', async (e) => {
    const [a, b] = await fresh(e, [{ preset: 'rect', rect: [100, 100, 160, 100] }, { preset: 'rect', rect: [400, 180, 160, 100] }])
    await e.click(180, 150)
    await shiftClick(e, 480, 230)
    assert.equal((await e.state()).selected, 2, 'Shift+click adds the second')
    await tabIs(e, 'shapeFormat', 'the selection brings Shape Format forward')
    // Align › Align Top: two clicks, no form.
    await choose(e, 'slides-align', 'slides-align-top')
    assert.equal(await forms(e), 0, 'Align opens no parameter form')
    assert.equal((await boxOf(e, a)).y, 100, 'the first keeps its top')
    assert.equal((await boxOf(e, b)).y, 100, 'the second moves up to it: two objects align to their selection')
    assert.equal((await boxOf(e, b)).x, 400, 'and keeps its left')
    // One Cmd+Z undoes the whole alignment.
    await press(e, 'ControlOrMeta+z', 400)
    assert.equal((await boxOf(e, b)).y, 180, 'one undo puts it back')
    await press(e, 'ControlOrMeta+Shift+z', 400)
    assert.equal((await boxOf(e, b)).y, 100, 'one redo again')
    // Home › Arrange has the same rows.
    await e.kit.getByTestId('ribbon-tab-home').click()
    await e.wait(200)
    await choose(e, 'slides-arrange', 'slides-arrange-align-bottom')
    const [ba, bb] = [await boxOf(e, a), await boxOf(e, b)]
    assert(near(ba.y + ba.h, bb.y + bb.h) && near(ba.y + ba.h, 200), `Home › Arrange › Align Bottom (${ba.y + ba.h}, ${bb.y + bb.h})`)
    assert.equal(await forms(e), 0, 'no form from Home either')
    assert.equal(await mode(e), 'line', 'the ribbon stays in Single line')
  }],
  ['g5-align-one-to-slide', async (e) => {
    const [a] = await fresh(e, [{ preset: 'rect', rect: [300, 200, 160, 100] }])
    await e.click(380, 250)
    await tabIs(e, 'shapeFormat', 'Shape Format for the shape')
    await e.kit.getByTestId('slides-align').first().click()
    await e.wait(200)
    assert.equal(await e.kit.getByTestId('slides-align-to-slide').getAttribute('aria-checked'), 'true', 'one object aligns to the slide')
    assert(await e.kit.getByTestId('slides-align-to-selection').isDisabled(), 'there is no selection to align to')
    await e.kit.getByTestId('slides-align-left').click()
    await e.wait(400)
    assert.equal((await boxOf(e, a)).x, 0, 'Align Left puts it on the slide\'s left edge')
    await choose(e, 'slides-align', 'slides-align-bottom')
    const box = await boxOf(e, a)
    assert(near(box.y + box.h, 540), `Align Bottom: on the slide's bottom edge (${box.y + box.h})`)
  }],
  ['g5-distribute', async (e) => {
    const ids = await fresh(e, [{ preset: 'rect', rect: [100, 200, 100, 80] }, { preset: 'rect', rect: [250, 200, 100, 80] }, { preset: 'rect', rect: [700, 200, 100, 80] }])
    await e.click(150, 240)
    await shiftClick(e, 300, 240)
    await shiftClick(e, 750, 240)
    assert.equal((await e.state()).selected, 3)
    await choose(e, 'slides-align', 'slides-distribute-horizontal')
    const boxes = await Promise.all(ids.map((id) => boxOf(e, id)))
    const gaps = [boxes[1].x - (boxes[0].x + boxes[0].w), boxes[2].x - (boxes[1].x + boxes[1].w)]
    assert(near(gaps[0], gaps[1]) && near(boxes[0].x, 100) && near(boxes[2].x, 700), `the middle one sits halfway (gaps ${gaps})`)
  }],
  ['w-a15-tab-switch', async (e) => {
    await fresh(e, [{ preset: 'rect', rect: [100, 100, 200, 120] }, { picture: true, rect: [500, 100, 160, 120] }])
    assert.equal(await activeTab(e), 'home')
    await e.click(200, 160)
    await tabIs(e, 'shapeFormat', 'selecting a shape shows Shape Format')
    await e.click(930, 520)
    await tabIs(e, 'home', 'clearing the selection goes back to Home')
    // From Insert: back to Insert.
    await e.kit.getByTestId('ribbon-tab-insert').click()
    await e.wait(200)
    await e.click(200, 160)
    await tabIs(e, 'shapeFormat', 'from Insert too')
    await e.click(580, 160)
    await tabIs(e, 'pictureFormat', 'selecting the picture shows Picture Format')
    await e.click(930, 520)
    await tabIs(e, 'insert', 'and clearing it goes back to the tab the person had')
    // A tab chosen while the shape stays selected stays.
    await e.click(200, 160)
    await tabIs(e, 'shapeFormat')
    await e.kit.getByTestId('ribbon-tab-design').click()
    await e.wait(300)
    assert.equal(await activeTab(e), 'design', 'the person picks Design with the shape selected')
    await e.click(930, 520)
    await tabIs(e, 'design', 'Design was the person\'s own choice')
    await e.kit.getByTestId('ribbon-tab-home').click()
    assert.equal(await mode(e), 'line')
  }],
  ['w-a12-group-keys', async (e) => {
    await fresh(e, [{ preset: 'rect', rect: [100, 100, 160, 100] }, { preset: 'ellipse', rect: [400, 100, 160, 100] }])
    await e.click(180, 150)
    await shiftClick(e, 480, 150)
    await press(e, 'ControlOrMeta+g', 500)
    let shapes = await shapesOn(e)
    assert.equal(shapes.length, 1, `Cmd+G groups the two (${shapes.map((s) => s.kind)})`)
    assert.equal(shapes[0].kind, 'group')
    await press(e, 'ControlOrMeta+Shift+g', 500)
    shapes = await shapesOn(e)
    assert.equal(shapes.length, 2, 'Cmd+Shift+G ungroups them')
    // The Group menu on Shape Format does the same.
    await choose(e, 'slides-group-menu', 'slides-group')
    assert.equal((await shapesOn(e)).length, 1, 'Shape Format › Group')
  }],
  ['w-p2-painter-sticky', async (e) => {
    const [a, b, c, d] = await fresh(e, [
      { preset: 'rect', rect: [80, 100, 140, 100] },
      { preset: 'rect', rect: [280, 100, 140, 100] },
      { preset: 'rect', rect: [480, 100, 140, 100] },
      { preset: 'rect', rect: [680, 100, 140, 100] },
    ])
    await e.agent([{ command: 'shape.fill', params: { ids: [a], color: '#FF0000' } }])
    const fill = async (id) => JSON.stringify((await e.ask('shape.inspect', { id }))?.shape?.fill)
    const red = await fill(a)
    const plain = await fill(d)
    await e.click(150, 150)
    await e.kit.getByTestId('ribbon-tab-home').click()
    await e.wait(200)
    const painter = e.kit.getByTestId('slides-cmd-format.painter')
    await painter.dblclick()
    await e.wait(300)
    assert.equal(await painter.getAttribute('aria-pressed'), 'true', 'a double click loads the painter')
    await e.click(350, 150)
    await e.click(550, 150)
    assert.equal(await fill(b), red, 'it paints the first shape clicked')
    assert.equal(await fill(c), red, 'and stays loaded for the next')
    await press(e, 'Escape', 400)
    await e.until(async () => (await painter.getAttribute('aria-pressed')) !== 'true', 20)
    assert.notEqual(await painter.getAttribute('aria-pressed'), 'true', 'Escape puts it down')
    await e.click(750, 150)
    assert.equal(await fill(d), plain, 'a click after Escape paints nothing')
    // A single click paints once.
    await e.click(150, 150)
    await e.kit.getByTestId('ribbon-tab-home').click()
    await e.wait(200)
    await painter.click()
    await e.wait(300)
    await e.click(750, 150)
    assert.equal(await fill(d), red, 'one click: one shape')
    await e.agent([{ command: 'shape.fill', params: { ids: [b], color: '#00FF00' } }])
    const green = await fill(b)
    await e.click(350, 150)
    assert.equal(await fill(b), green, 'and then it is put down')
  }],
  ['order-and-rotate', async (e) => {
    const [a, b] = await fresh(e, [{ preset: 'rect', rect: [100, 100, 200, 150] }, { preset: 'ellipse', rect: [200, 150, 200, 150] }])
    await e.click(130, 120)
    await tabIs(e, 'shapeFormat')
    await e.kit.getByTestId('slides-bring-forward').first().click()
    await e.wait(400)
    let order = (await shapesOn(e)).map((shape) => shape.id)
    assert.deepEqual(order, [b, a], 'Bring Forward puts the rectangle over the oval')
    assert.equal(await forms(e), 0, 'with no form')
    await e.kit.getByTestId('slides-send-backward').first().click()
    await e.wait(400)
    order = (await shapesOn(e)).map((shape) => shape.id)
    assert.deepEqual(order, [a, b], 'Send Backward puts it back')
    await choose(e, 'slides-rotate', 'slides-rotate-right')
    assert.equal((await boxOf(e, a)).rot, 90, 'Rotate Right 90°')
    await choose(e, 'slides-rotate', 'slides-rotate-left')
    assert.equal((await boxOf(e, a)).rot, 0, 'Rotate Left 90°')
  }],
  ['size-fields', async (e) => {
    const [a] = await fresh(e, [{ preset: 'rect', rect: [100, 100, 200, 100] }])
    await e.click(200, 150)
    await tabIs(e, 'shapeFormat')
    const width = e.kit.getByTestId('slides-size-width')
    const height = e.kit.getByTestId('slides-size-height')
    await e.until(async () => (await width.inputValue()).startsWith('7.06'), 30)
    assert.match(await width.inputValue(), /^7\.06 /, 'the width shows in centimetres')
    await width.click()
    await e.page.keyboard.type('5')
    await press(e, 'Enter', 400)
    assert(near((await boxOf(e, a)).w, 5 * CM, 0.01), `typing 5 makes it 5 cm wide (${(await boxOf(e, a)).w})`)
    assert.equal((await boxOf(e, a)).h, 100, 'the height stays')
    // Enter gave the keyboard back: an arrow key nudges the shape.
    const x = (await boxOf(e, a)).x
    await press(e, 'ArrowRight')
    assert.notEqual((await boxOf(e, a)).x, x, 'after Enter the keys go to the slide again')
    await height.click()
    await e.page.keyboard.type('150 pt')
    await press(e, 'Enter', 400)
    assert.equal((await boxOf(e, a)).h, 150, 'a value with its unit: 150 pt')
    await e.kit.getByTestId('slides-size-lock').click()
    await width.click()
    await e.page.keyboard.type('10')
    await press(e, 'Enter', 400)
    const locked = await boxOf(e, a)
    assert(near(locked.w, 10 * CM, 0.01) && near(locked.h, 150 * 2, 0.01), `with the ratio locked the height follows (${locked.w} × ${locked.h})`)
    // Rotation is in the group's panel in Single line.
    const rotation = e.kit.getByTestId('slides-size-rotation')
    if (!(await rotation.isVisible())) {
      await e.kit.getByTestId('kui-line-more-Size').click()
      await e.wait(200)
    }
    await rotation.click()
    await e.page.keyboard.type('30')
    await press(e, 'Enter', 400)
    assert.equal((await boxOf(e, a)).rot, 30, 'typing 30 turns it 30°')
    // Escape puts the shown value back.
    await width.click()
    await e.page.keyboard.type('2')
    await press(e, 'Escape', 300)
    assert(near((await boxOf(e, a)).w, 10 * CM, 0.01), `Escape leaves the width as it was (${JSON.stringify(await boxOf(e, a))})`)
    assert.equal((await e.state()).selected, 1, 'and the selection too')
  }],
  ['g2-fill-colors', async (e) => {
    const [a] = await fresh(e, [{ preset: 'rect', rect: [100, 100, 200, 100] }])
    const fill = async () => JSON.stringify((await e.ask('shape.inspect', { id: a }))?.shape?.fill ?? null)
    await e.click(200, 150)
    await tabIs(e, 'shapeFormat')
    await e.kit.getByTestId('slides-shape-fill').first().click()
    await e.wait(300)
    const accent = e.kit.getByTestId('slides-fills').locator('[data-color="accent2"]')
    const shown = await accent.evaluate((element) => getComputedStyle(element).backgroundImage === 'none' && getComputedStyle(element).backgroundColor !== 'rgba(0, 0, 0, 0)')
    assert(shown, 'a theme color shows its color')
    await accent.click()
    await e.wait(400)
    assert.match(await fill(), /accent2/i, `Accent 2 fills it with the theme's color (${await fill()})`)
    await e.kit.getByTestId('slides-shape-fill').first().click()
    await e.wait(200)
    await e.kit.getByTestId('slides-fills').locator('[data-color="accent1-lighter40"]').click()
    await e.wait(400)
    assert.match(await fill(), /lumMod|lum_mod|60000/i, `a lighter tint keeps the theme color with its tint (${await fill()})`)
    await e.kit.getByTestId('slides-shape-fill').first().click()
    await e.wait(200)
    await e.kit.getByTestId('slides-fills-hex').click()
    await e.page.keyboard.type('336699')
    await press(e, 'Enter', 400)
    const typed = JSON.parse(await fill())?.color?.base?.rgb
    assert.deepEqual(typed && [typed.r, typed.g, typed.b], [0x33, 0x66, 0x99], `a typed code fills it (${await fill()})`)
    assert.equal(await e.kit.locator('.kui-popover').count(), 0, 'and the menu closes')
    await e.kit.getByTestId('slides-shape-outline').first().click()
    await e.wait(200)
    await e.kit.getByTestId('slides-shape-outline-weight-3').click()
    await e.wait(400)
    assert.match(JSON.stringify((await e.ask('shape.inspect', { id: a }))?.shape?.line ?? null), /"w(idth)?":\s*3\b/, 'Shape Outline › 3 pt')
  }],
  ['g8-mode-step-down', async (e) => {
    await fresh(e, [{ preset: 'rect', rect: [100, 100, 200, 100] }, { picture: true, rect: [500, 100, 160, 120] }])
    for (const [x, y, id] of [[200, 150, 'shapeFormat'], [580, 160, 'pictureFormat']]) {
      await e.click(x, y)
      await tabIs(e, id)
      const fits = await e.frame().evaluate(() => {
        const row = document.querySelector('.kui-line-panel') ?? document.querySelector('[role="tabpanel"]')
        return row ? row.scrollWidth <= row.clientWidth + 1 : false
      })
      assert(fits, `${id} fits a 1600 px window in Single line`)
      await e.kit.getByTestId('slides-bring-forward').first().click()
      await e.wait(500)
      assert.equal(await mode(e), 'line', `a command on ${id} keeps Single line`)
    }
    await e.click(930, 520)
  }],
  ['g11-zh-format-tab', async (e) => {
    void e
    const zh = await open({ which: e.which, query: 'locale=zh-CN' })
    try {
      await fresh(zh, [{ preset: 'rect', rect: [100, 100, 200, 100] }, { picture: true, rect: [500, 100, 160, 120] }])
      const english = async () => {
        const names = await zh.frame().evaluate(() =>
          [...document.querySelectorAll('[role="tabpanel"] button, [role="tabpanel"] input, [role="tabpanel"] label, .kui-popover button, .kui-popover .slides-menu-heading')]
            .filter((element) => element.getClientRects().length > 0)
            .map((element) => (element.getAttribute('aria-label') || element.getAttribute('title') || element.textContent || '').trim())
            .filter(Boolean),
        )
        return names.filter((name) => /[A-Za-z]{3,}/.test(name) && !/[一-鿿]/.test(name) && !/^#[0-9A-F]{6}$/i.test(name) && name !== '#RRGGBB')
      }
      for (const [x, y, id, menus] of [[200, 150, 'shapeFormat', ['slides-align', 'slides-shape-fill', 'slides-shape-outline', 'slides-rotate']], [580, 160, 'pictureFormat', ['slides-align', 'slides-picture-corrections', 'slides-picture-border']]]) {
        await zh.click(x, y)
        await tabIs(zh, id)
        assert.deepEqual(await english(), [], `${id}: no English button names`)
        for (const menu of menus) {
          await zh.kit.getByTestId(menu).first().click()
          await zh.wait(250)
          assert.deepEqual(await english(), [], `${id} › ${menu}: no English rows`)
          await zh.page.keyboard.press('Escape')
          await zh.wait(200)
        }
      }
      await zh.kit.getByTestId('ribbon-tab-home').click()
      await zh.kit.getByTestId('slides-arrange').first().click()
      await zh.wait(250)
      assert.deepEqual(await english(), [], 'Home › Arrange: no English rows')
      await zh.page.keyboard.press('Escape')
      assert.deepEqual(zh.errors, [])
    } finally {
      await zh.close()
    }
  }],
]

await segments('format', list, (which) => open({ which }))
