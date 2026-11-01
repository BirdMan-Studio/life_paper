import * as THREE from 'three'
import { SVGLoader } from 'three/addons/loaders/SVGLoader.js'

export function createOrganismMesh(parts) {
  const group = new THREE.Group()
  const loader = new SVGLoader()
  for (const [index, part] of parts.entries()) {
    // Assign attributes through the DOM so catalog values cannot inject SVG markup.
    const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg')
    const path = document.createElementNS(svg.namespaceURI, 'path')
    for (const [name, value] of Object.entries({ d: part.d, transform: part.transform, fill: part.fill, stroke: part.stroke, 'stroke-width': part.strokeWidth })) path.setAttribute(name, value)
    svg.appendChild(path)
    const parsed = loader.parse(new XMLSerializer().serializeToString(svg))
    const add = (geometry, color, offset) => {
      const mesh = new THREE.Mesh(geometry, new THREE.MeshBasicMaterial({ color, side: THREE.DoubleSide }))
      mesh.userData.partOrder = index * 2 + offset
      group.add(mesh)
    }
    for (const shapePath of parsed.paths) {
      const style = shapePath.userData.style
      if (part.fill !== 'none') for (const shape of SVGLoader.createShapes(shapePath)) add(new THREE.ShapeGeometry(shape, 48), part.fill, 0)
      if (part.stroke !== 'none' && part.strokeWidth > 0) for (const subPath of shapePath.subPaths) {
        const geometry = SVGLoader.pointsToStroke(subPath.getPoints(48), style)
        if (geometry) add(geometry, part.stroke, 1)
      }
    }
  }
  group.scale.y = -1
  return group
}
