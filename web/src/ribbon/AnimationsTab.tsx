import { useEffect, useState } from 'react'
import { ListVideoIcon as ListVideo, PaintbrushIcon as Paintbrush, PlayIcon as Play, SparklesIcon as Sparkles, ZapIcon as Zap, TimerIcon as Timer } from '@kition/tessera-icons/react'
import { MenuItem, RibbonButton, RibbonGroup, RibbonStack } from '@kition/kit-editor'
import { useSlides } from '../slides'
import { CommandButton, RibbonMenu } from './common'
import { animationIcon } from './motionIcons'

type Effect = { id: string; label: string; class: string; options: string[] }
type Animation = { shape: number; effect: string; start: string; durationMs: number; delayMs: number }

export function AnimationsTab() {
  const { run, query, session, enabled, facts, view, t } = useSlides()
  const [effects, setEffects] = useState<Effect[]>([])
  const [animations, setAnimations] = useState<Animation[]>([])
  useEffect(() => { void query<Effect[]>('animation.list').then((list) => setEffects(list ?? [])) }, [query])
  useEffect(() => { void query<Animation[]>('animation.get').then((list) => setAnimations(list ?? [])) }, [query, session.change.revision, session.state?.current])
  const selected = facts.selection?.shapes ?? []
  const current = animations.find((animation) => selected.includes(animation.shape))
  const choose = (effect: Effect, add = false) => void run(add ? 'animation.add' : 'animation.set', { effect: effect.id, class: effect.class })
  const gallery = (add: boolean) => effects.map((effect) => <MenuItem key={effect.id} icon={animationIcon(effect.id)} label={`${effect.label} (${effect.class})`} disabled={!enabled('animation.set')} onSelect={() => choose(effect, add)} />)
  return <>
    <RibbonGroup label={t('group.preview')} icon={Play}><RibbonButton primary icon={Play} label={t('preview')} size="large" onClick={() => view.present(session.state?.current ?? 0, { reading: true })} /></RibbonGroup>
    <RibbonGroup label={t('group.animation')} icon={Sparkles}>
      <div className="slides-gallery" data-testid="slides-animations">{effects.filter((effect) => ['appear', 'fade', 'fly', 'float', 'split', 'wipe', 'zoom', 'pulse', 'spin', 'growShrink', 'fadeOut', 'flyOut', 'lines'].includes(effect.id)).map((effect) => <RibbonButton key={effect.id} icon={animationIcon(effect.id)} label={effect.label} size="large" pressed={current?.effect === effect.id} disabled={!enabled('animation.set')} onClick={() => choose(effect)} data-testid={`slides-animation-${effect.id}`} />)}</div>
      <RibbonMenu primary icon={Sparkles} label={t('allEffects')} size="large">{gallery(false)}</RibbonMenu>
      <CommandButton primary command="animation.options" icon={Zap} label={t('effectOptions')} size="large" ask />
    </RibbonGroup>
    <RibbonGroup label={t('group.advancedAnimation')} icon={Zap}>
      <RibbonMenu primary icon={Sparkles} label={t('addAnimation')} size="large">{gallery(true)}</RibbonMenu>
      <RibbonButton primary icon={ListVideo} label={t('animationPane')} size="large" onClick={() => view.setPane('animation')} />
      <CommandButton command="animation.timing" icon={Zap} label={t('trigger')} size="large" ask />
      <CommandButton command="animation.painter" icon={Paintbrush} label={t('animationPainter')} size="large" params={{}} />
    </RibbonGroup>
    <RibbonGroup label={t('group.timing')} icon={Timer}><RibbonStack>
      <label className="slides-field-row">{t('start')}<select className="kui-field" disabled={!current} value={current?.start ?? 'onClick'} onChange={(event) => void run('animation.timing', { start: event.target.value })}>{['onClick', 'withPrevious', 'afterPrevious'].map((value) => <option key={value} value={value}>{t(value)}</option>)}</select></label>
      {(['durationMs', 'delayMs'] as const).map((field) => <label className="slides-field-row" key={field}>{t(field === 'durationMs' ? 'duration' : 'delay')}<input className="kui-field" type="number" min={0} step={0.25} disabled={!current} value={(current?.[field] ?? 0) / 1000} onChange={(event) => void run('animation.timing', { [field === 'durationMs' ? 'duration' : 'delay']: Math.round(Number(event.target.value) * 1000) })} /></label>)}
    </RibbonStack></RibbonGroup>
  </>
}
