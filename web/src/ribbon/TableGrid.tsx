/**
 * Insert › Table (plan stage 9, walkthrough F9): a grid of cells to sweep over, as WPS and
 * PowerPoint have it: the cells up to the one under the pointer light up, the heading says the
 * size, and a click inserts a table that size. Insert Table… asks for the rows and columns.
 */
import { TableIcon as Table } from '@kition/tessera/icons/react'
import { Button, Dialog, MenuItem, RibbonMenu } from '@kition/sdk/editor'
import { useState, type KeyboardEvent } from 'react'
import { useSlides } from '../slides'
import { returnKeyboardLater } from '../focus'

/** The grid's size: 10 columns by 8 rows (the largest a sweep makes). */
const COLS = 10
const ROWS = 8
/** The largest table the dialog makes, each way (the engine's own bound is larger). */
const MOST = 75

export function TableMenu({ primary }: { primary?: boolean }) {
  const { run, enabled, t, view } = useSlides()
  const [at, setAt] = useState<{ rows: number; cols: number }>({ rows: 0, cols: 0 })
  const move = (event: KeyboardEvent<HTMLDivElement>) => {
    const step: Record<string, [number, number]> = { ArrowRight: [0, 1], ArrowLeft: [0, -1], ArrowDown: [1, 0], ArrowUp: [-1, 0] }
    const by = step[event.key]
    if (!by) return
    event.preventDefault()
    event.stopPropagation()
    const rows = Math.min(ROWS, Math.max(1, (at.rows || 1) + (at.rows ? by[0] : 0)))
    const cols = Math.min(COLS, Math.max(1, (at.cols || 1) + (at.cols ? by[1] : 0)))
    setAt({ rows, cols })
    event.currentTarget.querySelector<HTMLButtonElement>(`[data-cell="${rows}x${cols}"]`)?.focus()
  }
  return (
    <RibbonMenu icon={Table} label={t('table')} size="large" disabled={!enabled('table.insert')} testId="slides-table" primary={primary}>
      <div className="slides-menu-heading" data-testid="slides-table-size">{at.rows ? t('tableGrid.size', { rows: at.rows, cols: at.cols }) : t('table')}</div>
      <div className="slides-table-grid" style={{ gridTemplateColumns: `repeat(${COLS}, 18px)` }} onKeyDown={move} onPointerLeave={() => setAt({ rows: 0, cols: 0 })} data-testid="slides-table-grid">
        {Array.from({ length: ROWS * COLS }, (_, index) => {
          const rows = Math.floor(index / COLS) + 1
          const cols = (index % COLS) + 1
          return (
            <button
              key={index}
              type="button"
              role="menuitem"
              className="slides-table-cell"
              aria-label={t('insertTable', { rows, cols })}
              data-cell={`${rows}x${cols}`}
              data-lit={(rows <= at.rows && cols <= at.cols) || undefined}
              onPointerEnter={() => setAt({ rows, cols })}
              onFocus={() => setAt({ rows, cols })}
              onClick={() => void run('table.insert', { rows, cols })}
            />
          )
        })}
      </div>
      <MenuItem icon={Table} label={t('tableGrid.insert')} onSelect={() => view.setTableDialog(true)} testId="slides-table-dialog-open" />
    </RibbonMenu>
  )
}

/** Insert Table…: how many rows and columns, then OK. */
export function TableDialog({ onClose }: { onClose: () => void }) {
  const { run, t } = useSlides()
  const [rows, setRows] = useState('2')
  const [cols, setCols] = useState('5')
  const count = (text: string) => {
    const value = Number(text)
    return Number.isInteger(value) && value >= 1 && value <= MOST ? value : null
  }
  const valid = count(rows) !== null && count(cols) !== null
  const close = () => {
    onClose()
    returnKeyboardLater()
  }
  const apply = () => {
    const [r, c] = [count(rows), count(cols)]
    if (r === null || c === null) return
    void run('table.insert', { rows: r, cols: c })
    close()
  }
  const field = (label: string, value: string, set: (text: string) => void, testId: string) => (
    <label className="slides-field-row">
      {label}
      <input className="kui-field" type="number" min={1} max={MOST} step={1} value={value} onChange={(event) => set(event.target.value)} aria-label={label} aria-invalid={count(value) === null} data-testid={testId} />
    </label>
  )
  return (
    <Dialog
      title={t('tableDialog.title')}
      onClose={close}
      testId="slides-table-dialog"
      actions={
        <>
          <Button variant="ghost" onClick={close}>
            {t('cancel')}
          </Button>
          <Button variant="primary" disabled={!valid} onClick={apply} data-testid="slides-table-dialog-apply">
            {t('tableDialog.apply')}
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
        {field(t('tableDialog.cols'), cols, setCols, 'slides-table-dialog-cols')}
        {field(t('tableDialog.rows'), rows, setRows, 'slides-table-dialog-rows')}
      </form>
    </Dialog>
  )
}
