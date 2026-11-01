export const terrainKeys = ['cliff', 'pond', 'valley', 'swamp', 'grassland', 'wetland', 'wall']

export function contains(area, x, y) {
  if (!area) return true
  if (area.shape === 'circle') return (x - area.center.x) ** 2 + (y - area.center.y) ** 2 <= area.radius_px ** 2
  const dx = x - area.origin.x, dy = y - area.origin.y
  const inside = dx >= 0 && dy >= 0 && dx < area.size_x_px && dy < area.size_y_px
  if (area.shape === 'rectangle') return inside
  return inside && Math.min(dx, dy, area.size_x_px - 1 - dx, area.size_y_px - 1 - dy) < area.thickness_px
}

// noise-rs 0.9: XOR permutation hashing, diagonal gradients, quintic interpolation.
function perlin(x, y, table) {
  const ix = Math.floor(x), iy = Math.floor(y), dx = x - ix, dy = y - iy
  const gradient = (ox, oy) => {
    const hash = table[table[(ix + ox) & 255] ^ ((iy + oy) & 255)] & 3
    return (hash & 1 ? -(dx - ox) : dx - ox) + (hash & 2 ? -(dy - oy) : dy - oy)
  }
  const fade = t => t * t * t * (t * (t * 6 - 15) + 10)
  const mix = (a, b, t) => a + t * (b - a)
  return Math.max(-1, Math.min(1, mix(mix(gradient(0, 0), gradient(0, 1), fade(dy)), mix(gradient(1, 0), gradient(1, 1), fade(dy)), fade(dx)) * Math.SQRT2))
}

export function createTerrainSampler(data = {}) {
  const config = data.generator
  const tables = data.perlin_permutations || []
  return (x, y) => {
    const properties = {}
    for (const [index, name] of ['height', 'moisture', 'nutrients', 'temperature'].entries()) {
      const channel = index + 1
      const value = config && tables[index] ? Math.round((perlin(x / Math.max(1, config.noise_scale_px ?? 384) + channel * 17.13, y / Math.max(1, config.noise_scale_px ?? 384) - channel * 9.71, tables[index]) + 1) * 50) : 0
      properties[name] = config && tables[index] ? Math.max(0, Math.min(100, 50 + Math.trunc((value - 50) * (config.variation_percent ?? 100) / 100))) : 0
    }
    let key = data.default_terrain || 'grassland'
    if (config && tables.length === 4 && contains(config.area, x, y)) {
      const h = properties.height, m = properties.moisture
      const category = h <= 10 ? m <= 90 ? 'cliff' : 'pond' : h <= 30 ? m <= 70 ? 'valley' : m <= 90 ? 'swamp' : 'pond' : h <= 95 ? m <= 70 ? 'grassland' : 'wetland' : 'wall'
      key = config.profile?.[category] || category
    }
    for (const layer of data.terrain_layers || []) if (contains(layer.area, x, y)) key = layer.terrain
    return { key, ...properties }
  }
}
