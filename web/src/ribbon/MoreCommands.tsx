import { EllipsisIcon as Ellipsis } from '@kition/tessera-icons/react'
import { CommandItem, MenuSeparator, RibbonGroup, RibbonMenu } from '@kition/kit-editor'
import { useSlides } from '../slides'

/** Complete access to engine menu paths, including commands omitted by a compact gallery. */
export function MoreCommands({ menus }: { menus: readonly string[] }) {
  const { commands, t } = useSlides()
  const groups = new Map<string, typeof commands>()
  for (const command of commands) {
    if (!menus.includes(command.menu[0])) continue
    const group = command.menu.slice(0, 2).join(' / ')
    groups.set(group, [...(groups.get(group) ?? []), command])
  }
  if (!groups.size) return null
  return <RibbonGroup label={t('group.commands')} icon={Ellipsis}>
    <RibbonMenu icon={Ellipsis} label={t('more')} size="large" testId="slides-more-commands" primary>
      {[...groups].map(([group, items], index) => <div key={group}>
        {index > 0 ? <MenuSeparator /> : null}
        <div className="slides-menu-heading">{group}</div>
        {items.map((command) => <CommandItem key={command.id} command={command.id} />)}
      </div>)}
    </RibbonMenu>
  </RibbonGroup>
}
