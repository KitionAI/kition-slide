// Renders slides through the Slide engine's WebAssembly build for the visual-fidelity tool
// (crates/render/visual/examples/wasm.rs), which compares them with the references.
//
// node visual-render.mjs <job.json>
//
// The job is { slideFile, date: [year, month, day], slides: [{ index, width, height, out }] }. Each slide
// is written to `out` as straight RGBA8 (renderSlideClean); one line per slide goes to stdout:
// `ok <index> <ms>`, `error <index> <message>`, or `abort <index> <message>` when the engine
// trapped (a Rust panic aborts in WebAssembly), after which the process exits with code 3.
//
// The engine comes from KITION_VISUAL_WASM_ENGINE (default public/engine, from `pnpm engine`).
// Fonts are what the editor has: the faces built into the engine plus Droid Sans Fallback, from
// KITION_VISUAL_WASM_FONT or public/fonts.
import fs from 'node:fs'
import path from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'

const web = fileURLToPath(new URL('..', import.meta.url))
const engineDir = process.env.KITION_VISUAL_WASM_ENGINE || path.join(web, 'public/engine')
const font = process.env.KITION_VISUAL_WASM_FONT || path.join(web, 'public/fonts/DroidSansFallbackFull.ttf')

const job = JSON.parse(fs.readFileSync(process.argv[2], 'utf8'))
const engine = await import(pathToFileURL(path.join(engineDir, 'kition_slide_kit.js')).href)
await engine.default({ module_or_path: fs.readFileSync(path.join(engineDir, 'kition_slide_kit_bg.wasm')) })
if (fs.existsSync(font)) engine.addFont(fs.readFileSync(font))

const say = (line) => fs.writeSync(1, `${line.replace(/\s+/g, ' ')}\n`)
const trapped = (e) => e instanceof WebAssembly.RuntimeError

let doc
try {
  doc = engine.SlideDocument.open(path.basename(job.slideFile), fs.readFileSync(job.slideFile))
} catch (e) {
  say(`${trapped(e) ? 'abort' : 'error'} -1 open: ${e?.message ?? e}`)
  process.exit(trapped(e) ? 3 : 1)
}
const [year, month, day] = job.date
for (const s of job.slides) {
  const t = performance.now()
  try {
    const rgba = doc.renderSlideClean(s.index, s.width, s.height, year, month, day)
    fs.writeFileSync(s.out, rgba)
    say(`ok ${s.index} ${Math.round(performance.now() - t)}`)
  } catch (e) {
    say(`${trapped(e) ? 'abort' : 'error'} ${s.index} ${e?.message ?? e}`)
    if (trapped(e)) process.exit(3)
  }
}
