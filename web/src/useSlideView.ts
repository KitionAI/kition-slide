import { useCallback, useState } from 'react'

export type ViewMode = 'normal' | 'outline' | 'sorter' | 'notesPage'
export type TaskPane = 'selection' | 'animation' | 'comments' | 'spelling' | 'accessibility' | 'format' | 'background' | 'designer' | 'media' | null
export type SlideShowOptions = { from: number; reading: boolean; presenter: boolean; rehearse: boolean }

/** Editor-only state, as in the native UI; document changes still run in the Rust engine. */
export function useSlideView() {
  const [mode, setMode] = useState<ViewMode>('normal')
  const [notesOpen, setNotesOpen] = useState(true)
  const [ruler, setRuler] = useState(false)
  const [gridlines, setGridlines] = useState(false)
  const [guides, setGuides] = useState(false)
  const [grayscale, setGrayscale] = useState(false)
  const [dark, setDark] = useState(false)
  const [presenter, setPresenter] = useState(false)
  const [pane, setPane] = useState<TaskPane>(null)
  const [showing, setShowing] = useState<SlideShowOptions | null>(null)
  const [mediaAction, setMediaAction] = useState({ kind: 'pause', revision: 0 })
  const present = useCallback((from: number, options: Partial<Omit<SlideShowOptions, 'from'>> = {}) => setShowing({ from, reading: false, presenter, rehearse: false, ...options }), [presenter])
  const endShow = useCallback(() => setShowing(null), [])
  return { mode, setMode, notesOpen, setNotesOpen, ruler, setRuler, gridlines, setGridlines, guides, setGuides, grayscale, setGrayscale, dark, setDark, presenter, setPresenter, pane, setPane, showing, present, endShow, mediaAction, setMediaAction }
}
