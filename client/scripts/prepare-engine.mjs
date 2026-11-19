import { execFileSync } from 'node:child_process'
import { copyFileSync, mkdirSync, chmodSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import path from 'node:path'

const root = fileURLToPath(new URL('../../', import.meta.url))
const target = process.env.TAURI_ENV_TARGET_TRIPLE || execFileSync('rustc', ['-vV'], { encoding: 'utf8' }).match(/^host: (.+)$/m)?.[1]
if (!target || !/^[\w.-]+$/.test(target)) throw new Error('Missing or invalid Rust target triple')
const debug = process.env.TAURI_ENV_DEBUG === 'true'
const targetDir = path.join(root, 'target', 'desktop-engine')
execFileSync('cargo', ['build', '-p', 'server', '--bin', 'game-server', '--target', target, '--target-dir', targetDir, ...(debug ? [] : ['--release'])], {
  cwd: root, stdio: 'inherit', windowsHide: true,
})
const extension = target.includes('windows') ? '.exe' : ''
const output = path.join(root, 'client', 'src-tauri', 'binaries', `game-server-${target}${extension}`)
mkdirSync(path.dirname(output), { recursive: true })
copyFileSync(path.join(targetDir, target, debug ? 'debug' : 'release', `game-server${extension}`), output)
if (!extension) chmodSync(output, 0o755)
console.log(`Prepared local engine sidecar: ${output}`)
