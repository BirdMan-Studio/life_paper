import * as THREE from 'three'
import { terrainKeys } from './terrain'

export function createTerrainMaterial(data = {}) {
  const definitions = data.terrains || []
  const ids = [...new Set([...terrainKeys.map(key => data.generator?.profile?.[key] || key), data.default_terrain || 'grassland', ...(data.terrain_layers || []).map(layer => layer.terrain)])]
  const indexOf = id => Math.max(0, ids.indexOf(id))
  const tables = new THREE.DataTexture(new Uint8Array((data.perlin_permutations || Array.from({ length: 4 }, () => Array(256).fill(0))).flat()), 256, 4, THREE.RedFormat, THREE.UnsignedByteType)
  tables.needsUpdate = true
  const layers = [{ area: data.generator?.area, terrain: data.default_terrain }, ...(data.terrain_layers || [])]
  const regions = new Float32Array(layers.length * 8)
  layers.forEach(({ area, terrain }, index) => {
    const circle = area?.shape === 'circle'
    regions.set([area?.origin?.x ?? area?.center?.x ?? 0, area?.origin?.y ?? area?.center?.y ?? 0, circle ? area.radius_px : area?.size_x_px ?? 0, circle ? 0 : area?.size_y_px ?? 0,
      !area ? -1 : circle ? 1 : area.shape === 'rectangle_frame' ? 2 : 0, area?.thickness_px || 0, indexOf(terrain), 0], index * 8)
  })
  const areas = new THREE.DataTexture(regions, 2, layers.length, THREE.RGBAFormat, THREE.FloatType)
  areas.needsUpdate = true
  const material = new THREE.ShaderMaterial({
    glslVersion: THREE.GLSL3,
    uniforms: {
      permutations: { value: tables }, regions: { value: areas }, regionCount: { value: layers.length },
      hasGenerator: { value: !!data.generator && data.perlin_permutations?.length === 4 },
      noiseScale: { value: Math.max(1, data.generator?.noise_scale_px ?? 384) },
      variation: { value: data.generator?.variation_percent ?? 100 },
      defaultTerrain: { value: indexOf(data.default_terrain || 'grassland') },
      terrainColors: { value: ids.map(id => new THREE.Color(definitions.find(item => item.id === id)?.visual?.color || '#536273')) },
      profile: { value: terrainKeys.map(key => indexOf(data.generator?.profile?.[key] || key)) },
    },
    vertexShader: `
      out vec2 terrainPosition;
      void main() {
        vec4 worldPosition = modelMatrix * vec4(position, 1.0);
        terrainPosition = worldPosition.xy;
        gl_Position = projectionMatrix * viewMatrix * worldPosition;
      }
    `,
    fragmentShader: `
      precision highp float;
      precision highp int;
      uniform sampler2D permutations;
      uniform sampler2D regions;
      uniform int regionCount;
      uniform bool hasGenerator;
      uniform float noiseScale;
      uniform int variation;
      uniform vec3 terrainColors[${ids.length}];
      uniform int profile[7];
      uniform int defaultTerrain;
      in vec2 terrainPosition;
      out vec4 terrainColor;
      int perm(int x, int channel) { return int(round(texelFetch(permutations, ivec2(x & 255, channel), 0).r * 255.0)); }
      float gradient(ivec2 cell, vec2 distance, ivec2 offset, int channel) {
        int hash = perm(perm(cell.x + offset.x, channel) ^ ((cell.y + offset.y) & 255), channel) & 3;
        vec2 d = distance - vec2(offset);
        return ((hash & 1) != 0 ? -d.x : d.x) + ((hash & 2) != 0 ? -d.y : d.y);
      }
      int propertyAt(vec2 tile, int channel) {
        float c = float(channel + 1);
        vec2 point = tile / noiseScale + vec2(c * 17.13, -c * 9.71);
        ivec2 cell = ivec2(floor(point));
        vec2 d = fract(point), f = d*d*d*(d*(d*6.0-15.0)+10.0);
        float value = mix(mix(gradient(cell,d,ivec2(0,0),channel), gradient(cell,d,ivec2(0,1),channel), f.y),
                          mix(gradient(cell,d,ivec2(1,0),channel), gradient(cell,d,ivec2(1,1),channel), f.y), f.x) * sqrt(2.0);
        int sampleValue = int(floor((clamp(value,-1.0,1.0)+1.0)*50.0+0.5));
        return clamp(50 + (sampleValue-50)*variation/100, 0, 100);
      }
      bool containsRegion(vec2 tile, int index) {
        vec4 a = texelFetch(regions, ivec2(0,index),0);
        vec4 b = texelFetch(regions, ivec2(1,index),0);
        vec2 d = tile-a.xy;
        if (b.x < 0.0) return true;
        if (b.x == 1.0) return dot(d,d) <= a.z*a.z;
        bool inside = all(greaterThanEqual(d,vec2(0))) && all(lessThan(d,a.zw));
        if (b.x == 0.0) return inside;
        return inside && min(min(d.x,d.y),min(a.z-1.0-d.x,a.w-1.0-d.y)) < b.y;
      }
      void main() {
        vec2 tile = floor(vec2(terrainPosition.x, -terrainPosition.y));
        int terrain = defaultTerrain;
        if (hasGenerator && containsRegion(tile,0)) {
          int h = propertyAt(tile,0), m = propertyAt(tile,1);
          int category = h <= 10 ? (m <= 90 ? 0 : 1) : h <= 30 ? (m <= 70 ? 2 : m <= 90 ? 3 : 1) : h <= 95 ? (m <= 70 ? 4 : 5) : 6;
          terrain = profile[category];
        }
        for (int i=1; i<regionCount; i++) if (containsRegion(tile,i)) terrain = int(texelFetch(regions,ivec2(1,i),0).z);
        terrainColor = linearToOutputTexel(vec4(terrainColors[terrain], 1.0));
      }
    `,
    depthTest: false,
    depthWrite: false,
  })
  material.addEventListener('dispose', () => { tables.dispose(); areas.dispose() })
  return material
}
