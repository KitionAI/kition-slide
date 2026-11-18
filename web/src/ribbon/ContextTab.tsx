import { AlignHorizontalJustifyCenterIcon as AlignHorizontalJustifyCenter, BringToFrontIcon as BringToFront, CropIcon as Crop, GroupIcon as Group, LayersIcon as Layers, PaintBucketIcon as PaintBucket, RotateCwIcon as RotateCw, RulerIcon as Ruler, SendToBackIcon as SendToBack, SlidersHorizontalIcon as SlidersHorizontal, TextCursorInputIcon as TextCursorInput, UngroupIcon as Ungroup, type IconComponent } from '@kition/tessera-icons/react'
import { CommandButton, DrawingWarpMenu, RibbonGroup, useKitEditor } from '@kition/kit-editor'
import { useSlides } from '../slides'

const ICONS: Array<[string, IconComponent]> = [['bringforward', BringToFront], ['sendbackward', SendToBack], ['ungroup', Ungroup], ['group', Group], ['arrange.align', AlignHorizontalJustifyCenter], ['fill', PaintBucket], ['crop', Crop], ['rotate', RotateCw], ['size', Ruler], ['text', TextCursorInput], ['arrange', Layers]]

/** A command's icon here, by a word of its id; none for a command no word matches. */
const commandIconOf = (id: string) => ICONS.find(([key]) => id.toLowerCase().includes(key))?.[1]

/** Contextual ribbons retain every command the engine publishes for the selected object. */
export function ContextTab({ menu, extra = [], skip = [], primary = [] }: { menu: string; extra?: string[]; skip?: string[]; primary?: readonly string[] }) {
  const { commands, phrase } = useSlides()
  const editor = useKitEditor()
  const groups = new Map<string, typeof commands>()
  for (const command of commands) {
    if (![menu, ...extra].includes(command.menu[0]) || skip.includes(command.id)) continue
    const group = command.menu[1] ?? command.menu[0]
    groups.set(group, [...(groups.get(group) ?? []), command])
  }
  // A group's icon (shown when Classic collapses it): its first command's that has one.
  const groupIcon = (items: typeof commands) => items.map((command) => commandIconOf(command.id)).find((icon) => icon != null) ?? SlidersHorizontal
  return <>{[...groups].map(([group, items]) => <RibbonGroup key={group} label={phrase(group)} icon={groupIcon(items)}>
    <div className="kui-command-grid">{items.map((command) => {
      // Art text's Transform: the presets the kits share, for the selected shapes.
      if (command.id === 'format.transform') {
        return <DrawingWarpMenu key={command.id} testId="slide-drawing" disabled={!editor.enabled(command.id)} onPick={(preset) => void editor.run(command.id, { preset: preset ?? 'none' })} />
      }
      const icon = commandIconOf(command.id)
      // In Single line's row a command without an icon of its own keeps its label beside the generic one.
      return <CommandButton key={command.id} command={command.id} icon={icon ?? SlidersHorizontal} primary={primary.includes(command.id)} lineLabel={!icon} />
    })}</div>
  </RibbonGroup>)}</>
}
