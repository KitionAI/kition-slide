// Makes public/sample/Sample.kslide with the engine itself: a title slide and a process slide.
import fs from 'node:fs/promises'
import { fileURLToPath, pathToFileURL } from 'node:url'

const web = fileURLToPath(new URL('..', import.meta.url))
const engine = await import(pathToFileURL(`${web}public/engine/kition_slide_kit.js`).href)
await engine.default({ module_or_path: await fs.readFile(`${web}public/engine/kition_slide_kit_bg.wasm`) })
const doc = engine.SlideDocument.create('Sample.kslide', 960, 540)
const run = (command, params = {}) => doc.execute(command, JSON.stringify(params))
const title = JSON.parse(run('slide.inspect', { index: 0 })).shapes
run('text.edit', { id: title[0].id })
run('text.insert', { text: 'Northern Lights' })
run('text.exit')
run('slide.new', { layout: 'titleOnly' })
const shapes = [
  ['rect', 'Solar wind', '#F26B1D'],
  ['rect', 'Magnetosphere', '#2E6FD8'],
  ['rect', 'Particles collide', '#8A56D8'],
  ['star5', 'Light!', '#2E9E6B'],
]
shapes.forEach(([preset, text, color], index) => {
  run('shape.insert', { preset, rect: [60 + index * 220, 220, 190, 120], text })
  run('shape.fill', { color })
})
run('slide.go', { index: 0 })
await fs.writeFile(`${web}public/sample/Sample.kslide`, doc.save())
console.log('wrote public/sample/Sample.kslide')
