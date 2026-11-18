// Installs the published @kition/* packages. KITION_LINK=1 links a kition-sdk checkout instead
// (KITION_SDK, default ../kition-sdk) for development; do not commit the lockfile it writes.
const fs = require('node:fs')
const path = require('node:path')

const repo = path.resolve(__dirname, '..')
const sdk = path.resolve(process.env.KITION_SDK ?? path.join(repo, '..', 'kition-sdk'))

const local = {
  '@kition/kit-editor': path.join(sdk, 'packages/kit-editor'),
  '@kition/kit-host': path.join(sdk, 'packages/kit-host'),
  '@kition/kit-ui-protocol': path.join(sdk, 'packages/kit-ui-protocol'),
  '@kition/kit-dev-host': path.join(sdk, 'packages/kit-dev-host'),
}

function readPackage(pkg) {
  if (pkg.name !== 'kition-slide-ui' || process.env.KITION_LINK !== '1') return pkg
  for (const field of ['dependencies', 'devDependencies']) {
    for (const name of Object.keys(pkg[field] ?? {})) {
      const dir = local[name]
      if (dir && fs.existsSync(path.join(dir, 'package.json'))) {
        pkg[field][name] = `link:${path.relative(__dirname, dir).split(path.sep).join('/')}`
      }
    }
  }
  return pkg
}

module.exports = { hooks: { readPackage } }
