/**
 * Export's file handling, apart from the dialog: the engine answers the file as base64
 * (`file.exportBytes`), and the host writes it as a new file next to the slides (`host/write`,
 * mode `export`). Nothing here reaches the engine or the host, so it is tested on its own.
 */

export type ExportFormat = 'pdf' | 'png' | 'jpg'

/** What `file.exportBytes` answers: the first file, and with `all` every slide's. */
export type ExportAnswer = { data: string; format?: string; files?: Array<{ slide: number; data: string }> }

/** One file to write: its workspace path and bytes. */
export type ExportFile = { path: string; bytes: Uint8Array }

export function decodeBase64(base64: string): Uint8Array {
  return Uint8Array.from(atob(base64), (char) => char.charCodeAt(0))
}

/** The document's name without its extension, safe in a file name. */
export function exportStem(name: string): string {
  return name.replace(/\.[^.]+$/, '').replace(/[\\/:*?"<>|]/g, '_').trim() || 'Slides'
}

/** The folder of the document's workspace path, with its trailing slash ('' at the top). */
export function exportFolder(path: string): string {
  return path.includes('/') ? path.slice(0, path.lastIndexOf('/') + 1) : ''
}

/**
 * The files an answer makes: one (`Name.pdf`, `Name.png`), or one per slide numbered from 1
 * (`Name 1.png`, `Name 2.png`) when it lists every slide's.
 */
export function exportFiles(answer: ExportAnswer, document: { name: string; path: string }, format: ExportFormat): ExportFile[] {
  const at = `${exportFolder(document.path)}${exportStem(document.name)}`
  if (answer.files && answer.files.length > 1) return answer.files.map((file) => ({ path: `${at} ${file.slide + 1}.${format}`, bytes: decodeBase64(file.data) }))
  return [{ path: `${at}.${format}`, bytes: decodeBase64(answer.data) }]
}

/**
 * Writes each file through the host, in order, and answers the paths the host wrote (it may pick
 * an unused name). The host's refusal ends the export with its own error.
 */
export async function writeExports(files: readonly ExportFile[], write: (path: string, bytes: Uint8Array) => Promise<string>): Promise<string[]> {
  const written: string[] = []
  for (const file of files) written.push(await write(file.path, file.bytes))
  return written
}
