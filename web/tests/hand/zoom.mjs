// Zooming as WPS does (plan 0A; walkthrough A2, A3, A5, A7, A9; WPS W-L1, W-L2, W-L4): a mouse
// wheel notch with Ctrl or Cmd goes to the next multiple of 10% at least 10 points away (132% →
// 150%), the buttons and Cmd+− / Cmd+= 10% at a time, every Fit fits the slide as it opens, Cmd+1
// is 100%, the percentage takes a typed value, and zoom stays within 10%–400%. A trackpad swipe
// turns one page however long it is.
// node tests/hand/zoom.mjs [chromium|webkit] [segment…]
import assert from 'node:assert/strict'
import { open, segments } from './harness.mjs'

/** The fitted zoom the editor opened at (1600 × 1000: 132%). */
let fitted = 0

async function settled(e, value) {
  await e.until(async () => (await e.zoom()) === value, 30)
  return e.zoom()
}

async function wheelAtCentre(e, modifier, deltaY) {
  const box = await e.surface()
  await e.page.mouse.move(box.x + box.width / 2, box.y + box.height / 2)
  await e.page.keyboard.down(modifier)
  await e.page.mouse.wheel(0, deltaY)
  await e.page.keyboard.up(modifier)
  await e.wait(250)
}

async function fit(e) {
  await e.kit.getByTestId('slides-zoom-fit').click()
  await settled(e, fitted)
}

const list = [
  ['a2-fit-same', async (e) => {
    fitted = await e.zoom()
    assert.equal(fitted, 132, `the slide opens fitted at 132% (${fitted}%)`)
    await e.kit.getByTestId('slides-zoom-in').click()
    await e.kit.getByTestId('slides-zoom-fit').click()
    assert.equal(await settled(e, fitted), fitted, 'the status bar\'s Fit gives the zoom the slide opened at')
    await e.kit.getByTestId('slides-zoom-in').click()
    await e.page.keyboard.press('ControlOrMeta+0')
    assert.equal(await settled(e, fitted), fitted, 'Cmd+0 gives the same')
    await e.kit.getByTestId('slides-zoom-in').click()
    await e.kit.getByTestId('ribbon-tab-view').click()
    await e.kit.getByTestId('slides-view-fit').click()
    assert.equal(await settled(e, fitted), fitted, 'View › Fit gives the same')
    await e.kit.getByTestId('ribbon-tab-home').click()
  }],
  ['a3-wheel-zoom-step', async (e) => {
    fitted ||= await e.zoom()
    await fit(e)
    await wheelAtCentre(e, 'Control', -100)
    assert.equal(await settled(e, 150), 150, 'Ctrl and one wheel notch: 132% → 150%')
    await wheelAtCentre(e, 'Control', -100)
    assert.equal(await settled(e, 160), 160, 'and the next notch 160%')
    await wheelAtCentre(e, 'Control', 100)
    assert.equal(await settled(e, 150), 150, 'a notch back 150%')
    await fit(e)
    await wheelAtCentre(e, 'Meta', -100)
    assert.equal(await settled(e, 150), 150, 'Cmd and one wheel notch: the same')
  }],
  ['a3-steps', async (e) => {
    fitted ||= await e.zoom()
    await fit(e)
    await e.kit.getByTestId('slides-zoom-out').click()
    assert.equal(await settled(e, 130), 130, '− from 132%: 130%')
    await e.kit.getByTestId('slides-zoom-out').click()
    assert.equal(await settled(e, 120), 120, '− again: 120%')
    await e.page.keyboard.press('ControlOrMeta+Minus')
    assert.equal(await settled(e, 110), 110, 'Cmd+−: 110%')
    await e.page.keyboard.press('ControlOrMeta+Equal')
    assert.equal(await settled(e, 120), 120, 'Cmd+=: 120%')
    await e.kit.getByTestId('slides-zoom-in').click()
    assert.equal(await settled(e, 130), 130, '+: 130%')
    await e.page.keyboard.press('ControlOrMeta+1')
    assert.equal(await settled(e, 100), 100, 'Cmd+1: 100%')
  }],
  ['a7-type-percent', async (e) => {
    await e.kit.getByTestId('kit-zoom').click()
    const field = e.kit.getByTestId('slides-zoom-field')
    await field.fill('250')
    await e.page.keyboard.press('Enter')
    assert.equal(await settled(e, 250), 250, 'typing 250 and Enter: 250%')
    assert.equal(await e.active(), 'div.kui-app', 'the editor has the keyboard again')
    await e.kit.getByTestId('kit-zoom').click()
    await e.kit.getByTestId('slides-zoom-field').fill('80')
    await e.page.keyboard.press('Escape')
    await e.wait(250)
    assert.equal(await e.zoom(), 250, 'Escape keeps the zoom')
  }],
  ['a5-limits', async (e) => {
    for (let index = 0; index < 30; index += 1) await wheelAtCentre(e, 'Control', 100)
    assert.equal(await settled(e, 10), 10, 'zooming out stops at 10%')
    for (let index = 0; index < 40; index += 1) await wheelAtCentre(e, 'Control', -100)
    assert.equal(await settled(e, 400), 400, 'zooming in stops at 400%')
    await e.kit.getByTestId('kit-zoom').click()
    await e.kit.getByTestId('slides-zoom-field').fill('5')
    await e.page.keyboard.press('Enter')
    assert.equal(await settled(e, 10), 10, 'a typed 5% is 10%')
    await e.page.keyboard.press('ControlOrMeta+Minus')
    await e.wait(250)
    assert.equal(await e.zoom(), 10, 'Cmd+− stays at 10%')
  }],
  ['a9-swipe-pages-once', async (e) => {
    fitted ||= await e.zoom()
    await fit(e)
    await e.agent([{ command: 'slide.new', params: { at: 2 } }, { command: 'slide.new', params: { at: 3 } }, { command: 'slide.new', params: { at: 4 } }])
    await e.goto(0)
    const box = await e.surface()
    await e.page.mouse.move(box.x + box.width / 2, box.y + box.height / 2)
    // A slow swipe: 40 small steps.
    for (let index = 0; index < 40; index += 1) {
      await e.page.mouse.wheel(0, 12)
      await e.wait(16)
    }
    await e.wait(400)
    assert.equal((await e.state()).slide, 1, 'a slow swipe turns one page')
    // A flick with momentum: 90 events falling from 40.
    for (let index = 0; index < 90; index += 1) {
      await e.page.mouse.wheel(0, Math.max(1, Math.round(40 * 0.95 ** index)))
      await e.wait(12)
    }
    await e.wait(400)
    assert.equal((await e.state()).slide, 2, 'a flick turns one page')
  }],
]

await segments('zoom', list, (which) => open({ which }))
