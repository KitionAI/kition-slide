import { CameraIcon as Camera, EyeOffIcon as EyeOff, MonitorPlayIcon as MonitorPlay, PlayIcon as Play, PresentationIcon as Presentation, TimerIcon as Timer, VideoIcon as Video, ListVideoIcon, MonitorCogIcon } from '@kition/tessera/icons/react'
import { RibbonButton, RibbonGroup, RibbonStack } from '@kition/sdk/editor'
import { useSlides } from '../slides'

import { CommandButton } from './common'

export function SlideShowTab({ record = false }: { record?: boolean }) {
  const { session, t, view, run, facts } = useSlides()
  if (record) return <RibbonGroup label={t('group.record')} icon={Camera}>
    <RibbonButton icon={Camera} label={t('camera')} size="large" disabled />
    <RibbonButton primary icon={Play} label={t('fromBeginning')} size="large" onClick={() => view.present(0, { rehearse: true })} />
    <RibbonButton primary icon={MonitorPlay} label={t('fromCurrent')} size="large" onClick={() => view.present(session.state?.current ?? 0, { rehearse: true })} />
  </RibbonGroup>
  return (
    <>
    <RibbonGroup label={t('group.start')} icon={Play}>
      <RibbonButton primary icon={Play} label={t('fromBeginning')} size="large" onClick={() => view.present(0)} data-testid="slides-show-start" />
      <RibbonButton primary icon={MonitorPlay} label={t('fromCurrent')} size="large" onClick={() => view.present(session.state?.current ?? 0)} />
      <RibbonButton primary icon={Presentation} label={t('presenterView')} size="large" pressed={view.presenter} onClick={() => view.setPresenter(!view.presenter)} />
      <CommandButton command="show.customShow" icon={ListVideoIcon} label={t('customShow')} size="large" ask />
    </RibbonGroup>
    <RibbonGroup label={t('group.setup')} icon={MonitorCogIcon}>
      <CommandButton primary command="show.setup" icon={MonitorCogIcon} label={t('setupShow')} size="large" ask />
      <CommandButton primary command="slide.hide" icon={EyeOff} label={t('hideSlide')} size="large" pressed={facts.slides[session.state?.current ?? 0]?.hidden} params={{}} />
    </RibbonGroup>
    <RibbonGroup label={t('group.record')} icon={Camera}>
      <RibbonButton primary icon={Timer} label={t('rehearse')} size="large" onClick={() => view.present(0, { rehearse: true })} />
      <RibbonButton icon={Video} label={t('record')} size="large" disabled />
    </RibbonGroup>
    <RibbonGroup label={t('group.show')} icon={MonitorPlay}><RibbonStack>
      <label className="slides-field-row"><input type="checkbox" defaultChecked onChange={(event) => void run('show.setup', { useTimings: event.target.checked })} />{t('useTimings')}</label>
      <label className="slides-field-row"><input type="checkbox" defaultChecked onChange={(event) => void run('show.setup', { noNarration: !event.target.checked })} />{t('playNarrations')}</label>
      <label className="slides-field-row"><input type="checkbox" defaultChecked />{t('mediaControls')}</label>
    </RibbonStack></RibbonGroup>
    </>
  )
}
