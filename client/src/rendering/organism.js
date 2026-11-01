// Geometry uses game pixels, with X right and Y down, in both views.
export function capsulePoint(body, perimeterPpm) {
  const radius = body.size.width_px / 2
  const straight = Math.max(0, body.size.length_px - body.size.width_px)
  const perimeter = 2 * straight + 2 * Math.PI * radius
  let distance = ((perimeterPpm % 1_000_000 + 1_000_000) % 1_000_000) / 1_000_000 * perimeter
  let x, y, angle
  if (distance <= straight) { x = -straight / 2 + distance; y = -radius; angle = -90 }
  else if ((distance -= straight) <= Math.PI * radius) {
    angle = -90 + distance / radius * 180 / Math.PI
    x = straight / 2 + Math.cos(angle * Math.PI / 180) * radius
    y = Math.sin(angle * Math.PI / 180) * radius
  } else if ((distance -= Math.PI * radius) <= straight) { x = straight / 2 - distance; y = radius; angle = 90 }
  else {
    distance -= straight
    angle = 90 + distance / radius * 180 / Math.PI
    x = -straight / 2 + Math.cos(angle * Math.PI / 180) * radius
    y = Math.sin(angle * Math.PI / 180) * radius
  }
  const rotation = (body.rotationMdeg || 0) / 1000
  const radians = rotation * Math.PI / 180
  return { x: body.x + x * Math.cos(radians) - y * Math.sin(radians), y: body.y + x * Math.sin(radians) + y * Math.cos(radians), angle: angle + rotation }
}

export function interiorPosition(body, index) {
  return { x: body.x - 12 + (index % 3) * 12, y: body.y + (Math.floor(index / 3) % 2) * 11 - 5.5 }
}

export function externalPath(node, body, offset = 0) {
  if (!body) return `M ${node.x - node.size.length_px / 2} ${node.y} h ${node.size.length_px}`
  const perimeter = 2 * Math.max(0, body.size.length_px - body.size.width_px) + Math.PI * body.size.width_px
  if (!perimeter) return ''
  const wrap = node.model.assembly === 'wrap'
  const anchor = wrap ? 0 : node.anchorPpm ?? 0
  const center = wrap ? body : capsulePoint(body, anchor)
  const span = wrap ? perimeter : node.size.length_px
  const steps = Math.max(8, Math.ceil(span / (node.displayScale || 1)))
  const points = Array.from({ length: steps + 1 }, (_, index) => {
    const distance = wrap ? index / steps * span : (index / steps - 0.5) * span
    const point = capsulePoint(body, anchor + distance / perimeter * 1_000_000)
    const radians = point.angle * Math.PI / 180
    return `${point.x + node.x - center.x + Math.cos(radians) * offset} ${point.y + node.y - center.y + Math.sin(radians) * offset}`
  })
  return `M ${points.join(' L ')}${wrap ? ' Z' : ''}`
}

export function solidPath(node) {
  const width = node.size.length_px
  const height = node.size.width_px
  if (node.model.geometry === 'ellipse') return `M ${-width / 2} 0 A ${width / 2} ${height / 2} 0 1 0 ${width / 2} 0 A ${width / 2} ${height / 2} 0 1 0 ${-width / 2} 0 Z`
  const radius = node.model.geometry === 'rectangle' ? Math.min(2, width / 2, height / 2) : Math.min(width, height) / 2
  const x = width / 2, y = height / 2, r = radius
  return `M ${-x + r} ${-y} H ${x - r} A ${r} ${r} 0 0 1 ${x} ${-y + r} V ${y - r} A ${r} ${r} 0 0 1 ${x - r} ${y} H ${-x + r} A ${r} ${r} 0 0 1 ${-x} ${y - r} V ${-y + r} A ${r} ${r} 0 0 1 ${-x + r} ${-y} Z`
}

export function renderPart(node, body) {
  const appearance = node.model.appearance || {}
  const external = node.category === 'external_organ'
  return {
    id: node.id,
    d: external ? externalPath(node, body, appearance.protrusion_px || 0) : solidPath(node),
    transform: external ? '' : `translate(${node.x} ${node.y}) rotate(${(node.rotationMdeg || 0) / 1000})`,
    fill: external ? 'none' : appearance.fill || 'none',
    stroke: appearance.stroke || 'none',
    strokeWidth: appearance.stroke_width_px ?? 1,
    order: node.category === 'body' ? 0 : !external ? 1 : node.model.assembly === 'wrap' ? 2 : 3,
  }
}

export function organismRenderParts(composition, definitions) {
  const components = Object.values(composition.components || {})
  const nodes = components.flatMap(component => {
    const definition = definitions.get(component.component || component.component_id)
    if (!definition?.model) return []
    const placement = component.placement || {}
    return [{ id: component.id, category: definition.category, model: definition.model,
      size: placement.size || definition.model.size, x: placement.position?.x ?? 0, y: placement.position?.y ?? 0,
      rotationMdeg: placement.rotation_mdeg || 0, hostBody: placement.host_body,
      anchorPpm: placement.surface_anchor?.perimeter_ppm, hasPosition: !!placement.position }]
  })
  const byId = new Map(nodes.map(node => [node.id, node]))
  const root = byId.get(composition.root_body) || nodes.find(node => node.category === 'body')
  const origin = { x: root?.x || 0, y: root?.y || 0 }
  const interiorCounts = new Map()
  for (const node of nodes) {
    const body = byId.get(node.hostBody)
    if (!body) continue
    if (node.model.assembly === 'wrap') { node.x = body.x; node.y = body.y }
    if (node.model.assembly === 'interior') {
      const index = interiorCounts.get(body.id) || 0
      if (!node.hasPosition) Object.assign(node, interiorPosition(body, index))
      interiorCounts.set(body.id, index + 1)
    }
    if (node.model.assembly === 'surface' && node.anchorPpm != null) Object.assign(node, capsulePoint(body, node.anchorPpm))
  }
  for (const node of nodes) { node.x -= origin.x; node.y -= origin.y }
  return nodes.map(node => renderPart(node, byId.get(node.hostBody))).sort((a, b) => a.order - b.order)
}
