/**
 * A host that keeps comment threads itself (Kit UI protocol 2.6 section 7k), for
 * tests/comments.mjs: it serves `comment.create` (a new thread per anchor, shown back to the kit
 * with `comments/show` as a host does once the person posts), hears `comments/activated`, and asks
 * `comments/locate`. It keeps the document's replica next to the file (capability `replica`), as a
 * co-editing host keeps its room, so anchors made in one session are found in the next; `reopen`
 * opens the document again, as `viewer` or `commenter` too.
 */
import { mountKit, type KitConnection } from '@kition/kit-host'
import type { KitCommentAnchor, KitShownThread, StoredReplica } from '@kition/kit-ui-protocol'
import { modesInit, roleInit } from '@kition/kit-dev-host'

type CommentsHost = {
  connection: KitConnection | null
  created: KitCommentAnchor[]
  activated: string[]
  threads: KitShownThread[]
  show: (threads: KitShownThread[]) => void
  locate: (anchors: KitCommentAnchor[]) => Promise<boolean[]>
  reopen: (role: string | null) => Promise<void>
}

declare global {
  interface Window {
    commentsHost: CommentsHost
  }
}

const THEME = { colorScheme: 'light' as const, tokens: { '--kit-primary': '#5645d4', '--kit-surface': '#ffffff', '--kit-surface-muted': '#fafaf9', '--kit-canvas': '#f6f5f4', '--kit-text': '#1a1a1a', '--kit-text-muted': '#5d5b54', '--kit-border': '#e5e3df' } }

const host: CommentsHost = {
  connection: null,
  created: [],
  activated: [],
  threads: [],
  show: (threads) => {
    host.threads = threads
    host.connection?.showComments(threads)
  },
  locate: (anchors) => host.connection?.locateComments(anchors) ?? Promise.resolve(anchors.map(() => false)),
  reopen: (role) => open(role),
}
window.commentsHost = host

const status = document.getElementById('status') as HTMLElement
const frame = document.getElementById('kit') as HTMLIFrameElement
const params = new URLSearchParams(location.search)
const path = 'Sample.kslide'
const files = new Map<string, { bytes: ArrayBuffer; version: number }>()
const replicas = new Map<string, StoredReplica>()

async function open(role: string | null) {
  host.connection?.close()
  status.textContent = 'opening…'
  if (!files.has(path)) files.set(path, { bytes: await (await fetch(new URL('../../sample/Sample.kslide', import.meta.url))).arrayBuffer(), version: 1 })
  const file = files.get(path)!
  frame.src = `${new URL('../../index.html', import.meta.url).href}?${Date.now()}`
  host.connection = await mountKit(frame, {
    origin: location.origin,
    host: { name: 'Comments test host', version: '1' },
    document: { path, name: path, bytes: file.bytes.slice(0), version: String(file.version), ...roleInit(role) },
    theme: THEME,
    locale: 'en-US',
    shortcuts: [],
    ui: { modes: modesInit(params.get('modes')) },
    handlers: {
      read: ({ path: wanted }) => {
        const found = files.get(wanted)
        if (!found) throw new Error(`${wanted} is not in the workspace`)
        return { bytes: found.bytes.slice(0), version: String(found.version), size: found.bytes.byteLength }
      },
      stamp: ({ path: wanted }) => ({ version: files.has(wanted) ? String(files.get(wanted)!.version) : null }),
      write: ({ path: wanted, bytes }) => {
        const version = (files.get(wanted)?.version ?? 0) + 1
        files.set(wanted, { bytes, version })
        return { path: wanted, version: String(version) }
      },
      changed: () => {},
      replicaRead: ({ path: wanted }) => replicas.get(wanted) ?? null,
      replicaWrite: ({ path: wanted, ...stored }) => {
        replicas.set(wanted, stored)
      },
      services: {
        'comment.create': ({ anchor }) => {
          host.created.push(anchor)
          const id = `thread-${host.created.length}`
          host.show([...host.threads, { id, anchor, resolved: false, count: 1 }])
          return { thread: id }
        },
      },
      comments: { activated: (id) => host.activated.push(id) },
    },
  })
  status.textContent = `${path} open in ${host.connection.kit.id}`
}

open(params.get('role')).catch((error: unknown) => {
  status.textContent = `failed: ${error instanceof Error ? error.message : String(error)}`
})
