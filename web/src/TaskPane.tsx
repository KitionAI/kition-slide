import { useEffect, useState } from 'react'
import { Button, CommandTab, IconButton, useReadOnly } from '@kition/sdk/editor'
import { XIcon as X } from '@kition/tessera/icons/react'
import { useSlides } from './slides'
import { MediaPlayer } from './MediaPlayer'
import { DesignIdeas } from './DesignIdeas'
import { CommentsPane } from './Comments'

type Item = { id?: number; index?: number; slide?: number; shape?: number; name?: string; text?: string; word?: string; issue?: string; effect?: string; hidden?: boolean }

export function TaskPane() {
  const { view, query, run, session, t, ask } = useSlides()
  const [items, setItems] = useState<Item[]>([])
  const pane = view.pane
  // A viewer selects and goes to items; the pane's edits are disabled.
  const readOnly = useReadOnly()
  useEffect(() => {
    if (!pane || pane === 'format' || pane === 'background' || pane === 'media' || pane === 'designer' || pane === 'comments') return
    let stale = false
    const command = { animation: 'animation.get', selection: 'slide.inspect', spelling: 'spelling.check', accessibility: 'accessibility.check' }[pane]
    void query<Item[] | { shapes: Item[] }>(command).then((result) => {
      if (!stale) setItems(Array.isArray(result) ? result : result?.shapes ?? [])
    })
    return () => { stale = true }
  }, [pane, query, session.change.revision, session.state?.current])
  if (!pane) return null
  const title = t({ selection: 'selectionPane', animation: 'animationPane', comments: 'showComments', spelling: 'spelling', accessibility: 'accessibility', format: 'formatShape', background: 'background', designer: 'designIdeas', media: 'group.media' }[pane])
  return <aside className="kui-panel slides-task-pane" data-side="right" data-testid={`slides-pane-${pane}`}>
    <div className="slides-pane-header"><strong>{title}</strong><IconButton icon={X} label={t('close')} onClick={() => view.setPane(null)} /></div>
    <div className="slides-pane-body">
      {pane === 'comments' ? <CommentsPane /> : pane === 'media' ? <MediaPlayer /> : pane === 'designer' ? <DesignIdeas /> : pane === 'format' || pane === 'background' ? <CommandTab menu={pane === 'format' ? 'Format Shape' : 'Format Background'} extra={pane === 'format' ? ['Shape Format', 'Arrange', 'Format'] : ['Design']} /> : <>
        {!items.length ? <p>{t(pane === 'spelling' || pane === 'accessibility' ? 'noIssues' : 'empty')}</p> : null}
        {items.map((item, index) => <div className="slides-pane-item" key={item.id ?? index}>
          <strong>{item.name ?? item.effect ?? item.word ?? item.issue}</strong>
          {item.text ? <p>{item.text}</p> : null}
          {item.issue && item.name ? <p>{item.issue}</p> : null}
          <div className="slides-pane-actions">
            {pane === 'selection' ? <><Button onClick={() => void run('edit.select', { ids: [item.id] })}>{t('select')}</Button><Button disabled={readOnly} onClick={() => void run('shape.visible', { ids: [item.id], visible: !!item.hidden })}>{t(item.hidden ? 'hidden' : 'group.show')}</Button></> : null}
            {pane === 'animation' ? <><Button disabled={readOnly} onClick={() => ask('animation.timing', { index })}>{t('group.timing')}</Button><Button disabled={readOnly} onClick={() => void run('animation.remove', { index })}>{t('delete')}</Button></> : null}
            {pane === 'spelling' || pane === 'accessibility' ? <Button onClick={async () => { await run('slide.go', { index: item.slide }); if (item.shape != null) await run('edit.select', { ids: [item.shape] }) }}>{t('goTo')} {(item.slide ?? 0) + 1}</Button> : null}
          </div>
        </div>)}
      </>}
    </div>
  </aside>
}
