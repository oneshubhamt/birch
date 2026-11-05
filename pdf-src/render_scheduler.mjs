export function readingWindow({visiblePageNumbers, currentPage, scrollDirection}) {
  const visibleNumbers = visiblePageNumbers.length > 0 ? visiblePageNumbers : [currentPage];
  return {
    visibleNumbers,
    forwardEdge: scrollDirection > 0 ? Math.max(...visibleNumbers) : Math.min(...visibleNumbers),
    backwardEdge: scrollDirection > 0 ? Math.min(...visibleNumbers) : Math.max(...visibleNumbers),
  };
}

export function medianPositive(values) {
  const sorted = values.filter(value => Number.isFinite(value) && value > 0).sort((a, b) => a - b);
  if (sorted.length === 0) return 0;
  const middle = Math.floor(sorted.length / 2);
  return sorted.length % 2 === 0 ? (sorted[middle - 1] + sorted[middle]) / 2 : sorted[middle];
}

export function memoryOperatingBudget({
  visibleBytes,
  typicalPageBytes,
  desiredWarmPages,
  minimumBytes,
  hardLimitBytes,
}) {
  const desiredBytes = Math.max(0, visibleBytes)
    + Math.max(0, typicalPageBytes) * Math.max(1, desiredWarmPages);
  return Math.min(hardLimitBytes, Math.max(minimumBytes, desiredBytes));
}

export function renderTargetPageNumbers({
  pageCount,
  visiblePageNumbers,
  currentPage,
  scrollDirection,
  readAheadForward,
  estimatePageBytes,
  minimumOperatingBytes,
  hardLimitBytes,
}) {
  const {visibleNumbers, forwardEdge, backwardEdge} = readingWindow({
    visiblePageNumbers,
    currentPage,
    scrollDirection,
  });
  const target = new Set(visibleNumbers.filter(pageNumber => pageNumber >= 1 && pageNumber <= pageCount));
  const candidatePages = [];
  const nextPage = forwardEdge + scrollDirection;
  if (nextPage >= 1 && nextPage <= pageCount) candidatePages.push(nextPage);
  const behindPage = backwardEdge - scrollDirection;
  if (behindPage >= 1 && behindPage <= pageCount) candidatePages.push(behindPage);
  for (let offset = 2; offset <= readAheadForward; offset += 1) {
    const pageNumber = forwardEdge + scrollDirection * offset;
    if (pageNumber >= 1 && pageNumber <= pageCount) candidatePages.push(pageNumber);
  }

  const visibleCosts = [...target].map(estimatePageBytes).filter(bytes => bytes > 0);
  const candidateCosts = candidatePages.map(estimatePageBytes).filter(bytes => bytes > 0);
  const typicalBytes = medianPositive([...visibleCosts, ...candidateCosts]);
  const visibleBytes = visibleCosts.reduce((total, bytes) => total + bytes, 0);
  const operatingBudget = memoryOperatingBudget({
    visibleBytes,
    typicalPageBytes: typicalBytes,
    desiredWarmPages: candidatePages.length,
    minimumBytes: minimumOperatingBytes,
    hardLimitBytes,
  });
  let plannedBytes = visibleBytes;
  for (const pageNumber of candidatePages) {
    if (target.has(pageNumber)) continue;
    const bytes = Math.max(1, estimatePageBytes(pageNumber) || typicalBytes);
    if (plannedBytes + bytes > operatingBudget) continue;
    target.add(pageNumber);
    plannedBytes += bytes;
  }
  return target;
}

export function pageRenderPriority({
  pageNumber,
  visiblePages,
  currentPage,
  scrollDirection,
  readAheadForward,
}) {
  const visibleIndex = visiblePages.findIndex(item => item.pageNumber === pageNumber);
  if (visibleIndex >= 0) {
    const percent = visiblePages[visibleIndex].percent;
    return -2_000_000 - Math.round(percent * 100_000) + visibleIndex;
  }
  if (visiblePages.length === 0 && pageNumber === currentPage) return -1_900_000;

  const {forwardEdge, backwardEdge} = readingWindow({
    visiblePageNumbers: visiblePages.map(item => item.pageNumber),
    currentPage,
    scrollDirection,
  });
  const forwardDistance = (pageNumber - forwardEdge) * scrollDirection;
  const backwardDistance = (backwardEdge - pageNumber) * scrollDirection;
  if (forwardDistance === 1) return -1_000_000 + pageNumber;
  if (backwardDistance === 1) return -900_000 + pageNumber;
  if (forwardDistance > 1 && forwardDistance <= readAheadForward) {
    return -800_000 + forwardDistance * 100 + pageNumber;
  }
  return Math.abs(pageNumber - currentPage) * 1000 + pageNumber;
}

export function previewJobPriority({pageNumber, kind, center, currentPage}) {
  const currentBias = pageNumber === currentPage ? -100 : 0;
  const thumbnailPriority = Math.abs(pageNumber - center) * 100 + currentBias;
  return (kind === 'visible' ? -1_000_000 : 500_000) + thumbnailPriority;
}

/** Ordered single-raster-lane policy. Callbacks are evaluated lazily so later
 * lanes cannot allocate or mutate state until all higher-priority work yields. */
export function nextRenderAction({
  takeVisiblePage,
  visiblePagesNeedRaster,
  takeCriticalDetail,
  takeNextReadingPage,
  takeReadAheadPage,
  takeSettledDetail,
  takeVisiblePreview,
}) {
  const visiblePage = takeVisiblePage();
  if (visiblePage) return {kind: 'page', item: visiblePage};
  if (visiblePagesNeedRaster()) return null;

  const criticalDetail = takeCriticalDetail();
  if (criticalDetail) return {kind: 'detail', item: criticalDetail};
  const nextReadingPage = takeNextReadingPage();
  if (nextReadingPage) return {kind: 'page', item: nextReadingPage};
  const readAheadPage = takeReadAheadPage();
  if (readAheadPage) return {kind: 'page', item: readAheadPage};
  const settledDetail = takeSettledDetail();
  if (settledDetail) return {kind: 'detail', item: settledDetail};
  const visiblePreview = takeVisiblePreview();
  if (visiblePreview) return {kind: 'preview', item: visiblePreview};
  return null;
}
