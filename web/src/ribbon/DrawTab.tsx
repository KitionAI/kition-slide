import { PenIcon as Pen, PencilIcon as Pencil, SplineIcon as Spline, ShapesIcon as Shapes, TypeIcon as Type, SigmaIcon as Sigma } from '@kition/tessera/icons/react'
import { InkGroup, RibbonButton, RibbonGroup } from '@kition/sdk/editor'
import { useSlides } from '../slides'
import { useTools } from '../tools'

/** The shared pens and eraser (`draw.*`, the engine's ink tools), then its freeform tools. */
export function DrawTab() {
  const { t } = useSlides()
  const tools = useTools()
  const tool = (id: string, icon: typeof Pen, label: string, primary = false) => (
    <RibbonButton key={id} primary={primary} icon={icon} label={label} size="large" pressed={tools.active === id} onClick={() => tools.select(id)} data-testid={`slides-tool-${id}`} />
  )
  return (
    <>
      <InkGroup testId="slides-ink" />
      <RibbonGroup label={t('group.pens')} icon={Pen}>
        {tool('freeform', Spline, t('freeform'), true)}
        {tool('scribble', Pencil, t('scribble'))}
      </RibbonGroup>
      <RibbonGroup label={t('group.convert')} icon={Shapes}>
        <RibbonButton icon={Shapes} label={t('inkToShape')} size="large" disabled />
        <RibbonButton icon={Type} label={t('inkToText')} size="large" disabled />
        <RibbonButton icon={Sigma} label={t('inkToMath')} size="large" disabled />
      </RibbonGroup>
    </>
  )
}
