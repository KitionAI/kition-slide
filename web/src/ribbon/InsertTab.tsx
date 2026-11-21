import { AudioLinesIcon as AudioLines, CalendarClockIcon as CalendarClock, CameraIcon as Camera, CaseSensitiveIcon as CaseSensitive, HashIcon as Hash, ImageIcon as Image, LinkIcon as Link, MessageSquarePlusIcon as MessageSquarePlus, PanelTopIcon as PanelTop, SigmaIcon as Sigma, FaceSlightlySmilingIcon as Smile, SquarePlusIcon as SquarePlus, TableIcon as Table, TextCursorInputIcon as TextCursorInput, VideoIcon as Video, WorkflowIcon as Workflow, ShapesIcon as Shapes } from '@kition/tessera/icons/react'
import { ChartInsertMenu, MenuItem, RibbonButton, RibbonGroup, CommandItem } from '@kition/sdk/editor'
import { useSlides } from '../slides'
import { useTools } from '../tools'
import { CommandButton, RibbonMenu, ShapesMenu } from './common'
import { usePictureInsert } from './HomeTab'
import { useMediaInsert } from './useMediaInsert'

const TABLE_SIZES: Array<[number, number]> = [
  [2, 2],
  [3, 3],
  [4, 3],
  [4, 4],
  [5, 4],
  [6, 5],
]

export function InsertTab() {
  const { run, enabled, t, setDialog, byId, newComment, canNewComment } = useSlides()
  const tools = useTools()
  const picture = usePictureInsert()
  const audio = useMediaInsert('audio')
  const video = useMediaInsert('video')
  return (
    <>
      <RibbonGroup label={t('group.slides')} icon={SquarePlus}>
        <CommandButton command="slide.new" primary icon={SquarePlus} label={t('newSlide')} size="large" params={{}} />
      </RibbonGroup>
      <RibbonGroup label={t('group.tables')} icon={Table}>
        <RibbonMenu icon={Table} label={t('table')} size="large" disabled={!enabled('insert.table')} testId="slides-table" primary>
          {TABLE_SIZES.map(([cols, rows]) => (
            <MenuItem key={`${cols}x${rows}`} label={t('insertTable', { cols, rows })} onSelect={() => void run('insert.table', { rows, cols })} />
          ))}
          <CommandItem command="insert.table" ask />
        </RibbonMenu>
      </RibbonGroup>
      <RibbonGroup label={t('group.images')} icon={Image}>
        <RibbonButton icon={Image} label={t('picture')} size="large" disabled={!picture.can || !enabled('insert.picture')} onClick={() => void picture.insert()} primary />
        <RibbonButton icon={Camera} label={t('screenshot')} size="large" disabled />
      </RibbonGroup>
      <RibbonGroup label={t('group.illustrations')} icon={Shapes}>
        <ShapesMenu label={t('shapes')} primary />
        <RibbonMenu icon={Smile} label={t('icons')} size="large">
          {['star5', 'heart', 'lightningBolt', 'sun', 'moon', 'cloud', 'smileyFace', 'chevron', 'rightArrow', 'flowChartDecision', 'flowChartMagneticDisk', 'flowChartDocument'].map((preset) => <MenuItem key={preset} label={preset} onSelect={() => void run('shape.insert', { preset, rect: [400, 200, 96, 96] })} />)}
        </RibbonMenu>
        <CommandButton command="insert.smartGraphic" icon={Workflow} label={t('smartGraphic')} size="large" ask />
        <ChartInsertMenu testId="slides-chart" primary />
      </RibbonGroup>
      <RibbonGroup label={t('group.links')} icon={Link}><CommandButton command="insert.hyperlink" primary icon={Link} label={t('link')} size="large" ask /><CommandButton command="insert.actionButton" icon={Link} label={t('action')} size="large" ask /></RibbonGroup>
      <RibbonGroup label={t('group.comments')} icon={MessageSquarePlus}><RibbonButton icon={MessageSquarePlus} label={t('comment')} size="large" disabled={!canNewComment} onClick={() => void newComment()} data-testid="slides-new-comment" primary /></RibbonGroup>
      <RibbonGroup label={t('group.text')} icon={TextCursorInput}>
        <RibbonButton icon={TextCursorInput} label={t('textBox')} size="large" pressed={tools.active === 'textBox'} onClick={() => tools.select('textBox')} primary />
        <CommandButton command="design.headerFooter" icon={PanelTop} label={t('headerFooter')} size="large" ask />
        <CommandButton command="insert.artText" icon={CaseSensitive} label={t('artText')} size="large" params={{}} />
        <CommandButton command="insert.slideNumber" icon={Hash} label={t('slideNumber')} size="large" params={{}} />
        <CommandButton command="insert.dateTime" icon={CalendarClock} label={t('dateTime')} size="large" params={{}} />
      </RibbonGroup>
      <RibbonGroup label={t('group.symbols')} icon={Sigma}>
        <RibbonButton icon={Sigma} label={t('equation')} size="large" onClick={() => { const command = byId.get('insert.textBox'); if (command) setDialog({ ...command, title: t('equation'), params: '{text}' }, { text: 'a² + b² = c²', rect: [330, 245, 300, 50] }) }} />
        <CommandButton command="insert.symbol" icon={Sigma} label={t('symbol')} size="large" ask />
      </RibbonGroup>
      <RibbonGroup label={t('group.media')} icon={Video}>
        <RibbonButton icon={Video} label={t('video')} size="large" disabled={!video.can} onClick={() => void video.insert()} />
        <RibbonButton icon={AudioLines} label={t('audio')} size="large" disabled={!audio.can} onClick={() => void audio.insert()} />
      </RibbonGroup>
    </>
  )
}
