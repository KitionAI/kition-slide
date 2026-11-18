// Takes the README's pictures of the Slide editor (assets/readme/*.png at the repository root) on
// kition-sdk's shared runner (tools/readme-shots/README.md there): the development host in Classic
// mode, light theme, English, 1440 × 900 at device scale 2, on kition-showcase samples. Not a test;
// re-run it by hand when the editor changes:
//
//   cd web && pnpm install && pnpm run engine      # the engine and fonts in public/
//   node scripts/readme-shots.mjs [name ...]       # every shot, or only the named ones
//
// It needs kition-sdk and kition-showcase checkouts next to this repository (or KITION_SDK and
// KITION_SHOWCASE naming them). Run it with no HTTP proxy set. Every step is a person's: a click on
// a slide in the panel, on the canvas or on the ribbon. The pictures are PNG at device scale 2;
// shrink them afterwards with `pngquant --quality 70-90` (the README's are at most 400 KB each).
import fs from 'node:fs'
import path from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'

const web = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const sdk = path.resolve(process.env.KITION_SDK ?? path.join(web, '..', '..', 'kition-sdk'))
const showcase = path.resolve(process.env.KITION_SHOWCASE ?? path.join(web, '..', '..', 'kition-showcase'))
const { openFile, takeShots } = await import(pathToFileURL(path.join(sdk, 'tools/readme-shots/shots.mjs')).href)

/** The showcase sample each shot opens. */
const SAMPLE = {
  hero: 'seed-pitch-deck',
  layouts: 'layout-gallery',
  table: 'budget-variance-table',
  transitions: 'seed-pitch-deck',
  shapes: 'seed-pitch-deck',
}

/** The current slide's shapes (a query: it changes nothing). */
async function shapes(page) {
  const inspected = await page.evaluate(async () => (await window.devHost.connection().ai.run([{ command: 'slide.inspect', params: {} }], false)).completed[0].result)
  return inspected.shapes
}

/**
 * The page point at (`fx`, `fy`) of a shape's box, in slide points (960 × 540): the slide is centred
 * in the canvas at the zoom the status bar shows.
 */
async function point(page, kit, box, fx = 0.5, fy = 0.5) {
  const bounds = await kit.getByTestId('kit-canvas-surface').boundingBox()
  const zoom = Number((await kit.getByTestId('kit-zoom').textContent()).replace('%', '')) / 100
  const [x, y, w, h] = box
  return { x: bounds.x + bounds.width / 2 + (x + fx * w - 480) * zoom, y: bounds.y + bounds.height / 2 + (y + fy * h - 270) * zoom }
}

/** Shows slide `index` (from 0) by clicking it in the slides panel. */
async function slide(page, kit, index) {
  await kit.getByTestId(`slides-slide-${index}`).click()
  await page.waitForTimeout(600)
}

await takeShots({
  web,
  sample: async (page, shot) => {
    const name = SAMPLE[shot.name]
    await openFile(page, { name: `${name}.pptx`, bytes: fs.readFileSync(path.join(showcase, 'slide', name, `${name}.pptx`)) })
  },
  ready: '[data-testid="kit-canvas-surface"]',
  query: { modes: 'classic' },
  shots: [
    // A seven-slide pitch deck: the slides panel, the traction slide with its chart and three
    // figures, and the slide's speaker notes.
    { name: 'hero', steps: async (page, { kit }) => slide(page, kit, 4) },
    // Home › Layout on a template guide whose first slide maps its eleven layouts.
    {
      name: 'layouts',
      steps: async (page, { kit }) => {
        await kit.getByTestId('slides-layout').click()
        await kit.getByRole('menuitem', { name: 'Two Content' }).waitFor()
      },
    },
    // A click in a table cell: the table's frame, and the Table Design tab (style, shading, borders,
    // style options).
    {
      name: 'table',
      steps: async (page, { kit }) => {
        const table = (await shapes(page)).find((shape) => shape.kind === 'table')
        const cell = await point(page, kit, table.box, 0.1, 0.3)
        await page.mouse.click(cell.x, cell.y)
        await page.waitForTimeout(500)
        await kit.getByTestId('ribbon-tab-tableDesign').click()
      },
    },
    // The Transitions tab on the deck's third slide: its transition and timing.
    {
      name: 'transitions',
      steps: async (page, { kit }) => {
        await slide(page, kit, 2)
        await kit.getByTestId('ribbon-tab-transitions').click()
      },
    },
    // The middle card of the second slide clicked below its text: the shape selected, with its
    // size, rotation and corner handles.
    {
      name: 'shapes',
      steps: async (page, { kit }) => {
        await slide(page, kit, 1)
        const cards = (await shapes(page)).filter((shape) => shape.kind === 'shape')
        const middle = (shape) => Math.abs(shape.box[0] + shape.box[2] / 2 - 480)
        const card = cards.sort((a, b) => middle(a) - middle(b))[0]
        const at = await point(page, kit, card.box, 0.5, 0.9)
        await page.mouse.click(at.x, at.y)
      },
    },
  ],
})
