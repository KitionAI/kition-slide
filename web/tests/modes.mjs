// The editor in each display mode, through the development host: node tests/modes.mjs [chromium|webkit]
import { chromium, webkit } from 'playwright'
import { createServer } from 'vite'
import { runModeSmoke } from '@kition/sdk/dev-host/mode-smoke'

await runModeSmoke({
  kit: 'kit.slide',
  config: new URL('../vite.config.ts', import.meta.url).pathname,
  browser: process.argv[2] === 'webkit' ? webkit : chromium,
  createServer,
  line: { tab: 'home', primary: ['slides-cmd-edit.paste', 'slides-cmd-slide.new', 'slides-font', 'slides-font-size', 'slides-cmd-format.bold', 'slides-font-color', 'slides-shapes', 'slides-tool-textBox'] },
})
