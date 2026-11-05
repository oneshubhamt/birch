import {PORTABLE_ANNOTATION_TYPE, TEXT_FONT_SIZES} from './config.mjs';

export function rgbBytes(colorName) {
  const hex = colorFor({color: colorName}).slice(1);
  return [0, 2, 4].map(index => Number.parseInt(hex.slice(index, index + 2), 16));
}

export function annotationPdfPoint(viewport, point) {
  return viewport.convertToPdfPoint(
    Math.max(0, Math.min(1, Number(point?.[0]) || 0)) * viewport.width,
    Math.max(0, Math.min(1, Number(point?.[1]) || 0)) * viewport.height,
  );
}

export function annotationPdfRect(viewport, rect) {
  const x = Math.max(0, Math.min(1, Number(rect?.[0]) || 0));
  const y = Math.max(0, Math.min(1, Number(rect?.[1]) || 0));
  const width = Math.max(0, Math.min(1 - x, Number(rect?.[2]) || 0));
  const height = Math.max(0, Math.min(1 - y, Number(rect?.[3]) || 0));
  const points = [
    annotationPdfPoint(viewport, [x, y]),
    annotationPdfPoint(viewport, [x + width, y]),
    annotationPdfPoint(viewport, [x, y + height]),
    annotationPdfPoint(viewport, [x + width, y + height]),
  ];
  const xs = points.map(point => point[0]);
  const ys = points.map(point => point[1]);
  return [Math.min(...xs), Math.min(...ys), Math.max(...xs), Math.max(...ys)];
}

export function exportInkPath(points) {
  const lines = [];
  for (const [x, y] of points) lines.push(Number.NaN, Number.NaN, Number.NaN, Number.NaN, x, y);
  return lines;
}

export function exportVectorPoints(annotation) {
  if (annotation.kind === 'ink') return [annotation.points ?? []];
  if (annotation.kind === 'underline' || annotation.kind === 'strike') {
    return (annotation.rects ?? []).map(rect => {
      const [x, y, width, height] = rect;
      const lineY = y + height * (annotation.kind === 'underline' ? 0.90 : 0.52);
      return [[x, lineY], [x + width, lineY]];
    });
  }
  if (annotation.kind !== 'shape' || !Array.isArray(annotation.points) || annotation.points.length !== 2) return [];
  const [[x1, y1], [x2, y2]] = annotation.points;
  if (annotation.shape === 'line') return [[[x1, y1], [x2, y2]]];
  if (annotation.shape === 'rectangle') {
    return [[[x1, y1], [x2, y1], [x2, y2], [x1, y2], [x1, y1]]];
  }
  const centerX = (x1 + x2) / 2;
  const centerY = (y1 + y2) / 2;
  const radiusX = Math.abs(x2 - x1) / 2;
  const radiusY = Math.abs(y2 - y1) / 2;
  const ellipse = [];
  for (let index = 0; index <= 24; index += 1) {
    const angle = Math.PI * 2 * index / 24;
    ellipse.push([centerX + Math.cos(angle) * radiusX, centerY + Math.sin(angle) * radiusY]);
  }
  return [ellipse];
}

export function portableAnnotationValue(annotation, pageIndex, viewport) {
  const base = {pageIndex, rotation: viewport.rotation || 0, date: new Date(), user: 'Rustnote'};
  const color = rgbBytes(annotation.color);
  if (annotation.kind === 'highlight') {
    const rects = Array.isArray(annotation.rects) ? annotation.rects : [];
    if (rects.length === 0) return null;
    const quadPoints = [];
    const outlines = [];
    for (const rect of rects) {
      const [x, y, width, height] = rect;
      const topLeft = annotationPdfPoint(viewport, [x, y]);
      const topRight = annotationPdfPoint(viewport, [x + width, y]);
      const bottomLeft = annotationPdfPoint(viewport, [x, y + height]);
      const bottomRight = annotationPdfPoint(viewport, [x + width, y + height]);
      for (const point of [topLeft, topRight, bottomLeft, bottomRight]) quadPoints.push(...point);
      outlines.push([...topLeft, ...topRight, ...bottomRight, ...bottomLeft]);
    }
    const xs = quadPoints.filter((_, index) => index % 2 === 0);
    const ys = quadPoints.filter((_, index) => index % 2 === 1);
    return {
      ...base,
      annotationType: PORTABLE_ANNOTATION_TYPE.HIGHLIGHT,
      rect: [Math.min(...xs), Math.min(...ys), Math.max(...xs), Math.max(...ys)],
      color,
      opacity: 0.34,
      thickness: 1,
      quadPoints,
      outlines,
    };
  }
  if (annotation.kind === 'note' || annotation.kind === 'text') {
    const value = String(annotation.text ?? '');
    if (value.trim() === '') return null;
    return {
      ...base,
      annotationType: PORTABLE_ANNOTATION_TYPE.FREETEXT,
      rect: annotationPdfRect(viewport, [annotation.x, annotation.y, annotation.width, annotation.height]),
      color,
      fontSize: annotation.kind === 'text' ? (TEXT_FONT_SIZES[annotation.size] ?? TEXT_FONT_SIZES.medium) : 12,
      value,
    };
  }
  const paths = exportVectorPoints(annotation)
    .map(points => points.map(point => annotationPdfPoint(viewport, point)))
    .filter(points => points.length >= 2);
  if (paths.length === 0) return null;
  const flat = paths.flat();
  const xs = flat.map(point => point[0]);
  const ys = flat.map(point => point[1]);
  const normalizedWidth = Number(annotation.width) || (annotation.kind === 'underline' || annotation.kind === 'strike' ? 0.0015 : 0.003);
  const thickness = Math.max(0.5, normalizedWidth * Math.min(viewport.width, viewport.height));
  const padding = Math.max(1, thickness / 2);
  return {
    ...base,
    annotationType: PORTABLE_ANNOTATION_TYPE.INK,
    rect: [
      Math.min(...xs) - padding,
      Math.min(...ys) - padding,
      Math.max(...xs) + padding,
      Math.max(...ys) + padding,
    ],
    color,
    opacity: 1,
    thickness,
    paths: {
      points: paths.map(points => points.flat()),
      lines: paths.map(exportInkPath),
    },
  };
}

export function cloneAnnotations(value) {
  return JSON.parse(JSON.stringify(value ?? []));
}

export function annotationId() {
  const random = globalThis.crypto?.randomUUID?.().replaceAll('-', '')
    ?? `${Date.now().toString(36)}${Math.random().toString(36).slice(2)}`;
  return `ann_${random.slice(0, 96)}`;
}

export function normalizedPoint(view, clientX, clientY) {
  const bounds = view.shell.getBoundingClientRect();
  return [
    Math.max(0, Math.min(1, (clientX - bounds.left) / Math.max(1, bounds.width))),
    Math.max(0, Math.min(1, (clientY - bounds.top) / Math.max(1, bounds.height))),
  ];
}

export function colorFor(annotation) {
  const colors = {
    yellow: '#ffd84d', green: '#64d98b', blue: '#67a8ff', pink: '#ff7eb6',
    red: '#d92d20', black: '#202124',
  };
  return colors[annotation.color] ?? colors.yellow;
}

export function boxSizeFor(options, size) {
  return options[size] ?? options.medium;
}
