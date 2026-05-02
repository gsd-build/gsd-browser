import assert from "node:assert/strict";

function mapViewerPoint(event, frameMeta, wrapEl) {
  const rect = wrapEl.getBoundingClientRect();
  const viewportWidth = Number(frameMeta.viewportCssWidth || frameMeta.viewport?.width || 0);
  const viewportHeight = Number(frameMeta.viewportCssHeight || frameMeta.viewport?.height || 0);
  const scale = Math.min(rect.width / viewportWidth, rect.height / viewportHeight);
  const renderedWidth = viewportWidth * scale;
  const renderedHeight = viewportHeight * scale;
  const letterboxX = (rect.width - renderedWidth) / 2;
  const letterboxY = (rect.height - renderedHeight) / 2;
  return {
    x: (event.clientX - rect.left - letterboxX) / scale,
    y: (event.clientY - rect.top - letterboxY) / scale,
    scale,
    letterboxX,
    letterboxY,
  };
}

function wrapRect(rect) {
  return { getBoundingClientRect: () => rect };
}

const cases = [
  {
    name: "dpr 1 center",
    rect: { left: 0, top: 0, width: 1920, height: 1080 },
    meta: { viewport: { width: 1920, height: 1080 }, devicePixelRatio: 1 },
    event: { clientX: 960, clientY: 540 },
    expected: { x: 960, y: 540 },
  },
  {
    name: "dpr 2 remains css coordinates",
    rect: { left: 0, top: 0, width: 1440, height: 900 },
    meta: { viewport: { width: 1440, height: 900 }, devicePixelRatio: 2, capturePixelWidth: 2880 },
    event: { clientX: 720, clientY: 450 },
    expected: { x: 720, y: 450 },
  },
  {
    name: "wide letterbox",
    rect: { left: 10, top: 20, width: 1200, height: 600 },
    meta: { viewport: { width: 1000, height: 1000 } },
    event: { clientX: 610, clientY: 320 },
    expected: { x: 500, y: 500 },
  },
  {
    name: "tall letterbox",
    rect: { left: 0, top: 0, width: 600, height: 1200 },
    meta: { viewport: { width: 1000, height: 500 } },
    event: { clientX: 300, clientY: 600 },
    expected: { x: 500, y: 250 },
  },
  {
    name: "resized wrapper",
    rect: { left: 100, top: 50, width: 960, height: 540 },
    meta: { viewport: { width: 1920, height: 1080 } },
    event: { clientX: 580, clientY: 320 },
    expected: { x: 960, y: 540 },
  },
  {
    name: "scrolled page uses client viewport coordinates",
    rect: { left: 0, top: 200, width: 800, height: 600 },
    meta: { viewport: { width: 800, height: 600, scrollY: 900 } },
    event: { clientX: 400, clientY: 500 },
    expected: { x: 400, y: 300 },
  },
  {
    name: "capture size mismatch keeps css viewport",
    rect: { left: 0, top: 0, width: 1000, height: 500 },
    meta: { viewportCssWidth: 1000, viewportCssHeight: 500, capturePixelWidth: 2000, capturePixelHeight: 1000 },
    event: { clientX: 250, clientY: 125 },
    expected: { x: 250, y: 125 },
  },
];

for (const testCase of cases) {
  const actual = mapViewerPoint(testCase.event, testCase.meta, wrapRect(testCase.rect));
  assert.equal(Math.round(actual.x * 1000) / 1000, testCase.expected.x, testCase.name);
  assert.equal(Math.round(actual.y * 1000) / 1000, testCase.expected.y, testCase.name);
}

console.log(`viewer-coordinate: ${cases.length} cases passed`);
