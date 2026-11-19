// The canvas around the slide follows the host's theme, live: node tests/backdrop.mjs [chromium|webkit].
// The engine paints that pasteboard itself (`setBackdrop`), so the editor must hand it the theme's
// `--kit-canvas`. Kition sends its tokens as `hsl(...)`, not `#rrggbb`; this host does the same, and
// switches dark, light and dark again without a reload. Each time the pasteboard must be the
// theme's canvas color (no light-grey band in dark), and the white slide stays white and where it
// was. SHOT=<dir> writes screenshots of the canvas in each scheme there.
import assert from 'node:assert/strict'
import { chromium, webkit } from 'playwright'
import { createServer } from 'vite'

const which = process.argv[2] ?? 'chromium'
const shots = process.env.SHOT ?? null
// Kition's theme tokens as it sends them (src/app/styles.css: `hsl(var(--surface-soft))` and so on).
const THEMES = {
  dark: { colorScheme: 'dark', tokens: { '--kit-canvas': 'hsl(220 13% 11%)', '--kit-surface': 'hsl(220 13% 14%)', '--kit-surface-muted': 'hsl(220 13% 16%)', '--kit-text': 'hsl(210 20% 92%)', '--kit-text-muted': 'hsl(215 12% 65%)', '--kit-border': 'hsl(220 10% 24%)' } },
  light: { colorScheme: 'light', tokens: { '--kit-canvas': 'hsl(30 14% 97%)', '--kit-surface': 'hsl(0 0% 100%)', '--kit-surface-muted': 'hsl(40 11% 98%)', '--kit-text': 'hsl(0 0% 10%)', '--kit-text-muted': 'hsl(45 5% 35%)', '--kit-border': 'hsl(40 11% 89%)' } },
}

const server = await createServer({ configFile: new URL('../vite.config.ts', import.meta.url).pathname, server: { port: 0, host: '127.0.0.1' }, logLevel: 'error' })
await server.listen()
const browser = await (which === 'webkit' ? webkit : chromium).launch()
const page = await browser.newPage({ viewport: { width: 1280, height: 900 } })
const errors = []
page.on('pageerror', (error) => errors.push(error.message))
try {
  await page.goto(`http://127.0.0.1:${server.httpServer.address().port}/dev.html?locale=en-US`)
  await page.waitForFunction(() => document.getElementById('status')?.textContent?.includes(' open in '), null, { timeout: 90_000 })
  const kit = page.frameLocator('#kit')
  const surface = kit.getByTestId('kit-canvas-surface')
  await surface.waitFor()

  /** The canvas's pixels: the share near the theme's canvas color, near `other`, light grey (not white), and white. */
  const measure = (other) =>
    surface.evaluate((canvas, other) => {
      const probe = document.createElement('canvas').getContext('2d')
      probe.fillStyle = getComputedStyle(document.documentElement).getPropertyValue('--kit-canvas').trim()
      const hex = String(probe.fillStyle)
      const want = [1, 3, 5].map((at) => parseInt(hex.slice(at, at + 2), 16))
      const { data } = canvas.getContext('2d').getImageData(0, 0, canvas.width, canvas.height)
      const near = (at, color) => Math.abs(data[at] - color[0]) <= 3 && Math.abs(data[at + 1] - color[1]) <= 3 && Math.abs(data[at + 2] - color[2]) <= 3
      let total = 0, canvasColor = 0, otherColor = 0, lightGrey = 0, white = 0
      for (let at = 0; at < data.length; at += 16) {
        total += 1
        if (near(at, want)) canvasColor += 1
        else if (other && near(at, other)) otherColor += 1
        else if (data[at] >= 253 && data[at + 1] >= 253 && data[at + 2] >= 253) white += 1
        else if ([0, 1, 2].every((channel) => data[at + channel] >= 200 && data[at + channel] <= 250)) lightGrey += 1
      }
      return { want, canvas: canvasColor / total, other: otherColor / total, lightGrey: lightGrey / total, white: white / total }
    }, other)

  const show = async (scheme, other) => {
    await page.evaluate((theme) => window.devHost.connection().setTheme(theme), THEMES[scheme])
    let stats = null
    for (let attempt = 0; attempt < 60; attempt += 1) {
      stats = await measure(other)
      if (stats.canvas >= 0.05 && stats.lightGrey <= 0.01 && stats.other <= 0.001) break
      await page.waitForTimeout(200)
    }
    if (shots) await surface.screenshot({ path: `${shots}/slide-backdrop-${scheme}-${which}.png` })
    assert(stats.canvas >= 0.05, `${scheme}: the pasteboard is the theme's canvas color ${stats.want}: ${JSON.stringify(stats)}`)
    assert(stats.lightGrey <= 0.01, `${scheme}: no light-grey pasteboard band: ${JSON.stringify(stats)}`)
    assert(stats.other <= 0.001, `${scheme}: nothing is left in the other scheme's canvas color: ${JSON.stringify(stats)}`)
    return stats
  }

  const dark = await show('dark', null)
  const light = await show('light', dark.want)
  const again = await show('dark', light.want)
  assert(dark.white > 0.1, `the white slide is on the canvas: ${JSON.stringify(dark)}`)
  for (const stats of [light, again]) assert(Math.abs(stats.white - dark.white) <= 0.01, `the slide stays white and in place: ${dark.white} -> ${stats.white}`)
  assert.equal(await kit.getByTestId('slides-unsaved').count(), 0, 'the theme is not an edit')
  assert.deepEqual(errors, [], 'no page errors')
  console.log(`PASS ${which}: the pasteboard follows the host's theme, live`)
} finally {
  await browser.close()
  await server.close()
}
