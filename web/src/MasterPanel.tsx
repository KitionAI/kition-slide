import { useSlides } from './slides'

export function MasterPanel() {
  const { session, run, t } = useSlides()
  const state = session.state
  return <aside className="kui-panel slides-slides" data-side="left" data-testid="slides-master-list">
    <div className="slides-slides-list">{state?.masters.map((master) => <section key={master.index}>
      <button type="button" className="kui-button slides-layout-item" aria-pressed={state.target.kind === 'master' && state.target.master === master.index} onClick={() => void run('view.slideMaster', { master: master.index, layout: null })}>{master.name || t('slideMaster')}</button>
      {master.layouts.map((layout) => <button type="button" className="kui-button slides-layout-item" key={layout.index} aria-pressed={state.target.master === master.index && state.target.layout === layout.index} onClick={() => void run('view.slideMaster', { master: master.index, layout: layout.index })}>{layout.name}</button>)}
    </section>)}</div>
  </aside>
}
