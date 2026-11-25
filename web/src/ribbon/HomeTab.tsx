import { AArrowDownIcon as AArrowDown, AArrowUpIcon as AArrowUp, TextAlignCenterIcon as AlignCenter, TextAlignJustifyIcon as AlignJustify, TextAlignStartIcon as AlignLeft, TextAlignEndIcon as AlignRight, BaselineIcon as Baseline, BoldIcon as Bold, BringToFrontIcon as BringToFront, ClipboardPasteIcon as ClipboardPaste, CopyIcon as Copy, CaseSensitiveIcon as CaseSensitive, Columns2Icon as Columns2, PanelRightIcon as PanelRight, SparklesIcon as Sparkles, GroupIcon as Group, HighlighterIcon as Highlighter, ImageIcon as Image, ListIndentDecreaseIcon as IndentDecrease, ListIndentIncreaseIcon as IndentIncrease, ItalicIcon as Italic, LayersIcon as Layers, LayoutTemplateIcon as LayoutTemplate, ListIcon as List, ListOrderedIcon as ListOrdered, PaintBucketIcon as PaintBucket, PaintbrushIcon as Paintbrush, PenLineIcon as PenLine, RemoveFormattingIcon as RemoveFormatting, RotateCcwIcon as RotateCcw, Rows3Icon as Rows3, ScissorsIcon as Scissors, SendToBackIcon as SendToBack, SquarePlusIcon as SquarePlus, StrikethroughIcon as Strikethrough, SubscriptIcon as Subscript, SuperscriptIcon as Superscript, TextCursorInputIcon as TextCursorInput, UnderlineIcon as Underline, UngroupIcon as Ungroup, MoveHorizontalIcon, MoveVerticalIcon, AlignVerticalJustifyCenterIcon, ShapesIcon as Shapes } from '@kition/tessera/icons/react'
import { MenuItem, MenuSeparator, RibbonButton, RibbonGroup, RibbonItem, RibbonRow, RibbonStack, toast } from '@kition/sdk/editor'
import { useSlides } from '../slides'
import { useTools } from '../tools'
import { ColorGrid, CommandButton, RibbonMenu, ShapesMenu } from './common'

const SIZES = [8, 9, 10, 10.5, 11, 12, 14, 16, 18, 20, 24, 28, 32, 36, 40, 44, 48, 54, 60, 66, 72, 80, 88, 96]
const SPACINGS = [1, 1.15, 1.5, 2, 2.5, 3]
const PICTURES = ['png', 'jpg', 'jpeg', 'gif', 'webp', 'svg', 'bmp']

function base64(bytes: ArrayBuffer) {
  let text = ''
  const view = new Uint8Array(bytes)
  for (let index = 0; index < view.length; index += 0x8000) text += String.fromCharCode(...view.subarray(index, index + 0x8000))
  return btoa(text)
}

/** Picks a workspace picture through the host and inserts it on the current slide. */
export function usePictureInsert() {
  const { host, run, t } = useSlides()
  const can = host.init.capabilities.includes('pick-file')
  const insert = async () => {
    try {
      const path = await host.pick('picture', PICTURES)
      if (!path) return
      const { bytes } = await host.files.read(path)
      await run('image.insert', { data: base64(bytes), name: path.split('/').pop() })
    } catch (error) {
      toast('error', t('commandFailed'), error instanceof Error ? error.message : String(error))
    }
  }
  return { can, insert }
}

export function HomeTab() {
  const { run, enabled, format, facts, t, view } = useSlides()
  const tools = useTools()
  const picture = usePictureInsert()
  const fonts = Array.from(new Set([facts.fonts?.major, facts.fonts?.minor, format.font, 'Inter', 'Arial', 'Helvetica', 'Georgia', 'Times New Roman', 'Courier New'].filter((font): font is string => Boolean(font))))
  const textOn = enabled('format.bold')
  const align = format.align ?? ''
  return (
    <>
      <RibbonGroup label={t('group.clipboard')} icon={ClipboardPaste}>
        <CommandButton command="edit.paste" primary icon={ClipboardPaste} label={t('paste')} size="large" />
        <RibbonStack>
          <CommandButton command="edit.cut" icon={Scissors} label={t('cut')} />
          <CommandButton command="edit.copy" icon={Copy} label={t('copy')} />
          <CommandButton command="format.painter" icon={Paintbrush} label={t('formatPainter')} pressed={format.painter} params={{}} />
        </RibbonStack>
      </RibbonGroup>

      <RibbonGroup label={t('group.slides')} icon={SquarePlus}>
        <CommandButton command="slide.new" primary icon={SquarePlus} label={t('newSlide')} size="large" params={{}} />
        <RibbonStack>
          <RibbonMenu icon={LayoutTemplate} label={t('layout')} disabled={!enabled('slide.layout')} testId="slides-layout">
            {facts.layouts.map((layout) => (
              <MenuItem key={layout.id} label={layout.name} onSelect={() => void run('slide.layout', { layout: layout.name })} testId={`slides-layout-${layout.id}`} />
            ))}
          </RibbonMenu>
          <CommandButton command="slide.reset" icon={RotateCcw} label={t('reset')} params={{}} />
          <RibbonMenu icon={Rows3} label={t('section')}>
            <MenuItem label={t('addSection')} onSelect={() => void run('section.add', {})} />
            <MenuItem label={t('removeSections')} disabled={!facts.sections.length} onSelect={() => void run('section.removeAll', {})} />
          </RibbonMenu>
        </RibbonStack>
      </RibbonGroup>

      <RibbonGroup label={t('group.font')} icon={Baseline}>
        <RibbonStack>
          <RibbonRow>
            <RibbonItem primary>
              <select className="kui-ribbon-field" style={{ width: 132 }} aria-label={t('font')} disabled={!textOn} value={format.font ?? ''} onChange={(event) => void run('format.fontName', { family: event.target.value })} data-testid="slides-font">
                {format.font ? null : <option value="" />}
                {fonts.map((font) => (
                  <option key={font} value={font}>
                    {font}
                  </option>
                ))}
              </select>
            </RibbonItem>
            <RibbonItem primary>
              <select className="kui-ribbon-field" style={{ width: 62 }} aria-label={t('fontSize')} disabled={!textOn} value={format.size ?? ''} onChange={(event) => void run('format.fontSize', { size: Number(event.target.value) })} data-testid="slides-font-size">
                {format.size == null || SIZES.includes(format.size) ? null : <option value={format.size}>{format.size}</option>}
                {format.size == null ? <option value="" /> : null}
                {SIZES.map((size) => (
                  <option key={size} value={size}>
                    {size}
                  </option>
                ))}
              </select>
            </RibbonItem>
            <CommandButton command="format.increaseFontSize" icon={AArrowUp} label={t('grow')} size="icon" params={{}} />
            <CommandButton command="format.decreaseFontSize" icon={AArrowDown} label={t('shrink')} size="icon" params={{}} />
            <RibbonMenu icon={CaseSensitive} label={t('changeCase')} size="icon" disabled={!textOn}>{['sentence', 'lower', 'upper', 'title', 'toggle'].map((mode) => <MenuItem key={mode} label={mode} onSelect={() => void run('format.case', { mode })} />)}</RibbonMenu>
            <RibbonMenu icon={MoveHorizontalIcon} label={t('characterSpacing')} size="icon" disabled={!textOn}>{[-3, -1.5, 0, 3, 6].map((pt) => <MenuItem key={pt} label={`${pt} pt`} onSelect={() => void run('format.spacing', { pt })} />)}</RibbonMenu>
            <CommandButton command="format.clear" icon={RemoveFormatting} label={t('clearFormatting')} size="icon" params={{}} />
          </RibbonRow>
          <RibbonRow>
            <CommandButton command="format.bold" primary icon={Bold} label={t('bold')} size="icon" pressed={format.bold} params={{}} />
            <CommandButton command="format.italic" primary icon={Italic} label={t('italic')} size="icon" pressed={format.italic} params={{}} />
            <CommandButton command="format.underline" primary icon={Underline} label={t('underline')} size="icon" pressed={format.underline} params={{}} />
            <CommandButton command="format.strikethrough" icon={Strikethrough} label={t('strikethrough')} size="icon" pressed={format.strike} params={{}} />
            <CommandButton command="format.superscript" icon={Superscript} label={t('superscript')} size="icon" pressed={format.superscript} params={{}} />
            <CommandButton command="format.subscript" icon={Subscript} label={t('subscript')} size="icon" pressed={format.subscript} params={{}} />
            <CommandButton command="format.textShadow" icon={Baseline} label={t('textShadow')} size="icon" params={{}} />
            <RibbonMenu icon={Baseline} label={t('fontColor')} size="icon" disabled={!textOn} swatch="#C00000" testId="slides-font-color" primary>
              <ColorGrid onPick={(color) => void run('format.color', { color })} testId="slides-font-colors" />
            </RibbonMenu>
            <RibbonMenu icon={Highlighter} label={t('highlight')} size="icon" disabled={!textOn} swatch="#FFFF00">
              <MenuItem label={t('noHighlight')} onSelect={() => void run('format.highlight', {})} />
              <ColorGrid onPick={(color) => void run('format.highlight', { color })} testId="slides-highlights" />
            </RibbonMenu>
          </RibbonRow>
        </RibbonStack>
      </RibbonGroup>

      <RibbonGroup label={t('group.paragraph')} icon={AlignLeft}>
        <RibbonStack>
          <RibbonRow>
            <CommandButton command="format.bullets" primary icon={List} label={t('bullets')} size="icon" pressed={format.bullets} params={{}} />
            <CommandButton command="format.numbering" primary icon={ListOrdered} label={t('numbering')} size="icon" pressed={format.numbering} params={{}} />
            <CommandButton command="format.outdent" icon={IndentDecrease} label={t('outdent')} size="icon" params={{}} />
            <CommandButton command="format.indent" icon={IndentIncrease} label={t('indent')} size="icon" params={{}} />
            <RibbonMenu icon={MoveVerticalIcon} label={t('lineSpacing')} size="icon" disabled={!enabled('format.lineSpacing')}>
              {SPACINGS.map((lines) => (
                <MenuItem key={lines} label={lines.toFixed(lines % 1 ? 2 : 1)} onSelect={() => void run('format.lineSpacing', { lines })} />
              ))}
            </RibbonMenu>
          </RibbonRow>
          <RibbonRow>
            <CommandButton command="format.alignLeft" primary icon={AlignLeft} label={t('alignLeft')} size="icon" pressed={align === 'l'} params={{}} />
            <CommandButton command="format.alignCenter" primary icon={AlignCenter} label={t('alignCenter')} size="icon" pressed={align === 'ctr'} params={{}} />
            <CommandButton command="format.alignRight" icon={AlignRight} label={t('alignRight')} size="icon" pressed={align === 'r'} params={{}} />
            <CommandButton command="format.justify" icon={AlignJustify} label={t('justify')} size="icon" pressed={align === 'just'} params={{}} />
          </RibbonRow>
          <RibbonRow>
            <RibbonMenu icon={Columns2} label={t('columns')} size="icon" disabled={!textOn}>{[1, 2, 3].map((count) => <MenuItem key={count} label={String(count)} onSelect={() => void run('format.columns', { count })} />)}</RibbonMenu>
            <RibbonMenu icon={TextCursorInput} label={t('textDirection')} size="icon" disabled={!textOn}>{['horizontal', 'rotate90', 'rotate270', 'stacked'].map((dir) => <MenuItem key={dir} label={dir} onSelect={() => void run('format.direction', { dir })} />)}</RibbonMenu>
            <RibbonMenu icon={AlignVerticalJustifyCenterIcon} label={t('alignText')} size="icon" disabled={!textOn}>{['top', 'middle', 'bottom'].map((anchor) => <MenuItem key={anchor} label={anchor} onSelect={() => void run('format.anchor', { anchor })} />)}</RibbonMenu>
          </RibbonRow>
        </RibbonStack>
      </RibbonGroup>

      <RibbonGroup label={t('group.drawing')} icon={Shapes}>
        <ShapesMenu label={t('shapes')} primary />
        <RibbonButton icon={TextCursorInput} label={t('textBox')} size="large" pressed={tools.active === 'textBox'} onClick={() => tools.select('textBox')} data-testid="slides-tool-textBox" primary />
        <RibbonMenu icon={Layers} label={t('arrange')} size="large" disabled={!enabled('arrange.bringToFront')}>
          <MenuItem icon={BringToFront} label={t('bringToFront')} onSelect={() => void run('arrange.bringToFront', {})} />
          <MenuItem label={t('bringForward')} onSelect={() => void run('arrange.bringForward', {})} />
          <MenuItem label={t('sendBackward')} onSelect={() => void run('arrange.sendBackward', {})} />
          <MenuItem icon={SendToBack} label={t('sendToBack')} onSelect={() => void run('arrange.sendToBack', {})} />
          <MenuSeparator />
          <MenuItem icon={Group} label={t('group')} disabled={!enabled('arrange.group')} onSelect={() => void run('arrange.group', {})} />
          <MenuItem icon={Ungroup} label={t('ungroup')} disabled={!enabled('arrange.ungroup')} onSelect={() => void run('arrange.ungroup', {})} />
          <MenuSeparator />
          <MenuItem icon={PanelRight} label={t('selectionPane')} onSelect={() => view.setPane('selection')} />
        </RibbonMenu>
        <RibbonStack>
          <RibbonMenu icon={PaintBucket} label={t('shapeFill')} disabled={!enabled('shape.fill')} testId="slides-shape-fill">
            <MenuItem label={t('noFill')} onSelect={() => void run('shape.fill', { none: true })} />
            <ColorGrid onPick={(color) => void run('shape.fill', { color })} testId="slides-fills" />
          </RibbonMenu>
          <RibbonMenu icon={PenLine} label={t('shapeOutline')} disabled={!enabled('shape.line')}>
            <MenuItem label={t('noOutline')} onSelect={() => void run('shape.line', { none: true })} />
            <ColorGrid onPick={(color) => void run('shape.line', { color })} testId="slides-outlines" />
          </RibbonMenu>
        </RibbonStack>
      </RibbonGroup>

      {picture.can ? (
        <RibbonGroup label={t('group.images')} icon={Image}>
          <RibbonButton icon={Image} label={t('picture')} size="large" disabled={!enabled('image.insert')} onClick={() => void picture.insert()} primary />
        </RibbonGroup>
      ) : null}
      <RibbonGroup label={t('designIdeas')} icon={Sparkles}><RibbonButton icon={Sparkles} label={t('designIdeas')} size="large" onClick={() => view.setPane('designer')} primary /></RibbonGroup>
    </>
  )
}
