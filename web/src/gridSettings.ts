/**
 * Grid and guides (plan stage 8, WPS W-C4): snap to grid, the grid's spacing, the gridlines, the
 * drawing guides and the smart guides. They are this person's settings on this machine, never the
 * file's (`.kslide` is frozen at version 1): the editor keeps them in the frame's storage and gives
 * the engine the three that steer snapping (`setSnapping`). The gridlines are drawn at the spacing
 * the snapping uses.
 */
import { useEffect } from 'react'
import type { KitEngine } from '@kition/sdk/editor'

export type GridSettings = {
  /** Objects moved and resized land on the grid; arrow keys move one grid step. */
  snapToGrid: boolean
  /** The grid's spacing, in points. */
  gridSpacing: number
  /** The guides drawn while an object lines up with another one or the slide, which it snaps to. */
  smartGuides: boolean
  /** The gridlines drawn over the slide. */
  gridlines: boolean
  /** The drawing guides (the slide's middle, across and down). */
  guides: boolean
}

/** WPS's: a 6 pt grid that objects snap to, smart guides on, nothing drawn. */
export const GRID_DEFAULTS: GridSettings = { snapToGrid: true, gridSpacing: 6, smartGuides: true, gridlines: false, guides: false }
/** The spacing a person may set, in points (the engine's bounds). */
export const GRID_MIN_PT = 1
export const GRID_MAX_PT = 200
/** Points in a centimetre. */
export const PT_PER_CM = 72 / 2.54

const KEY = 'kition-slide.grid-guides'

/** The settings kept on this machine, else the defaults. */
export function loadGridSettings(): GridSettings {
  try {
    const kept = JSON.parse(window.localStorage.getItem(KEY) ?? 'null') as Partial<GridSettings> | null
    if (!kept || typeof kept !== 'object') return GRID_DEFAULTS
    const flag = (value: unknown, fallback: boolean) => (typeof value === 'boolean' ? value : fallback)
    const spacing = typeof kept.gridSpacing === 'number' && kept.gridSpacing >= GRID_MIN_PT && kept.gridSpacing <= GRID_MAX_PT ? kept.gridSpacing : GRID_DEFAULTS.gridSpacing
    return {
      snapToGrid: flag(kept.snapToGrid, GRID_DEFAULTS.snapToGrid),
      gridSpacing: spacing,
      smartGuides: flag(kept.smartGuides, GRID_DEFAULTS.smartGuides),
      gridlines: flag(kept.gridlines, GRID_DEFAULTS.gridlines),
      guides: flag(kept.guides, GRID_DEFAULTS.guides),
    }
  } catch {
    return GRID_DEFAULTS
  }
}

/** Keeps `settings` on this machine (a frame without storage keeps them for the session only). */
export function saveGridSettings(settings: GridSettings) {
  try {
    window.localStorage.setItem(KEY, JSON.stringify(settings))
  } catch {
    // No storage in this frame: the settings hold until it closes.
  }
}

/** Gives the open document's engine the settings that steer snapping, now and on every change. */
export function useEngineSnapping(doc: KitEngine | null, settings: GridSettings) {
  const { snapToGrid, gridSpacing, smartGuides } = settings
  useEffect(() => {
    if (!doc) return
    void doc.invoke('setSnapping', JSON.stringify({ snapToGrid, gridSpacing, smartGuides })).catch(() => undefined)
  }, [doc, snapToGrid, gridSpacing, smartGuides])
}

/** The gridlines' step on screen: the spacing, or the smallest multiple of it 8 px or more apart. */
export function gridStep(spacing: number, pixelsPerPoint: number) {
  if (!(spacing > 0) || !(pixelsPerPoint > 0)) return 0
  return spacing * Math.max(1, Math.ceil(8 / (spacing * pixelsPerPoint)))
}
