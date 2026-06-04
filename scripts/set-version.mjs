/**
 * set-version.mjs
 * 同步更新三處版本號：package.json、src-tauri/tauri.conf.json、src-tauri/Cargo.toml
 *
 * Usage: npm run version:set -- <semver>
 * Example: npm run version:set -- 0.2.0
 */

import fs from 'node:fs'
import path from 'node:path'

const version = process.argv[2]

if (!version || !/^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$/.test(version)) {
  console.error('Usage: npm run version:set -- <semver>')
  console.error('Example: npm run version:set -- 0.2.0')
  process.exit(1)
}

const root = process.cwd()

function writeJson(relativePath, updater) {
  const filePath = path.join(root, relativePath)
  const data = JSON.parse(fs.readFileSync(filePath, 'utf8'))
  updater(data)
  fs.writeFileSync(filePath, `${JSON.stringify(data, null, 2)}\n`)
}

writeJson('package.json', (data) => {
  data.version = version
})

writeJson('src-tauri/tauri.conf.json', (data) => {
  data.version = version
})

const cargoPath = path.join(root, 'src-tauri/Cargo.toml')
const cargo = fs.readFileSync(cargoPath, 'utf8')
fs.writeFileSync(
  cargoPath,
  cargo.replace(/^version = ".*"$/m, `version = "${version}"`),
)

console.log(`AMAGI Core version set to ${version}`)
