import { DownloadIcon as Download } from '@kition/tessera-icons/react'
import { RibbonButton, RibbonGroup } from '@kition/kit-editor'
import { useSlides } from '../slides'

/** File: Export, the slides as a PDF or pictures written next to them (Save is the host's own). */
export function FileTab({ onExport }: { onExport: () => void }) {
  const { t, session } = useSlides()
  return (
    <RibbonGroup label={t('group.export')} icon={Download}>
      <RibbonButton primary icon={Download} label={t('export')} size="large" disabled={!session.doc} onClick={onExport} data-testid="slides-export" />
    </RibbonGroup>
  )
}
