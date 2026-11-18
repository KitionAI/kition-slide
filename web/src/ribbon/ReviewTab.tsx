import { AccessibilityIcon as Accessibility, LanguagesIcon as Languages, MessageSquareIcon as MessageSquare, MessageSquarePlusIcon as MessageSquarePlus, SpellCheckIcon as SpellCheck, TrashIcon as Trash2, BookOpenIcon as BookOpen } from '@kition/tessera-icons/react'
import { RibbonButton, RibbonGroup } from '@kition/kit-editor'
import { useSlides } from '../slides'
import { CommandButton } from './common'

export function ReviewTab() {
  const { view, t, newComment, canNewComment, commenter } = useSlides()
  return <>
    <RibbonGroup label={t('group.proofing')} icon={SpellCheck}>
      <RibbonButton primary icon={SpellCheck} label={t('spelling')} size="large" onClick={() => view.setPane('spelling')} />
      <RibbonButton icon={BookOpen} label={t('thesaurus')} size="large" disabled />
    </RibbonGroup>
    <RibbonGroup label={t('group.accessibility')} icon={Accessibility}><RibbonButton primary icon={Accessibility} label={t('accessibility')} size="large" onClick={() => view.setPane('accessibility')} /></RibbonGroup>
    <RibbonGroup label={t('group.language')} icon={Languages}>
      <RibbonButton icon={Languages} label={t('translate')} size="large" disabled />
      <RibbonButton icon={Languages} label={t('language')} size="large" disabled />
    </RibbonGroup>
    <RibbonGroup label={t('group.comments')} icon={MessageSquare}>
      <RibbonButton primary icon={MessageSquarePlus} label={t('comment')} size="large" disabled={!canNewComment} onClick={() => void newComment()} data-testid="slides-new-comment" />
      {commenter ? null : <CommandButton primary command="comment.delete" icon={Trash2} label={t('delete')} size="large" params={{ all: true }} />}
      <RibbonButton primary icon={MessageSquare} label={t('showComments')} size="large" pressed={view.pane === 'comments'} onClick={() => view.setPane(view.pane === 'comments' ? null : 'comments')} />
    </RibbonGroup>
  </>
}
