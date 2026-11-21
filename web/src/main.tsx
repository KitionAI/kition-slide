import { createRoot } from 'react-dom/client'
import { connectHost, createKitStrings } from '@kition/sdk/editor'
import '@kition/sdk/editor/styles.css'
import './app.css'
import { App } from './App'
import { CATALOGS } from './strings'
import manifest from '../../kit.json'
import './commandIcons'

const root = createRoot(document.getElementById('root')!)

// Opened outside a host, the page says where it belongs instead of waiting forever.
const alone = window.setTimeout(() => {
  const t = createKitStrings(CATALOGS, navigator.language)
  root.render(<div className="kui-alone">{t('noHost')}</div>)
}, 3000)

void connectHost({ id: 'kit.slide', version: '0.1.3', capabilities: ['ai', 'replica', 'presence', 'comment-anchors'], brand: manifest.ui.brand, display: manifest.ui.display }).then((host) => {
  window.clearTimeout(alone)
  // No StrictMode: its doubled effects would open the document twice, in two engine workers.
  root.render(<App host={host} />)
})
