import { useCallback, useState } from 'react'
import { loadGridSettings, saveGridSettings, type GridSettings } from './gridSettings'

export type ViewMode = 'normal' | 'outline' | 'sorter' | 'notesPage'
export type TaskPane = 'selection' | 'animation' | 'comments' | 'spelling' | 'accessibility' | 'format' | 'background' | 'designer' | 'media' | null
export type SlideShowOptions = { from: number; reading: boolean; presenter: boolean; rehearse: boolean }

/** Editor-only state, as in the native UI; document changes still run in the Rust engine. */
export function useSlideView() {
  const [mode, setMode] = useState<ViewMode>('normal')
  const [notesOpen, setNotesOpen] = useState(true)
  const [ruler, setRuler] = useState(false)
  // Grid and guides: this person's, kept on this machine (plan stage 8, WPS W-C4).
  const [grid, setGridState] = useState<GridSettings>(loadGridSettings)
  const setGrid = useCallback((change: Partial<GridSettings>) => setGridState((now) => {
    const next = { ...now, ...change }
    saveGridSettings(next)
    return next
  }), [])
  const [gridDialog, setGridDialog] = useState(false)
  const { gridlines, guides } = grid
  const setGridlines = useCallback((on: boolean) => setGrid({ gridlines: on }), [setGrid])
  const setGuides = useCallback((on: boolean) => setGrid({ guides: on }), [setGrid])
  const [grayscale, setGrayscale] = useState(false)
  const [dark, setDark] = useState(false)
  const [presenter, setPresenter] = useState(false)
  const [pane, setPane] = useState<TaskPane>(null)
  const [showing, setShowing] = useState<SlideShowOptions | null>(null)
  const [mediaAction, setMediaAction] = useState({ kind: 'pause', revision: 0 })
  const present = useCallback((from: number, options: Partial<Omit<SlideShowOptions, 'from'>> = {}) => setShowing({ from, reading: false, presenter, rehearse: false, ...options }), [presenter])
  const endShow = useCallback(() => setShowing(null), [])
  return { mode, setMode, notesOpen, setNotesOpen, ruler, setRuler, gridlines, setGridlines, guides, setGuides, grid, setGrid, gridDialog, setGridDialog, grayscale, setGrayscale, dark, setDark, presenter, setPresenter, pane, setPane, showing, present, endShow, mediaAction, setMediaAction }
}
