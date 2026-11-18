import { useEffect, useState } from 'react'
import { CopyCheckIcon as CopyCheck, SparkleIcon as Sparkle, PlayIcon as Play, ZapIcon as Zap, TimerIcon as Timer } from '@kition/tessera-icons/react'
import { RibbonButton, RibbonGroup, RibbonStack, MenuItem } from '@kition/kit-editor'
import { useSlides } from '../slides'
import { CommandButton, RibbonMenu } from './common'
import { transitionIcon } from './motionIcons'

type Transition = { id: string; label: string; category: string; duration: number; options: string[] }
type Current = { kind?: string; durationMs?: number; advanceOnClick?: boolean; advanceAfterMs?: number | null }

/** The transition gallery for the current slide, its duration, and Apply To All. */
export function TransitionsTab() {
  const { run, enabled, query, t, session, view } = useSlides()
  const [list, setList] = useState<Transition[]>([])
  const [current, setCurrent] = useState<Current | null>(null)
  const index = session.state?.current ?? 0
  useEffect(() => {
    void query<Transition[]>('transition.list').then((value) => value && setList(value))
  }, [query])
  useEffect(() => {
    void query<{ transition?: Current | null }>('slide.inspect', { index, select: false }).then((slide) => setCurrent(slide?.transition ?? null))
  }, [index, query, session.change.revision])
  return (
    <>
      <RibbonGroup label={t('group.preview')} icon={Play}><RibbonButton primary icon={Play} label={t('preview')} size="large" onClick={() => view.present(index, { reading: true })} /></RibbonGroup>
      <RibbonGroup label={t('group.transition')} icon={Sparkle}>
        <div className="slides-gallery" data-testid="slides-transitions">
          {list.slice(0, 18).map((item) => (
            <RibbonButton key={item.id} icon={transitionIcon(item.id)} label={item.label} size="large" pressed={(current?.kind ?? 'none') === item.id} disabled={!enabled('transition.set')} onClick={() => void run('transition.set', { kind: item.id })} data-testid={`slides-transition-${item.id}`} />
          ))}
        </div>
        <RibbonMenu primary icon={Sparkle} label={t('allEffects')} size="large">{list.map((item) => <MenuItem key={item.id} icon={transitionIcon(item.id)} label={`${item.label} (${item.category})`} onSelect={() => void run('transition.set', { kind: item.id })} />)}</RibbonMenu>
        <RibbonMenu primary icon={Zap} label={t('effectOptions')} size="large" disabled={!list.find((item) => item.id === current?.kind)?.options.length}>
          {list.find((item) => item.id === current?.kind)?.options.map((option) => <MenuItem key={option} label={option} onSelect={() => void run('transition.options', { option })} />)}
        </RibbonMenu>
      </RibbonGroup>
      <RibbonGroup label={t('group.timing')} icon={Timer}>
        <RibbonStack><label className="slides-field-row">
          <span>{t('duration')}</span>
          <input
            className="kui-field"
            type="number"
            min={0.1}
            step={0.1}
            style={{ width: 70 }}
            value={current?.durationMs != null ? current.durationMs / 1000 : ''}
            disabled={!current?.kind || current.kind === 'none'}
            onChange={(event) => void run('transition.timing', { duration: Math.round(Number(event.target.value) * 1000) })}
          />
        </label>
        <label className="slides-field-row"><input type="checkbox" checked={current?.advanceOnClick ?? true} onChange={(event) => void run('transition.timing', { onClick: event.target.checked })} />{t('onClick')}</label>
        <label className="slides-field-row">{t('after')}<input className="kui-field" type="number" min={0} step={0.25} value={(current?.advanceAfterMs ?? 0) / 1000} onChange={(event) => void run('transition.timing', { after: Math.round(Number(event.target.value) * 1000) || null })} /></label></RibbonStack>
        <CommandButton primary command="transition.applyAll" icon={CopyCheck} label={t('applyAll')} size="large" params={{}} />
      </RibbonGroup>
    </>
  )
}
