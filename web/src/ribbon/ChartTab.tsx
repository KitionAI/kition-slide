import { useEffect, useState } from 'react'
import { ChartDesignGroups, useKitEditor } from '@kition/sdk/editor'
import { ContextTab } from './ContextTab'

/**
 * Chart Design for the selected chart: the shared chart controls (type, elements, data, through
 * `drawing.chart`), then any other Chart Design command the engine adds (the rest are under More).
 */
export function ChartTab() {
  const editor = useKitEditor()
  const revision = editor.session.change?.revision
  const [id, setId] = useState<string | null>(null)
  useEffect(() => {
    let stale = false
    void editor.query<{ objects?: Array<{ id: string; kind: string }> }>('drawing.describe', {}).then((answer) => {
      if (!stale) setId(answer?.objects?.find((object) => object.kind === 'chart')?.id ?? null)
    })
    return () => {
      stale = true
    }
  }, [editor, revision])
  return (
    <>
      {id ? <ChartDesignGroups id={id} testId="slides-chart" /> : null}
      <ContextTab menu="Chart Design" skip={['chart.type', 'chart.data', 'chart.options']} />
    </>
  )
}
