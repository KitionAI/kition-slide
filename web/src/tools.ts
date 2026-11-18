import { useKitTools } from '@kition/kit-editor'

/** The engine's active tool and how to choose one, for the ribbon's tool buttons. */
export function useTools() {
  const tools = useKitTools()
  return { active: tools?.active ?? 'select', select: (id: string) => tools?.select(id) }
}
