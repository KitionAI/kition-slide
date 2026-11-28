/**
 * View › Grid and Guides… and the slide's context menu (plan stage 8, WPS W-C4): snap objects to
 * the grid, the grid's spacing (in centimetres, as the size fields), the gridlines, the drawing
 * guides and the smart guides. OK keeps them on this machine (`gridSettings.ts`).
 */
import { useState } from 'react'
import { Button, Dialog } from '@kition/sdk/editor'
import { useSlides } from './slides'
import { returnKeyboardLater } from './focus'
import { GRID_DEFAULTS, GRID_MAX_PT, GRID_MIN_PT, PT_PER_CM } from './gridSettings'

/** Centimetres as the field shows them: two decimals, no trailing zeros. */
const centimetres = (points: number) => String(Number((points / PT_PER_CM).toFixed(2)))

export function GridDialog({ onClose }: { onClose: () => void }) {
  const { t, view } = useSlides()
  const [draft, setDraft] = useState(view.grid)
  const [spacing, setSpacing] = useState(centimetres(view.grid.gridSpacing))
  // The field shows centimetres to two decimals: left as it was shown, the spacing stays exactly as it was.
  const shown = centimetres(draft.gridSpacing)
  const points = spacing === shown ? draft.gridSpacing : Number(spacing.replace(',', '.')) * PT_PER_CM
  const valid = Number.isFinite(points) && points >= GRID_MIN_PT - 1e-6 && points <= GRID_MAX_PT + 1e-6
  const close = () => {
    onClose()
    returnKeyboardLater()
  }
  const apply = () => {
    if (!valid) return
    view.setGrid({ ...draft, gridSpacing: spacing === shown ? draft.gridSpacing : Math.min(GRID_MAX_PT, Math.max(GRID_MIN_PT, Math.round(points * 1000) / 1000)) })
    close()
  }
  const check = (key: 'snapToGrid' | 'gridlines' | 'guides' | 'smartGuides', label: string) => (
    <label className="slides-field-row">
      <input type="checkbox" checked={draft[key]} onChange={(event) => setDraft({ ...draft, [key]: event.target.checked })} data-testid={`slides-grid-${key}`} />
      {label}
    </label>
  )
  return (
    <Dialog
      title={t('grid.title')}
      onClose={close}
      testId="slides-grid-dialog"
      actions={
        <>
          <Button variant="ghost" onClick={() => { setDraft(GRID_DEFAULTS); setSpacing(centimetres(GRID_DEFAULTS.gridSpacing)) }} data-testid="slides-grid-defaults">
            {t('grid.defaults')}
          </Button>
          <Button variant="ghost" onClick={close}>
            {t('cancel')}
          </Button>
          <Button variant="primary" disabled={!valid} onClick={apply} data-testid="slides-grid-apply">
            {t('grid.apply')}
          </Button>
        </>
      }
    >
      <form
        className="slides-link-form"
        onSubmit={(event) => {
          event.preventDefault()
          apply()
        }}
      >
        <strong>{t('grid.snap')}</strong>
        {check('snapToGrid', t('grid.snapToGrid'))}
        <label className="slides-field-row">
          {t('grid.spacing')}
          <input
            className="kui-field"
            type="number"
            min={(GRID_MIN_PT / PT_PER_CM).toFixed(2)}
            max={(GRID_MAX_PT / PT_PER_CM).toFixed(2)}
            step="0.01"
            value={spacing}
            onChange={(event) => setSpacing(event.target.value)}
            aria-label={t('grid.spacing')}
            aria-invalid={!valid}
            data-testid="slides-grid-spacing"
          />
          {t('grid.cm')}
        </label>
        {check('gridlines', t('grid.gridlines'))}
        <strong>{t('grid.guidesHeading')}</strong>
        {check('guides', t('grid.guides'))}
        {check('smartGuides', t('grid.smartGuides'))}
      </form>
    </Dialog>
  )
}
