const binaryExtensions = /\.(?:bin|png|jpe?g|gif|webp|ico|pdf|zip|gz|7z|woff2?|ttf|mp[34]|wav|ogg|exe|dll|so|pyc|npy|npz|pt|pth|onnx|pkl|pickle|safetensors)$/i

export async function readProgramFile(blob, filename) {
  const bytes = await blob.arrayBuffer()
  let text = null
  if (!binaryExtensions.test(filename)) {
    try {
      const decoded = new TextDecoder('utf-8', { fatal: true, ignoreBOM: true }).decode(bytes)
      if (!/[\u0000-\u0008\u000b\u000c\u000e-\u001f\u007f]/.test(decoded)) text = decoded
    } catch { /* Invalid UTF-8 remains downloadable as raw bytes. */ }
  }
  return { blob, text }
}

export function programFileBlob(original, text, initialText) {
  return text === initialText ? original : new Blob([text], { type: 'text/plain;charset=utf-8' })
}
