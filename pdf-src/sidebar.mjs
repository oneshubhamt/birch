export function defaultSidebarOpenForPageCount(pageCount) {
  return Number.isFinite(Number(pageCount)) && Number(pageCount) > 1;
}

export function thumbnailWindowBounds({scrollTop, viewportHeight, pageCount, rowHeight, overscanRows}) {
  const safeRowHeight = Math.max(1, Number(rowHeight) || 1);
  const firstVisible = Math.floor(Math.max(0, Number(scrollTop) || 0) / safeRowHeight) + 1;
  const visibleCount = Math.max(1, Math.ceil(Math.max(1, Number(viewportHeight) || 1) / safeRowHeight));
  const visibleStart = Math.max(1, firstVisible);
  const visibleEnd = Math.min(pageCount, firstVisible + visibleCount - 1);
  return {
    visibleStart,
    visibleEnd,
    start: Math.max(1, visibleStart - overscanRows),
    end: Math.min(pageCount, visibleEnd + overscanRows),
    center: visibleStart + (Math.max(1, visibleEnd - visibleStart + 1) - 1) / 2,
  };
}

export function annotationWindowBounds({scrollTop, viewportHeight, itemCount, rowHeight, overscanRows}) {
  const safeRowHeight = Math.max(1, Number(rowHeight) || 1);
  const firstVisible = Math.floor(Math.max(0, Number(scrollTop) || 0) / safeRowHeight);
  const visibleCount = Math.max(1, Math.ceil(Math.max(1, Number(viewportHeight) || 1) / safeRowHeight));
  return {
    start: Math.max(0, firstVisible - overscanRows),
    end: Math.min(itemCount, firstVisible + visibleCount + overscanRows + 1),
  };
}

export function previewDimensions({baseWidth, baseHeight, width, maxHeight, maxPixels}) {
  const scaleToFit = Math.min(
    width / Math.max(1, baseWidth),
    maxHeight / Math.max(1, baseHeight),
  );
  const cssWidth = Math.max(1, baseWidth * scaleToFit);
  const cssHeight = Math.max(1, baseHeight * scaleToFit);
  const requestedPixels = cssWidth * cssHeight;
  const outputScale = requestedPixels > maxPixels
    ? Math.sqrt(maxPixels / Math.max(1, requestedPixels))
    : 1;
  return {
    cssWidth,
    cssHeight,
    pixelWidth: Math.max(1, Math.floor(cssWidth * outputScale)),
    pixelHeight: Math.max(1, Math.floor(cssHeight * outputScale)),
    outputScale,
    pageScale: scaleToFit,
  };
}

function thumbnailDisplayDimensions({baseWidth, baseHeight, maxWidth, maxHeight}) {
  const scaleToFit = Math.min(
    maxWidth / Math.max(1, baseWidth),
    maxHeight / Math.max(1, baseHeight),
  );
  return {
    width: Math.max(1, baseWidth * scaleToFit),
    height: Math.max(1, baseHeight * scaleToFit),
  };
}

function annotationKindLabel(kind) {
  return {
    highlight: 'Highlight',
    underline: 'Underline',
    strike: 'Strike-through',
    note: 'Note',
    ink: 'Pen',
    shape: 'Shape',
    text: 'Text',
  }[kind] ?? 'Annotation';
}

function annotationAnchor(annotation, textMarkTools) {
  if (textMarkTools.has(annotation?.kind) && Array.isArray(annotation?.rects?.[0])) {
    return [Number(annotation.rects[0][0]) || 0, Number(annotation.rects[0][1]) || 0];
  }
  if (annotation?.kind === 'note' || annotation?.kind === 'text') {
    return [Number(annotation.x) || 0, Number(annotation.y) || 0];
  }
  if ((annotation?.kind === 'ink' || annotation?.kind === 'shape') && Array.isArray(annotation?.points?.[0])) {
    return [Number(annotation.points[0][0]) || 0, Number(annotation.points[0][1]) || 0];
  }
  return [0, 0];
}

export function annotationSummary(pageNumber, annotation, textMarkTools) {
  const [x, y] = annotationAnchor(annotation, textMarkTools);
  return {
    id: String(annotation?.id ?? ''),
    kind: String(annotation?.kind ?? ''),
    pageNumber,
    x: Math.max(0, Math.min(1, x)),
    y: Math.max(0, Math.min(1, y)),
    text: String(annotation?.text ?? '').replace(/\s+/g, ' ').trim().slice(0, 160),
  };
}

function annotationRowExcerpt(item) {
  if (item.text) return item.text;
  if (item.kind === 'shape') return 'Shape annotation';
  if (item.kind === 'ink') return 'Pen annotation';
  return annotationKindLabel(item.kind);
}

export function annotationCounterLabel({count, activeIndex, truncated}) {
  if (count === 0) return '0';
  if (activeIndex >= 0) return `${activeIndex + 1} / ${count}${truncated ? '+' : ''}`;
  return `${count}${truncated ? '+' : ''}`;
}

export function initialAnnotationIndex({items, currentPage, direction = 1}) {
  if (items.length === 0) return -1;
  if (direction < 0) {
    for (let index = items.length - 1; index >= 0; index -= 1) {
      if (items[index].pageNumber <= currentPage) return index;
    }
    return 0;
  }
  const atOrAfterPage = items.findIndex(item => item.pageNumber >= currentPage);
  return atOrAfterPage >= 0 ? atOrAfterPage : items.length - 1;
}

export function createThumbnailCanvas(documentRef, entry, {maxWidth, maxHeight}) {
  const canvas = documentRef.createElement('canvas');
  canvas.className = 'thumbnail-canvas';
  const display = thumbnailDisplayDimensions({
    baseWidth: entry.baseWidth,
    baseHeight: entry.baseHeight,
    maxWidth,
    maxHeight,
  });
  canvas.width = Math.max(1, Math.floor(display.width));
  canvas.height = Math.max(1, Math.floor(display.height));
  canvas.style.width = `${display.width}px`;
  canvas.style.height = `${display.height}px`;
  const context = canvas.getContext('2d', {alpha: false});
  if (context) {
    context.imageSmoothingEnabled = true;
    context.imageSmoothingQuality = 'medium';
    context.drawImage(entry.canvas, 0, 0, canvas.width, canvas.height);
  }
  return canvas;
}

export function createThumbnailRowElement(documentRef, {
  pageNumber,
  rowHeight,
  currentPage,
  onNavigate,
}) {
  const button = documentRef.createElement('button');
  button.className = 'thumbnail-row';
  button.type = 'button';
  button.dataset.pageNumber = String(pageNumber);
  button.style.top = `${(pageNumber - 1) * rowHeight}px`;
  button.setAttribute('aria-label', `Go to page ${pageNumber}`);

  const preview = documentRef.createElement('span');
  preview.className = 'thumbnail-preview';
  preview.setAttribute('aria-hidden', 'true');
  const label = documentRef.createElement('span');
  label.className = 'thumbnail-label';
  label.textContent = String(pageNumber);
  button.append(preview, label);
  button.addEventListener('click', () => onNavigate(pageNumber));
  button.classList.toggle('is-current', pageNumber === currentPage);
  return {pageNumber, button, preview, cacheEntry: null};
}

export function renderOutlineItems(documentRef, items, {maxItems, onNavigateDestination}) {
  const root = documentRef.createElement('ul');
  root.className = 'outline-list';
  let renderedItems = 0;

  function appendItems(parent, children, depth) {
    if (!Array.isArray(children) || depth > 12 || renderedItems >= maxItems) return;
    for (const item of children) {
      if (renderedItems >= maxItems) break;
      renderedItems += 1;
      const listItem = documentRef.createElement('li');
      listItem.className = 'outline-item';
      const button = documentRef.createElement('button');
      button.type = 'button';
      button.className = 'outline-link';
      button.textContent = String(item?.title || 'Untitled section').trim() || 'Untitled section';
      if (item?.bold) button.style.fontWeight = '650';
      if (item?.italic) button.style.fontStyle = 'italic';
      if (item?.dest) {
        button.addEventListener('click', async () => {
          button.disabled = true;
          try {
            await onNavigateDestination(item.dest);
          } finally {
            button.disabled = false;
          }
        });
      } else {
        button.disabled = true;
      }
      listItem.append(button);
      if (Array.isArray(item?.items) && item.items.length > 0) {
        const nested = documentRef.createElement('ul');
        nested.className = 'outline-list';
        appendItems(nested, item.items, depth + 1);
        if (nested.childElementCount > 0) listItem.append(nested);
      }
      parent.append(listItem);
    }
  }

  appendItems(root, items, 0);
  return {root, truncated: renderedItems >= maxItems};
}

export function createAnnotationRowElement(documentRef, {
  item,
  index,
  rowHeight,
  isCurrent,
  onActivate,
}) {
  const button = documentRef.createElement('button');
  button.type = 'button';
  button.className = 'annotation-row';
  button.style.top = `${index * rowHeight}px`;
  button.dataset.annotationId = item.id;
  button.setAttribute('aria-label', `${annotationKindLabel(item.kind)} on page ${item.pageNumber}`);

  const heading = documentRef.createElement('span');
  heading.className = 'annotation-row-heading';
  const kind = documentRef.createElement('span');
  kind.className = 'annotation-kind';
  kind.textContent = annotationKindLabel(item.kind);
  const page = documentRef.createElement('span');
  page.className = 'annotation-page';
  page.textContent = `Page ${item.pageNumber}`;
  heading.append(kind, page);

  const excerpt = documentRef.createElement('span');
  excerpt.className = 'annotation-excerpt';
  excerpt.textContent = annotationRowExcerpt(item);
  button.append(heading, excerpt);
  button.classList.toggle('is-current', isCurrent);
  button.addEventListener('click', () => onActivate(index));
  return button;
}
