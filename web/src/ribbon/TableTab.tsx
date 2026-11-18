import { useEffect, useState } from 'react'
import { AlignVerticalJustifyCenterIcon as AlignVerticalJustifyCenter, BetweenHorizontalEndIcon as BetweenHorizontalEnd, BetweenHorizontalStartIcon as BetweenHorizontalStart, BetweenVerticalEndIcon as BetweenVerticalEnd, BetweenVerticalStartIcon as BetweenVerticalStart, Columns2Icon as Columns2, Grid2x2Icon as Grid2X2, MergeIcon as Merge, PaintBucketIcon as PaintBucket, PaintbrushIcon as Paintbrush, Rows3Icon as Rows3, SplitIcon as Split, TextCursorInputIcon as TextCursorInput, TrashIcon as Trash2, SquareDashedMousePointerIcon, SquareDashedIcon, AlignVerticalJustifyStartIcon, AlignVerticalJustifyEndIcon, RulerIcon as Ruler } from '@kition/tessera-icons/react'
import { CommandButton, MenuItem, RibbonButton, RibbonGroup, RibbonMenu, RibbonStack } from '@kition/kit-editor'
import { useSlides } from '../slides'
import { ColorGrid } from './common'

type Cell = [number, number]
type TableState = {
  id: number
  rows: number
  cols: number
  from: Cell
  to: Cell
  style: string
  styles: Array<[string, string]>
  rowHeight: number
  columnWidth: number
  canMerge: boolean
  canSplit: boolean
  anchor: string
  options: Record<string, boolean>
}

const BORDER_EDGES = ['all', 'outside', 'inside', 'top', 'bottom', 'left', 'right', 'insideH', 'insideV', 'diagDown', 'diagUp'] as const
const PEN_WEIGHTS = [0.25, 0.5, 0.75, 1, 1.5, 2.25, 3, 4.5, 6]

/** Native table controls use the engine's selection, including merged-cell ownership. */
export function TableTab({ design = false }: { design?: boolean }) {
  const { run, query, session, enabled, t } = useSlides()
  const [table, setTable] = useState<TableState | null>(null)
  // The pen the Borders menu draws with, as in a table's Draw Borders group.
  const [pen, setPen] = useState({ color: 'tx1', width: 1 })
  useEffect(() => {
    let stale = false
    void query<TableState>('table.state').then((state) => { if (!stale) setTable(state ?? null) })
    return () => { stale = true }
  }, [query, session.change.revision, session.state])
  if (!table) return null
  const editable = enabled('table.cellFormat')
  const act = (command: string, params: Record<string, unknown> = {}) => void run(command, { id: table.id, ...params })
  const select = (from: Cell, to: Cell) => act('table.selectCells', { from, to })
  // In Single line's row the two deletes share one icon, so they keep their labels there.
  const button = (command: string, icon: typeof Rows3, label: string, disabled = false, primary = false) => (
    <CommandButton primary={primary} lineLabel={icon === Trash2} key={command} command={command} icon={icon} label={t(label)} disabled={!editable || disabled} params={{ id: table.id }} />
  )
  if (design) return <>
    <RibbonGroup label={t('table.styles')} icon={PaintBucket}>
      <label className="slides-field-row">{t('table.style')}<select className="kui-field" data-testid="slides-table-style" value={table.style} disabled={!editable} onChange={(event) => act('table.style', { style: event.target.value })}>
        {!table.styles.some(([id]) => id === table.style) ? <option value={table.style}>{table.style}</option> : null}
        {table.styles.map(([id, label]) => <option key={id} value={id}>{label}</option>)}
      </select></label>
      <RibbonMenu primary icon={PaintBucket} label={t('table.shading')} disabled={!editable} testId="slides-table-shading">
        <ColorGrid testId="slides-table-colors" onPick={(color) => act('table.cellFill', { color })} />
        <MenuItem label={t('table.noFill')} onSelect={() => act('table.cellFill', { none: true })} />
      </RibbonMenu>
    </RibbonGroup>
    <RibbonGroup label={t('table.drawBorders')} icon={Paintbrush}><RibbonStack>
      <RibbonMenu primary icon={Paintbrush} label={t('table.penColor')} disabled={!editable} swatch={pen.color.startsWith('#') ? pen.color : undefined} testId="slides-table-pen-color">
        <ColorGrid testId="slides-table-pen-colors" onPick={(color) => setPen({ ...pen, color })} />
      </RibbonMenu>
      <label className="slides-field-row">{t('table.penWeight')}<select className="kui-field" data-testid="slides-table-pen-weight" value={pen.width} disabled={!editable} onChange={(event) => setPen({ ...pen, width: Number(event.target.value) })}>
        {PEN_WEIGHTS.map((width) => <option key={width} value={width}>{t('table.points', { value: width })}</option>)}
      </select></label>
    </RibbonStack><RibbonStack>
      <RibbonMenu primary icon={Grid2X2} label={t('table.borders')} disabled={!editable} testId="slides-table-borders">
        {BORDER_EDGES.map((edges) => <MenuItem key={edges} label={t(`table.border.${edges}`)} onSelect={() => act('table.border', { edges, color: pen.color, width: pen.width })} />)}
        <MenuItem label={t('table.border.none')} onSelect={() => act('table.border', { edges: 'all', none: true })} />
      </RibbonMenu>
    </RibbonStack></RibbonGroup>
    <RibbonGroup label={t('table.options')} icon={Grid2X2}>
      <div className="kui-command-grid">{Object.entries(table.options).map(([key, checked]) => <label key={key} className="slides-field-row">
        <input type="checkbox" checked={checked} disabled={!editable} data-testid={`slides-table-${key}`} onChange={(event) => act('table.options', { [key]: event.target.checked })} />{t(`table.${key}`)}
      </label>)}</div>
    </RibbonGroup>
  </>
  return <>
    <RibbonGroup label={t('table.selection')} icon={SquareDashedMousePointerIcon}>
      <RibbonMenu icon={SquareDashedMousePointerIcon} label={t('table.select')} testId="slides-table-select">
        <MenuItem label={t('table.selectCell')} onSelect={() => select(table.from, table.from)} />
        <MenuItem label={t('table.selectRow')} onSelect={() => select([table.from[0], 0], [table.from[0], table.cols - 1])} />
        <MenuItem label={t('table.selectColumn')} onSelect={() => select([0, table.from[1]], [table.rows - 1, table.from[1]])} />
        <MenuItem label={t('table.selectAll')} onSelect={() => select([0, 0], [table.rows - 1, table.cols - 1])} />
      </RibbonMenu>
      <CommandButton command="text.edit" icon={TextCursorInput} label={t('table.editCell')} params={{ id: table.id, cell: table.from }} />
      <span data-testid="slides-table-selection">{t('table.cellPosition', { row: table.from[0] + 1, col: table.from[1] + 1 })}</span>
    </RibbonGroup>
    <RibbonGroup label={t('table.rowsColumns')} icon={Rows3}><RibbonStack>
      {button('table.insertRowAbove', BetweenHorizontalStart, 'table.insertAbove', false, true)}
      {button('table.insertRowBelow', BetweenHorizontalEnd, 'table.insertBelow', false, true)}
    </RibbonStack><RibbonStack>
      {button('table.insertColumnLeft', BetweenVerticalStart, 'table.insertLeft', false, true)}
      {button('table.insertColumnRight', BetweenVerticalEnd, 'table.insertRight', false, true)}
    </RibbonStack><RibbonStack>
      {button('table.deleteRow', Trash2, 'table.deleteRow', table.rows <= 1, true)}
      {button('table.deleteColumn', Trash2, 'table.deleteColumn', table.cols <= 1, true)}
    </RibbonStack></RibbonGroup>
    <RibbonGroup label={t('table.mergeGroup')} icon={Merge}><RibbonStack>
      {button('table.merge', Merge, 'table.merge', !table.canMerge, true)}
      {button('table.split', Split, 'table.split', !table.canSplit, true)}
    </RibbonStack></RibbonGroup>
    <RibbonGroup label={t('table.size')} icon={Ruler}><RibbonStack>
      {(['rowHeight', 'columnWidth'] as const).map((field) => <label key={field} className="slides-field-row">{t(`table.${field}`)}
        <input key={`${table.id}-${table.from.join('-')}-${table[field]}`} className="kui-field" type="number" min={4} max={4000} step={1} defaultValue={Math.round(table[field] * 100) / 100} disabled={!editable} data-testid={`slides-table-${field}`}
          onKeyDown={(event) => { if (event.key === 'Enter') event.currentTarget.blur() }}
          onBlur={(event) => { const value = event.currentTarget.valueAsNumber; if (Number.isFinite(value) && value >= 4 && value <= 4000 && value !== table[field]) act(`table.${field}`, { [field === 'rowHeight' ? 'height' : 'width']: value }) }} />
      </label>)}
    </RibbonStack><RibbonStack>
      {button('table.distributeRows', Rows3, 'table.distributeRows')}
      {button('table.distributeColumns', Columns2, 'table.distributeColumns')}
    </RibbonStack></RibbonGroup>
    <RibbonGroup label={t('table.alignment')} icon={AlignVerticalJustifyCenter}><RibbonStack>
      {(['top', 'center', 'bottom'] as const).map((anchor) => <RibbonButton key={anchor} icon={{ top: AlignVerticalJustifyStartIcon, center: AlignVerticalJustifyCenter, bottom: AlignVerticalJustifyEndIcon }[anchor]} label={t(`table.align.${anchor}`)} disabled={!editable} pressed={table.anchor === anchor} onClick={() => act('table.cellFormat', { anchor })} data-testid={`slides-table-align-${anchor}`} />)}
    </RibbonStack><RibbonStack>
      <RibbonMenu icon={SquareDashedIcon} label={t('table.margins')} disabled={!editable}>
        <MenuItem label={t('table.normalMargins')} onSelect={() => act('table.cellFormat', { margins: [7.2, 7.2, 3.6, 3.6] })} />
        <MenuItem label={t('table.noMargins')} onSelect={() => act('table.cellFormat', { margins: [0, 0, 0, 0] })} />
      </RibbonMenu>
    </RibbonStack></RibbonGroup>
  </>
}
