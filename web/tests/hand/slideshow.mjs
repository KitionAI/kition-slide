// The slide show by hand (plan stage 11; walkthrough L2, L3, L4; WPS W-M2 to W-M6): N and P, a
// typed number and Return, B and W, Home and End, the wheel and the right-click menu move through
// the show; after the last slide a black end screen, and one more step ends it; the editor is then
// on the slide shown last with the keyboard; the pointer and a control bar at the bottom left show
// while the pointer moves and hide after it rested. A person's mouse and keys only; the show's
// slide is read from its element, the editor's from the engine.
// node tests/hand/slideshow.mjs [chromium|webkit] [segment…]
import assert from 'node:assert/strict'
import { open, segments } from './harness.mjs'

const press = async (e, key, ms = 250) => {
  await e.page.keyboard.press(key)
  await e.wait(ms)
}
const show = (e) => e.kit.getByTestId('slides-show')
const showing = (e) => show(e).isVisible().catch(() => false)
/** The slide the show shows (from 0), or 'end' on the end screen, or null with no show. */
const shown = async (e) => {
  if (!(await showing(e))) return null
  const at = await show(e).getAttribute('data-slide')
  return at === 'end' ? 'end' : Number(at)
}
const screen = async (e) => (await show(e).getAttribute('data-screen')) ?? null
const barShown = async (e) => (await e.kit.getByTestId('slides-show-controls').getAttribute('data-shown')) === 'true'
const cursor = (e) => e.frame().evaluate(() => getComputedStyle(document.querySelector('[data-testid=slides-show]')).cursor)

/** Clicks the pasteboard beside the slide (nothing there): nothing selected, the editor has the keyboard. */
async function clickBeside(e) {
  const box = await e.surface()
  await e.page.mouse.click(box.x + 8, box.y + box.height / 2)
  await e.wait(300)
  await e.until(async () => (await e.state()).selected === 0)
}

let prepared = false
/** Six slides; the person on slide `at`, nothing selected. */
async function prepare(e, at = 0) {
  // A segment that failed may have left its show open.
  if (await showing(e)) await press(e, 'Escape', 600)
  if (!prepared) {
    const have = (await e.state()).slides
    await e.agent(Array.from({ length: 6 - have }, (_, k) => ({ command: 'slide.new', params: { at: have + k, layout: 'titleOnly', title: `Slide ${have + k + 1}` } })))
    await e.until(async () => (await e.kit.locator('.slides-slide').count()) === 6)
    prepared = true
  }
  await e.goto(at)
  await clickBeside(e)
}

/** Starts the show with `key` (F5 from the first slide, Shift+F5 from the current one). */
async function start(e, key = 'F5') {
  await press(e, key, 700)
  assert(await e.until(() => showing(e)), `${key} starts the show`)
}

/** The middle of the show's stage on the page. */
async function middle(e) {
  const box = await show(e).boundingBox()
  return { x: box.x + box.width / 2, y: box.y + box.height / 2, box }
}

const list = [
  ['l2-keys', async (e) => {
    await prepare(e)
    await start(e)
    assert.equal(await shown(e), 0, 'F5 starts on the first slide')
    await press(e, 'n')
    assert.equal(await shown(e), 1, 'N goes on')
    await press(e, 'p')
    assert.equal(await shown(e), 0, 'P goes back')
    for (const [key, at] of [['Space', 1], ['ArrowRight', 2], ['ArrowDown', 3], ['Enter', 4], ['PageDown', 5]]) {
      await press(e, key)
      assert.equal(await shown(e), at, `${key} goes on`)
    }
    for (const [key, at] of [['ArrowLeft', 4], ['ArrowUp', 3], ['Backspace', 2], ['PageUp', 1]]) {
      await press(e, key)
      assert.equal(await shown(e), at, `${key} goes back`)
    }
    await press(e, 'End')
    assert.equal(await shown(e), 5, 'End goes to the last slide')
    await press(e, 'Home')
    assert.equal(await shown(e), 0, 'Home goes to the first slide')
    // A typed number and Return go to that slide (W-M3); a number past the deck goes nowhere.
    await press(e, '4', 150)
    assert.match(await e.kit.getByTestId('slides-show-typed').textContent(), /4/, 'the typed number shows')
    await press(e, 'Enter')
    assert.equal(await shown(e), 3, '4 and Return go to slide 4')
    await press(e, '9', 100)
    await press(e, 'Enter')
    assert.equal(await shown(e), 3, '9 and Return (past the deck) stay')
    await press(e, '1', 100)
    await press(e, '2', 100)
    await press(e, 'Backspace', 100)
    await press(e, 'Enter')
    assert.equal(await shown(e), 0, 'Backspace takes back a typed digit: 1 and Return')
    await press(e, '3', 100)
    await press(e, 'Escape')
    assert(await showing(e), 'Escape takes back a typed number first, the show stays')
    assert.equal(await e.kit.getByTestId('slides-show-typed').count(), 0, 'the typed number is gone')
    await press(e, 'Enter')
    assert.equal(await shown(e), 1, 'Return then goes on')
    // B and W black and white the screen, again brings the slide back (W-M3).
    await press(e, 'b')
    assert.equal(await screen(e), 'black', 'B blacks the screen')
    await press(e, 'b')
    assert.equal(await screen(e), null, 'B again brings the slide back')
    await press(e, 'w')
    assert.equal(await screen(e), 'white', 'W whites the screen')
    await press(e, 'b')
    assert.equal(await screen(e), 'black', 'B on the white screen blacks it')
    await press(e, 'ArrowRight')
    assert.equal(await screen(e), null, 'a step brings the slide back')
    assert.equal(await shown(e), 1, 'without going on')
    // The editor's keys stay behind the show.
    const slides = (await e.state()).slides
    await press(e, 'ControlOrMeta+d')
    assert.equal((await e.state()).slides, slides, 'Cmd+D in the show duplicates nothing')
    await press(e, 'Escape', 600)
    assert(!(await showing(e)), 'Escape ends the show')
  }],
  ['l2-wheel', async (e) => {
    await prepare(e)
    await start(e)
    const at = await middle(e)
    await e.page.mouse.move(at.x, at.y)
    await e.page.mouse.wheel(0, 100)
    await e.wait(500)
    assert.equal(await shown(e), 1, 'a wheel notch down goes on')
    await e.page.mouse.wheel(0, 100)
    await e.wait(500)
    assert.equal(await shown(e), 2, 'and another')
    await e.page.mouse.wheel(0, -100)
    await e.wait(500)
    assert.equal(await shown(e), 1, 'a notch up goes back')
    await press(e, 'End')
    await e.page.mouse.wheel(0, 100)
    await e.wait(500)
    assert.equal(await shown(e), 'end', 'past the last slide the end screen')
    await e.page.mouse.wheel(0, 100)
    await e.wait(500)
    assert.equal(await shown(e), 'end', 'the wheel does not leave the end screen')
    await press(e, 'Escape', 600)
  }],
  ['l2-menu', async (e) => {
    await prepare(e)
    await start(e)
    const menu = e.kit.getByTestId('slides-show-menu')
    const at = await middle(e)
    await e.page.mouse.click(at.x, at.y, { button: 'right' })
    await e.wait(300)
    assert(await menu.isVisible(), 'a right-click opens the show\'s menu')
    assert.equal(await shown(e), 0, 'and does not go back or on')
    for (const id of ['next', 'previous', 'first', 'last', 'goTo', 'screen', 'end']) assert(await e.kit.getByTestId(`slides-show-cm-${id}`).isVisible(), `the menu has ${id}`)
    await e.kit.getByTestId('slides-show-cm-next').click()
    await e.wait(300)
    assert.equal(await shown(e), 1, 'Next goes on')
    assert(!(await menu.isVisible().catch(() => false)), 'and closes the menu')
    assert(await showing(e), 'the show goes on')
    // Go to Slide ▸ lists the slides.
    await e.page.mouse.click(at.x, at.y, { button: 'right' })
    await e.wait(300)
    await e.kit.getByTestId('slides-show-cm-goTo').hover()
    await e.wait(200)
    await e.kit.getByTestId('slides-show-cm-goTo-4').click()
    await e.wait(300)
    assert.equal(await shown(e), 4, 'Go to Slide ▸ 5 goes to slide 5')
    // A press beside the menu closes it and does not go on.
    await e.page.mouse.click(at.x, at.y, { button: 'right' })
    await e.wait(300)
    await e.page.mouse.click(at.x - 200, at.y - 150)
    await e.wait(300)
    assert(!(await menu.isVisible().catch(() => false)), 'a click beside the menu closes it')
    assert.equal(await shown(e), 4, 'and does not go on')
    // Keys in the menu are the menu's.
    await e.page.mouse.click(at.x, at.y, { button: 'right' })
    await e.wait(300)
    await press(e, 'ArrowDown')
    assert.equal(await shown(e), 4, 'Down in the menu moves in the menu, not the show')
    await press(e, 'Escape')
    assert(!(await menu.isVisible().catch(() => false)), 'Escape closes the menu')
    assert(await showing(e), 'and not the show')
    // Screen ▸ Black Screen.
    await e.page.mouse.click(at.x, at.y, { button: 'right' })
    await e.wait(300)
    await e.kit.getByTestId('slides-show-cm-screen').hover()
    await e.wait(200)
    await e.kit.getByTestId('slides-show-cm-black').click()
    await e.wait(300)
    assert.equal(await screen(e), 'black', 'Screen ▸ Black Screen blacks it')
    await e.page.mouse.click(at.x, at.y)
    await e.wait(300)
    assert.equal(await screen(e), null, 'a click brings the slide back')
    assert.equal(await shown(e), 4, 'without going on')
    // End Show: the editor is on the slide shown.
    await e.page.mouse.click(at.x, at.y, { button: 'right' })
    await e.wait(300)
    await e.kit.getByTestId('slides-show-cm-end').click()
    await e.wait(600)
    assert(!(await showing(e)), 'End Show ends it')
    assert(await e.until(async () => (await e.state()).slide === 4), 'the editor is on the slide shown last')
  }],
  ['l3-show-exit-slide', async (e) => {
    // From the current slide (2): two steps on and Escape leave the editor on slide 4.
    await prepare(e, 1)
    await start(e, 'Shift+F5')
    assert.equal(await shown(e), 1, 'Shift+F5 starts on the current slide')
    await press(e, 'ArrowRight')
    await press(e, 'ArrowRight')
    assert.equal(await shown(e), 3)
    await press(e, 'Escape', 600)
    assert(!(await showing(e)), 'Escape ends the show')
    assert(await e.until(async () => (await e.state()).slide === 3), `the editor is on the slide shown last (${(await e.state()).slide})`)
    assert.equal(await e.active(), 'div.kui-app', 'the editor has the keyboard')
    await press(e, 'PageDown', 400)
    assert.equal((await e.state()).slide, 4, 'PageDown works right after the show')
    // After the last slide: the black end screen; back returns to the last slide; one more step ends it.
    await start(e)
    await press(e, 'End')
    await press(e, 'ArrowRight')
    assert.equal(await shown(e), 'end', 'a step after the last slide shows the end screen')
    assert(await e.kit.getByTestId('slides-show-ended').isVisible(), 'which says the show has ended')
    await press(e, 'ArrowLeft')
    assert.equal(await shown(e), 5, 'back from the end screen is the last slide')
    await press(e, 'Space')
    assert.equal(await shown(e), 'end')
    await press(e, 'Space', 600)
    assert(!(await showing(e)), 'one more step ends the show')
    assert(await e.until(async () => (await e.state()).slide === 5), 'the editor is on the last slide')
    assert.equal(await e.active(), 'div.kui-app', 'the editor has the keyboard')
    // A click on the end screen ends it too.
    await clickBeside(e)
    await start(e)
    await press(e, 'End')
    await press(e, 'n')
    const at = await middle(e)
    await e.page.mouse.click(at.x, at.y)
    await e.wait(600)
    assert(!(await showing(e)), 'a click on the end screen ends the show')
  }],
  ['l4-controls', async (e) => {
    await prepare(e)
    await start(e)
    assert(!(await barShown(e)), 'the control bar is hidden as the show starts')
    assert.equal(await cursor(e), 'none', 'and so is the pointer')
    const at = await middle(e)
    await e.page.mouse.move(at.x, at.y)
    await e.page.mouse.move(at.x + 40, at.y + 20, { steps: 5 })
    await e.wait(200)
    assert(await barShown(e), 'moving the pointer shows the control bar')
    assert.notEqual(await cursor(e), 'none', 'and the pointer')
    const bar = await e.kit.getByTestId('slides-show-controls').boundingBox()
    assert(bar.x - at.box.x < at.box.width / 4 && at.box.y + at.box.height - (bar.y + bar.height) < at.box.height / 4, 'the bar is at the bottom left')
    await e.wait(2900)
    assert(!(await barShown(e)), 'the bar hides after the pointer rested 2.5 s')
    assert.equal(await cursor(e), 'none', 'and the pointer')
    // On the bar: it stays while the pointer rests there; its buttons go back, on and end.
    const next = await e.kit.getByTestId('slides-show-next').boundingBox()
    await e.page.mouse.move(next.x + next.width / 2, next.y + next.height / 2, { steps: 6 })
    await e.wait(2900)
    assert(await barShown(e), 'the bar stays while the pointer rests on it')
    await e.page.mouse.click(next.x + next.width / 2, next.y + next.height / 2)
    await e.wait(300)
    assert.equal(await shown(e), 1, 'its Next goes on')
    await e.kit.getByTestId('slides-show-previous').click()
    await e.wait(300)
    assert.equal(await shown(e), 0, 'its Previous goes back')
    await e.kit.getByTestId('slides-show-next').click()
    await e.kit.getByTestId('slides-show-end').click()
    await e.wait(600)
    assert(!(await showing(e)), 'its End Show ends the show')
    assert(await e.until(async () => (await e.state()).slide === 1), 'the editor is on the slide shown last')
  }],
]

await segments('slideshow', list, (which) => open({ which }))
