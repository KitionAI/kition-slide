/**
 * Pictures go into an empty placeholder (plan stage 9, walkthrough F7): before a picture is
 * inserted or pasted, the current slide's first empty content or picture placeholder is selected,
 * so the engine's `image.insert` puts the picture in it (a content placeholder holds it whole in its
 * middle, a picture placeholder is filled), as PowerPoint and WPS do. A placeholder already
 * selected is the engine's to take; without an empty one the picture goes in the slide's middle.
 */
type Query = <T>(command: string, params?: Record<string, unknown>) => Promise<T | null | undefined>
type Run = (command: string, params?: Record<string, unknown>) => Promise<unknown>

type SlideShape = { id: number; kind?: string; placeholder?: string; text?: string; hidden?: boolean }

/** The placeholder types a picture goes into: content (`obj`) and picture (`pic`). */
const TAKES_PICTURES = new Set(['obj', 'pic'])
/** The ones a selected empty placeholder takes a picture in (the engine's): a text one too. */
const ALL_PLACEHOLDERS = new Set(['obj', 'pic', 'body'])

const isEmpty = (shape: SlideShape, types: Set<string>) => shape.kind === 'placeholder' && types.has(shape.placeholder ?? '') && !shape.text && !shape.hidden

export async function selectEmptyPlaceholder(query: Query, run: Run) {
  const [slide, facts] = await Promise.all([
    query<{ shapes?: SlideShape[] }>('slide.inspect', { select: false }).catch(() => null),
    query<{ selection?: { shapes?: number[]; target?: { kind?: string } } }>('file.inspect').catch(() => null),
  ])
  const shapes = slide?.shapes ?? []
  const selected = new Set(facts?.selection?.shapes ?? [])
  // Editing a master or layout, or an empty placeholder (a text one too) already chosen: the engine's.
  if (facts?.selection?.target?.kind && facts.selection.target.kind !== 'slides') return
  if (shapes.some((shape) => selected.has(shape.id) && isEmpty(shape, ALL_PLACEHOLDERS))) return
  const empty = shapes.find((shape) => isEmpty(shape, TAKES_PICTURES))
  if (!empty) return
  await Promise.resolve(run('edit.select', { ids: [empty.id] })).catch(() => undefined)
}
