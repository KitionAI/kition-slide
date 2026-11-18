import { useEffect, useRef, useState } from 'react'
import { useReadOnly } from '@kition/kit-editor'
import { useSlides } from './slides'

const SAVE_MS = 600

/** The current slide's speaker notes, written to the slide a moment after typing stops. */
export function NotesPane() {
  const { notes, run, session, t } = useSlides()
  const index = session.state?.current ?? 0
  // A viewer reads the notes; only an editor writes them.
  const readOnly = useReadOnly()
  const [text, setText] = useState(notes)
  const editing = useRef(false)
  const timer = useRef<number | undefined>(undefined)

  // The engine's notes win unless the person is typing in this pane.
  useEffect(() => {
    if (!editing.current) setText(notes)
  }, [notes, index])

  const commit = (value: string, slide: number) => {
    window.clearTimeout(timer.current)
    if (!readOnly && value !== notes) void run('slide.notes', { text: value, index: slide })
  }

  return (
    <div className="slides-notes" data-testid="slides-notes">
      <textarea
        value={text}
        placeholder={t('notesPlaceholder')}
        aria-label={t('notes')}
        spellCheck={false}
        readOnly={readOnly}
        onFocus={() => {
          editing.current = true
        }}
        onBlur={() => {
          editing.current = false
          commit(text, index)
        }}
        onChange={(event) => {
          const value = event.target.value
          setText(value)
          window.clearTimeout(timer.current)
          timer.current = window.setTimeout(() => commit(value, index), SAVE_MS)
        }}
        data-testid="slides-notes-text"
      />
    </div>
  )
}
