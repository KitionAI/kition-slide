/**
 * File › Export: the slides as a PDF, or the current slide (or every slide) as PNG or JPEG
 * pictures at 1, 2 or 3 times their size. The engine draws the file (`file.exportBytes`) and the
 * host writes it as a new file next to the slides; success is shown only once the host answers.
 */
import { useState } from 'react'
import { Button, Choice, Dialog, toast, useKitEditor } from '@kition/sdk/editor'
import { useSlides } from './slides'
import { exportFiles, writeExports, type ExportAnswer, type ExportFormat } from './exporting'

export function ExportDialog({ onClose }: { onClose: () => void }) {
  const { query, host } = useKitEditor()
  const { t, session } = useSlides()
  const current = session.state?.current ?? 0
  const [format, setFormat] = useState<ExportFormat>('pdf')
  const [scope, setScope] = useState<'current' | 'all'>('current')
  const [scale, setScale] = useState('2')
  const [busy, setBusy] = useState(false)
  const picture = format !== 'pdf'

  const run = async () => {
    setBusy(true)
    try {
      const params: Record<string, unknown> = { format }
      if (picture) {
        params.scale = Number(scale)
        if (scope === 'all') params.all = true
        else params.slide = current
      }
      const answer = await query<ExportAnswer>('file.exportBytes', params)
      if (!answer?.data) throw new Error(t('exportNoFile'))
      const written = await writeExports(exportFiles(answer, host.init.document, format), host.exportFile)
      toast('success', t('exported'), written.join(', '))
      onClose()
    } catch (error) {
      toast('error', t('exportFailed'), error instanceof Error ? error.message : String(error))
    } finally {
      setBusy(false)
    }
  }

  return (
    <Dialog
      title={t('export')}
      onClose={onClose}
      testId="slides-export-dialog"
      actions={
        <>
          <Button variant="ghost" onClick={onClose}>
            {t('cancel')}
          </Button>
          <Button variant="primary" disabled={busy} onClick={() => void run()} data-testid="slides-export-run">
            {t('exportRun')}
          </Button>
        </>
      }
    >
      <div className="slides-export-form">
        <Choice label={t('exportFormat')} value={format} choices={[['pdf', 'PDF'], ['png', 'PNG'], ['jpg', 'JPEG']] as const} onChange={setFormat} testId="slides-export-format" />
        {picture ? (
          <>
            <Choice label={t('exportSlides')} value={scope} choices={[['current', t('exportCurrent', { slide: current + 1 })], ['all', t('exportAll')]] as const} onChange={setScope} testId="slides-export-scope" />
            <Choice label={t('exportScale')} value={scale} choices={[['1', '1×'], ['2', '2×'], ['3', '3×']] as const} onChange={setScale} testId="slides-export-scale" />
          </>
        ) : null}
      </div>
    </Dialog>
  )
}
