export function appendSearchFragment(state, value, node = null) {
  if (typeof value !== 'string' || value.length === 0) return;
  if (state.text.length > 0 && !/\s$/.test(state.text) && !/^\s/.test(value)) {
    state.text += ' ';
  }
  const start = state.text.length;
  state.text += value;
  if (node) state.segments.push({start, end: state.text.length, node});
}

export function textLayerSearchMap(view) {
  const state = {text: '', segments: []};
  const textLayer = view?.textLayerObject;
  const textDivs = textLayer?.textDivs ?? [];
  const strings = textLayer?.textContentItemsStr ?? [];
  const count = Math.min(textDivs.length, strings.length);
  for (let index = 0; index < count; index += 1) {
    const node = textDivs[index]?.firstChild;
    if (!node || node.nodeType !== Node.TEXT_NODE) continue;
    appendSearchFragment(state, strings[index] ?? '', node);
  }
  return state;
}

export function searchRangeForOffsets(map, start, end) {
  const first = map.segments.find(segment => segment.end > start);
  const last = [...map.segments].reverse().find(segment => segment.start < end);
  if (!first || !last) return null;
  const range = document.createRange();
  range.setStart(first.node, Math.max(0, Math.min(first.node.length, start - first.start)));
  range.setEnd(last.node, Math.max(0, Math.min(last.node.length, end - last.start)));
  return range;
}
