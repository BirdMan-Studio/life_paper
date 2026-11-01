const { chromium } = require(process.env.PLAYWRIGHT_MODULE || 'playwright');
const assert = require('node:assert/strict');

(async () => {
  const browser = await chromium.launch({ channel: 'msedge', headless: true });
  try {
    const page = await browser.newPage({ viewport: { width: 1440, height: 960 } });
    const errors = [];
    page.on('pageerror', error => errors.push(error.message));
    await page.goto(process.env.WORLD_TEST_URL || 'http://127.0.0.1:1421/');
    const result = await page.evaluate(async () => {
      const source = await (await fetch('/src/components/OrganismAssembler.vue')).text();
      const { createApp, h, ref, nextTick } = await import(source.match(/from\s+["']([^"']*\/vue\.js[^"']*)["']/)[1]);
      const { default: Assembler } = await import('/src/components/OrganismAssembler.vue');
      const { organismRenderParts } = await import('/src/rendering/organism.js');
      const { createOrganismMesh } = await import('/src/rendering/organismThree.js');
      const THREE = await import('/node_modules/.vite/deps/three.js');
      const definitions = ['capsule', 'surface', 'host_outline', 'rectangle', 'ellipse'].map((geometry, i) => ({
        id: `part${i}`, name: geometry, category: i === 0 ? 'body' : i < 3 ? 'external_organ' : 'internal_organ',
        slots: { counts: { body: 1, external_organ: 4, internal_organ: 4 } },
        model: { geometry, assembly: ['free', 'surface', 'wrap', 'interior', 'interior'][i],
          size: { width_px: i === 0 ? 40 : i < 3 ? 0 : 8, length_px: i === 0 ? 80 : i < 3 ? 15 : 12 },
          appearance: { fill: ['#234339', 'none', 'none', '#cc6633', '#aa66bb'][i], stroke: ['#668e70', '#ffdd33', '#7eab86', '#ffffff', '#ffffff'][i], stroke_width_px: 2, protrusion_px: 1 } },
      }));
      const host = document.createElement('div'); host.className = 'builder-page'; document.body.replaceChildren(host);
      const instance = ref(); createApp({ render: () => h(Assembler, { components: definitions, ref: instance }) }).mount(host);
      for (const definition of definitions) instance.value.addComponent(definition.id);
      await nextTick();
      const payload = instance.value.buildPayload();
      const composition = { root_body: payload.components[0].id, components: payload.components };
      const parts = organismRenderParts(composition, new Map(definitions.map(d => [d.id, d])));
      const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
      svg.setAttribute('width', '256'); svg.setAttribute('height', '192'); svg.setAttribute('viewBox', '-64 -48 128 96');
      const sceneGroup = document.createElementNS(svg.namespaceURI, 'g');
      sceneGroup.setAttribute('transform', 'scale(0.5) translate(-360 -215)'); svg.appendChild(sceneGroup);
      for (const node of host.querySelectorAll('.assembly-node')) {
        const group = document.createElementNS(svg.namespaceURI, 'g');
        group.setAttribute('transform', `translate(${parseFloat(node.style.left)} ${parseFloat(node.style.top)}) rotate(${parseFloat(node.style.getPropertyValue('--part-rotation'))}) scale(2)`);
        group.appendChild(node.querySelector('.organism-solid path').cloneNode(true)); sceneGroup.appendChild(group);
      }
      for (const node of host.querySelectorAll('.surface-organ')) {
        const path = node.querySelector('.surface-solid').cloneNode(true);
        path.removeAttribute('class'); path.setAttribute('fill', 'none');
        path.setAttribute('stroke', node.style.getPropertyValue('--part-stroke'));
        path.setAttribute('stroke-width', parseFloat(node.style.getPropertyValue('--part-stroke-width')));
        sceneGroup.appendChild(path);
      }
      const image = new Image();
      image.src = `data:image/svg+xml,${encodeURIComponent(new XMLSerializer().serializeToString(svg))}`;
      await image.decode();
      const expected = document.createElement('canvas'); expected.width = 256; expected.height = 192;
      const ctx = expected.getContext('2d'); ctx.fillStyle = '#000000'; ctx.fillRect(0, 0, 256, 192); ctx.drawImage(image, 0, 0);
      const scene = new THREE.Scene(); scene.background = new THREE.Color('#000000'); scene.add(createOrganismMesh(parts));
      const renderer = new THREE.WebGLRenderer({ preserveDrawingBuffer: true }); renderer.setSize(256, 192);
      const camera = new THREE.OrthographicCamera(-64, 64, 48, -48, .1, 100); camera.position.z = 10;
      scene.traverse(node => { if (node.material) { node.material.depthTest = false; node.material.depthWrite = false; node.renderOrder = node.userData.partOrder || 0; } });
      renderer.render(scene, camera);
      const actual = document.createElement('canvas'); actual.width = 256; actual.height = 192;
      const actualCtx = actual.getContext('2d'); actualCtx.drawImage(renderer.domElement, 0, 0);
      const a = ctx.getImageData(0, 0, 256, 192).data, b = actualCtx.getImageData(0, 0, 256, 192).data;
      let checked = 0, matches = 0, colored = 0;
      // Ignore rasterizer edge antialiasing; compare stable interior pixels, including all organ colors.
      for (let y = 2; y < 190; y++) for (let x = 2; x < 254; x++) {
        const index = (y * 256 + x) * 4;
        if (![-514, -512, -510, -2, 2, 510, 512, 514].every(delta => [0, 1, 2].every(c => a[index + c] === a[index + delta * 4 + c]))) continue;
        checked++;
        if ([0, 1, 2].every(c => Math.abs(a[index + c] - b[index + c]) < 3)) matches++;
        if (a[index] + a[index + 1] + a[index + 2] > 0) colored++;
      }
      document.body.append(expected, renderer.domElement);
      window.renderTest = { renderer };
      return { checked, matches, colored, parts: parts.length };
    });
    assert.equal(result.parts, 5);
    assert(result.colored > 3000, JSON.stringify(result));
    assert(result.matches / result.checked > .995, JSON.stringify(result));
    assert.deepEqual(errors, []);
    await page.screenshot({ path: process.env.RENDER_SCREENSHOT || 'organism-rendering.png', fullPage: true });
    console.log('PASS: assembler SVG and world mesh pixels agree for body, surface, wrap, rectangle and ellipse.', result);
  } finally { await browser.close(); }
})().catch(error => { console.error(error); process.exitCode = 1; });
