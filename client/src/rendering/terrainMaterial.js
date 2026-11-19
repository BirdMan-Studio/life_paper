import * as THREE from 'three'
import { terrainKeys } from './terrain'

export function createTerrainMaterial(data = {}) {
  const definitions = data.terrains || []
  const ids = [...new Set([...terrainKeys.map(key => data.generator?.profile?.[key] || key), data.default_terrain || 'grassland', ...(data.terrain_layers || []).map(layer => layer.terrain)])]
  const indexOf = id => Math.max(0, ids.indexOf(id))
  const patterns = ['dust', 'grass', 'ripples', 'waves', 'blocks', 'rock', 'mud', 'reeds', 'mist']
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
      terrainStyles: { value: ids.map(id => patterns.indexOf(definitions.find(item => item.id === id)?.visual?.pattern)) },
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
      uniform int terrainStyles[${ids.length}];
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
      float regionInterior(vec2 point, int index) {
        vec4 a = texelFetch(regions, ivec2(0,index),0);
        vec4 b = texelFetch(regions, ivec2(1,index),0);
        if (b.x < 0.0) return 1000.0;
        if (b.x == 1.0) return a.z - length(point-a.xy);
        vec2 d = point-a.xy;
        float edge = min(min(d.x,d.y),min(a.z-d.x,a.w-d.y));
        return b.x == 2.0 ? min(edge, b.y-edge) : edge;
      }
      float hash21(vec2 p) {
        vec3 p3 = fract(vec3(p.xyx) * 0.1031);
        p3 += dot(p3, p3.yzx + 33.33);
        return fract((p3.x + p3.y) * p3.z);
      }
      float textureNoise(vec2 p) {
        vec2 cell = floor(p), f = fract(p);
        f = f*f*(3.0-2.0*f);
        return mix(mix(hash21(cell), hash21(cell+vec2(1,0)), f.x),
                   mix(hash21(cell+vec2(0,1)), hash21(cell+vec2(1,1)), f.x), f.y);
      }
      vec3 surfaceColor(vec3 base, int style, vec2 p, float edge) {
        if (style < 0) return base;
        float pixel = max(fwidth(p.x), fwidth(p.y));
        float detail = 1.0-smoothstep(1.5,5.0,pixel);
        float broad = textureNoise(p*0.018)-0.5;
        float fine = textureNoise(p*0.11)-0.5;
        vec3 color = base * (1.0 + broad*0.22 + fine*0.08);
        if (style == 1 || style == 7) {
          vec2 cell = floor(p/5.0), local = fract(p/5.0);
          float random = hash21(cell);
          float blade = (1.0-smoothstep(0.035,0.09,abs(local.x-0.5-(local.y-0.5)*0.24)))
            * smoothstep(0.18,0.35,local.y) * (1.0-smoothstep(0.58,0.8,local.y));
          color *= 1.0-blade*step(style == 7 ? 0.36 : 0.73,random)*detail*0.16;
          color += vec3(0.025,0.035,0.008)*broad;
        } else if (style == 2 || style == 3) {
          float ripple = pow(max(0.0,sin(p.y*1.15+textureNoise(p*0.075)*3.0)),14.0);
          float patches = smoothstep(0.45,0.7,textureNoise(p*vec2(0.085,0.12)));
          color += vec3(0.028,0.045,0.045)*ripple*patches*detail;
          float shore = 1.0-smoothstep(0.0,5.0,edge);
          color = mix(color, color*0.76+vec3(0.045,0.07,0.055),shore*0.65);
        } else if (style == 4 || style == 5) {
          float facet = textureNoise(p*0.16);
          color *= 0.87+facet*0.24;
          vec2 joint = abs(fract(p/vec2(12,8)+vec2(floor(p.y/8.0)*0.5,0))-0.5);
          if (style == 4) color *= 1.0-smoothstep(0.41,0.49,max(joint.x,joint.y))*0.2*detail;
          color *= 0.77+0.23*smoothstep(0.0,3.0,edge);
        } else {
          float grain = hash21(floor(p*0.8));
          color *= 1.0+(grain-0.5)*0.1*detail;
        }
        return color;
      }
      void main() {
        vec2 point = vec2(terrainPosition.x, -terrainPosition.y);
        vec2 tile = floor(point);
        int terrain = defaultTerrain;
        float edge = 1000.0;
        if (hasGenerator && containsRegion(tile,0)) {
          int h = propertyAt(tile,0), m = propertyAt(tile,1);
          int category = h <= 10 ? (m <= 90 ? 0 : 1) : h <= 30 ? (m <= 70 ? 2 : m <= 90 ? 3 : 1) : h <= 95 ? (m <= 70 ? 4 : 5) : 6;
          terrain = profile[category];
        }
        for (int i=1; i<regionCount; i++) if (containsRegion(tile,i)) {
          int nextTerrain = int(texelFetch(regions,ivec2(1,i),0).z);
          float interior = regionInterior(point,i);
          edge = nextTerrain == terrain ? max(edge,interior) : interior;
          terrain = nextTerrain;
        }
        vec3 color = surfaceColor(terrainColors[terrain], terrainStyles[terrain], point,edge);
        terrainColor = linearToOutputTexel(vec4(color, 1.0));
      }
    `,
    depthTest: false,
    depthWrite: false,
  })
  material.addEventListener('dispose', () => { tables.dispose(); areas.dispose() })
  return material
}
