import { useMemo } from 'react'
import { CommentsPanel, type CommentsPanelProps } from '@kition/kit-editor'
import { useSlides, type CommentAnchor } from './slides'

/**
 * The comments pane (Kit Standard 2.0 rule M23): the slide's threads in kit-editor's `CommentsPanel`.
 * A person who may comment opens threads on the selected shape or the slide, replies, resolves,
 * and deletes their own comments; who may do what comes from the host's `document.comments`.
 */
export function CommentsPane() {
  const slides = useSlides()
  const { t, threads, comment, access, activeThread, setActiveThread, pendingComment, setPendingComment, run } = slides
  const labels = useMemo(
    () => ({
      title: t('comments.title'),
      empty: t('comments.empty'),
      newComment: t('comments.new'),
      placeholder: t('comments.placeholder'),
      replyPlaceholder: t('comments.replyPlaceholder'),
      comment: t('comment'),
      reply: t('reply'),
      cancel: t('cancel'),
      resolve: t('resolve'),
      reopen: t('comments.reopen'),
      delete: t('delete'),
      resolved: t('comments.resolved'),
      readOnlyHistory: t('comments.history'),
      you: t('comments.you'),
    }),
    [t],
  )
  const anchorLabel = (anchor: Record<string, unknown>) =>
    typeof anchor.slide === 'number' ? t(anchor.shape != null ? 'comments.onShape' : 'comments.onSlide', { slide: anchor.slide + 1 }) : null
  // A failed write (refused, or the document closed) leaves the threads as the engine has them.
  const write = (id: Parameters<typeof comment>[0], params: Record<string, unknown>) => comment(id, params).catch(() => null)
  return (
    <CommentsPanel
      threads={threads as unknown as CommentsPanelProps['threads']}
      me={access.me}
      canComment={access.canComment}
      othersReadOnly={access.othersReadOnly}
      pendingAnchor={pendingComment as CommentsPanelProps['pendingAnchor']}
      activeThread={activeThread}
      labels={labels}
      anchorLabel={anchorLabel as unknown as CommentsPanelProps['anchorLabel']}
      testId="slides-comments"
      onActivate={(id) => {
        setActiveThread(id)
        const thread = threads.find((item) => item.id === id)
        if (thread && thread.slide !== slides.session.state?.current) void run('slide.go', { index: thread.slide })
      }}
      onCancelNew={() => setPendingComment(null)}
      onAdd={async (anchor, body) => {
        const at = anchor as CommentAnchor
        const added = await write('comment.add', { text: body, slide: at.slide, x: at.x, y: at.y, ...(at.shape != null ? { id: at.shape } : {}) })
        setPendingComment(null)
        if (added?.thread) setActiveThread(added.thread)
      }}
      onReply={(thread, body) => void write('comment.reply', { thread, text: body })}
      onResolve={(thread, resolved) => void write('comment.resolve', { thread, resolved })}
      onDelete={(thread, id) => void write('comment.delete', { thread, comment: id })}
    />
  )
}
