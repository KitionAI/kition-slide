// Export's file handling: node --test src/exporting.test.mjs (Node strips the module's types).
import assert from 'node:assert/strict'
import { test } from 'node:test'
import { decodeBase64, exportFiles, exportFolder, exportStem, writeExports } from './exporting.ts'

const PNG = 'iVBORw0KGgo='

test('names the file after the slides, next to them', () => {
  assert.equal(exportStem('Pitch.kslide'), 'Pitch')
  assert.equal(exportStem('a:b?.pptx'), 'a_b_')
  assert.equal(exportStem('.kslide'), 'Slides')
  assert.equal(exportFolder('talks/2026/Pitch.kslide'), 'talks/2026/')
  assert.equal(exportFolder('Pitch.kslide'), '')
  const [pdf] = exportFiles({ data: 'JVBERi0=' }, { name: 'Pitch.kslide', path: 'talks/Pitch.kslide' }, 'pdf')
  assert.equal(pdf.path, 'talks/Pitch.pdf')
  assert.equal(new TextDecoder().decode(pdf.bytes), '%PDF-')
})

test('every slide becomes its own numbered picture', () => {
  const files = exportFiles({ data: PNG, files: [{ slide: 0, data: PNG }, { slide: 1, data: PNG }] }, { name: 'Pitch.kslide', path: 'Pitch.kslide' }, 'png')
  assert.deepEqual(files.map((file) => file.path), ['Pitch 1.png', 'Pitch 2.png'])
  assert.deepEqual([...decodeBase64(PNG).slice(0, 4)], [0x89, 0x50, 0x4e, 0x47])
  const one = exportFiles({ data: PNG, files: [{ slide: 0, data: PNG }] }, { name: 'Pitch.kslide', path: 'Pitch.kslide' }, 'jpg')
  assert.deepEqual(one.map((file) => file.path), ['Pitch.jpg'], 'a one-slide file is not numbered')
})

test('answers the paths the host wrote, and stops at its refusal', async () => {
  const files = exportFiles({ data: PNG, files: [{ slide: 0, data: PNG }, { slide: 1, data: PNG }] }, { name: 'Pitch.kslide', path: 'Pitch.kslide' }, 'png')
  const asked = []
  const written = await writeExports(files, async (path) => {
    asked.push(path)
    return path.replace('.png', ' 2.png')
  })
  assert.deepEqual(written, ['Pitch 1 2.png', 'Pitch 2 2.png'], 'the host may choose an unused name')
  await assert.rejects(
    writeExports(files, async () => {
      throw new Error('the disk is full')
    }),
    /the disk is full/,
  )
  assert.deepEqual(asked, ['Pitch 1.png', 'Pitch 2.png'])
})
