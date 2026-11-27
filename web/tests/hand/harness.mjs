// What the hand tests share (web/tests/hand/*.mjs): a Vite server and one browser on dev.html, a
// person's input (page.mouse, page.keyboard), and reads that are no input of a person (the engine's
// state through the development host's agent connection, the overlays drawn on the canvas).
// A test file runs its segments in order; `node tests/hand/<file>.mjs <chromium|webkit> [segment…]`
// runs only the named ones (a regression case names its segment).
import { chromium, webkit } from 'playwright'
import { createServer } from 'vite'

/**
 * Init script for every frame: records the paths drawn on the editor's canvas surface (the slide is
 * one drawImage, the engine's overlays the paths after it), so a test reads how a handle looks
 * without reading pixels. `window.__hand.frame` is the last frame's operations.
 */
const RECORD_OVERLAYS = () => {
  if (window.__TRACE_KEYS !== undefined) return
  window.__TRACE_KEYS = []
  document.addEventListener('keydown', (event) => window.__TRACE_KEYS.push(`${event.key}@${document.activeElement?.className?.toString().split(' ')[0] || document.activeElement?.tagName}`), true)
  window.addEventListener('keydown', (event) => window.__TRACE_KEYS.push(`${event.key}:${event.defaultPrevented ? 'taken' : 'free'}`))
  const P = window.CanvasRenderingContext2D?.prototype
  if (!P) return
  const state = (window.__hand = { frame: [], current: [] })
  const surface = (ctx) => ctx.canvas?.classList?.contains('kui-canvas-surface')
  const wrap = (name, record) => {
    const original = P[name]
    if (typeof original !== 'function') return
    P[name] = function (...args) {
      try {
        if (surface(this)) record(this, args)
      } catch {}
      return original.apply(this, args)
    }
  }
  const at = (ctx, x, y) => {
    const m = ctx.getTransform()
    const d = window.devicePixelRatio || 1
    return [(m.a * x + m.c * y + m.e) / d, (m.b * x + m.d * y + m.f) / d]
  }
  wrap('drawImage', () => {
    state.frame = state.current
    state.current = []
  })
  wrap('putImageData', () => {
    state.frame = state.current
    state.current = []
  })
  wrap('beginPath', () => state.current.push({ op: 'begin' }))
  wrap('arc', (ctx, a) => state.current.push({ op: 'arc', at: at(ctx, a[0], a[1]), r: a[2] }))
  wrap('moveTo', (ctx, a) => state.current.push({ op: 'move', at: at(ctx, a[0], a[1]) }))
  wrap('lineTo', (ctx, a) => state.current.push({ op: 'line', at: at(ctx, a[0], a[1]) }))
  wrap('rect', (ctx, a) => state.current.push({ op: 'rect', at: at(ctx, a[0], a[1]), w: a[2], h: a[3] }))
  wrap('fill', (ctx) => state.current.push({ op: 'fill', style: String(ctx.fillStyle) }))
  wrap('stroke', (ctx) => state.current.push({ op: 'stroke', style: String(ctx.strokeStyle), width: ctx.lineWidth }))
  wrap('fillRect', (ctx, a) => state.current.push({ op: 'fillRect', at: at(ctx, a[0], a[1]), w: a[2], h: a[3], style: String(ctx.fillStyle) }))
  wrap('strokeRect', (ctx, a) => state.current.push({ op: 'strokeRect', at: at(ctx, a[0], a[1]), w: a[2], h: a[3], style: String(ctx.strokeStyle) }))
}

/** Opens dev.html (the sample slides) in `which` browser at `width` × `height`. */
export async function open({ which = process.argv[2] ?? 'chromium', query = 'locale=en-US', width = 1600, height = 1000, dpr = 1 } = {}) {
  const server = await createServer({ configFile: new URL('../../vite.config.ts', import.meta.url).pathname, server: { port: 0, host: '127.0.0.1' }, logLevel: 'error' })
  await server.listen()
  const browser = await (which === 'webkit' ? webkit : chromium).launch()
  const context = await browser.newContext({ viewport: { width, height }, deviceScaleFactor: dpr })
  await context.addInitScript(RECORD_OVERLAYS)
  const page = await context.newPage()
  page.setDefaultTimeout(10_000)
  const errors = []
  page.on('pageerror', (error) => errors.push(error.message))
  await page.goto(`http://127.0.0.1:${server.httpServer.address().port}/dev.html${query ? `?${query}` : ''}`)
  await page.waitForFunction(() => document.getElementById('status')?.textContent?.includes(' open in '), null, { timeout: 90_000 })
  const kit = page.frameLocator('#kit')
  const frame = () => page.frames().find((item) => item !== page.mainFrame())
  const wait = (ms) => page.waitForTimeout(ms)
  const until = async (check, tries = 60, gap = 100) => {
    for (let index = 0; index < tries; index += 1) {
      if (await check().catch(() => false)) return true
      await page.waitForTimeout(gap)
    }
    return false
  }
  await until(() => kit.getByTestId('kit-canvas-surface').isVisible())
  // The commands are known (their shortcuts work) once the ribbon's command buttons are enabled.
  await until(() => kit.getByTestId('slides-cmd-slide.new').isEnabled(), 100)
  /** Runs commands as the agent: setup, never a person's action. */
  const agent = (commands) => page.evaluate(async (list) => (await window.devHost.connection().ai.run(list, false)).completed?.map((item) => item.result) ?? [], commands)
  const ask = async (command, params = {}) => (await agent([{ command, params }]))[0]
  const inspected = async () => {
    const info = await page.evaluate(async () => window.devHost.connection().ai.inspect())
    return info.details ?? info.inspected ?? info
  }
  /** The engine's state in short: current slide (from 0), slide count, selection count, text edited. */
  const state = async () => {
    const info = await inspected()
    return { slide: info.slide, slides: info.slides?.length, selected: info.selection, editing: info.editingText === true }
  }
  const zoom = async () => Number((await kit.getByTestId('kit-zoom').textContent()).replace('%', ''))
  const surface = () => kit.getByTestId('kit-canvas-surface').boundingBox()
  /**
   * Slide point (`x`, `y`) on the page, while the slide is fitted (centred in the canvas) or zoomed
   * about the centre: the slide is 960 × 540 points.
   */
  const toScreen = async (x, y) => {
    const box = await surface()
    const scale = (await zoom()) / 100
    return { x: box.x + box.width / 2 + (x - 480) * scale, y: box.y + box.height / 2 + (y - 270) * scale }
  }
  /** Goes to slide `index` (from 0) by its thumbnail, as a person does. */
  const goto = async (index) => {
    await kit.getByTestId(`slides-slide-${index}`).click()
    await until(async () => (await state()).slide === index)
    await page.waitForTimeout(250)
  }
  /** Clicks slide point (`x`, `y`) with the mouse. */
  const click = async (x, y, options = {}) => {
    const at = await toScreen(x, y)
    await page.mouse.click(at.x, at.y, options)
    await page.waitForTimeout(250)
  }
  /** The element with the keyboard in the editor's frame, in short. */
  const active = () => frame().evaluate(() => {
    const element = document.activeElement
    if (!element || element === document.body) return 'body'
    return `${element.tagName.toLowerCase()}${element.className && typeof element.className === 'string' ? `.${element.className.split(' ')[0]}` : ''}`
  })
  /** The operations of the last frame drawn over the slide. */
  const drawn = () => frame().evaluate(() => window.__hand?.frame ?? [])
  const close = async () => {
    await browser.close().catch(() => undefined)
    await server.close().catch(() => undefined)
  }
  return { which, page, kit, frame, wait, until, agent, ask, inspected, state, zoom, surface, toScreen, click, goto, active, drawn, errors, close }
}

/**
 * Runs a test file's segments, each `[name, fn]`, in order: only those named on the command line
 * when any are. Prints PASS or FAIL per segment and exits non-zero on a failure.
 */
export async function segments(title, list, start) {
  const which = process.argv[2] ?? 'chromium'
  const wanted = process.argv.slice(3)
  const chosen = wanted.length ? list.filter(([name]) => wanted.includes(name)) : list
  if (wanted.length && chosen.length !== wanted.length) {
    console.log(`FAIL unknown segment among ${wanted.join(', ')}`)
    process.exit(1)
  }
  const editor = await start(which)
  let failed = 0
  try {
    for (const [name, fn] of chosen) {
      try {
        await fn(editor)
        console.log(`PASS ${title} ${name} (${which})`)
      } catch (error) {
        failed += 1
        console.log(`FAIL ${title} ${name} (${which}): ${error instanceof Error ? error.message.split('\n').slice(0, 4).join(' | ') : String(error)}`)
      }
    }
    if (editor.errors.length) {
      failed += 1
      console.log(`FAIL page errors: ${editor.errors.slice(0, 5).join(' | ')}`)
    }
  } finally {
    await editor.close()
  }
  process.exit(failed ? 1 : 0)
}
