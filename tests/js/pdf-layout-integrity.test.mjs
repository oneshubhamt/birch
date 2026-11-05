import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import test from 'node:test';

const cssSource = readFileSync(new URL('../../assets/pdfjs/rustnote-pdf-viewer.css', import.meta.url), 'utf8');
const viewerSource = readFileSync(new URL('../../pdf-src/viewer.mjs', import.meta.url), 'utf8');
const attachmentViewSource = readFileSync(new URL('../../editor-src/attachments/view.js', import.meta.url), 'utf8');
const {defaultSidebarOpenForPageCount} = await import('../../pdf-src/sidebar.mjs');

function cssRule(selector) {
  const escaped = selector.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  const match = cssSource.match(new RegExp(`${escaped}\\s*\\{([\\s\\S]*?)\\}`));
  assert.ok(match, `missing CSS rule ${selector}`);
  return match[1];
}

function mediaBlock(maxWidth) {
  const marker = `@media (max-width: ${maxWidth}px)`;
  const start = cssSource.indexOf(marker);
  assert.notEqual(start, -1, `missing ${marker}`);
  const open = cssSource.indexOf('{', start);
  let depth = 0;
  for (let index = open; index < cssSource.length; index += 1) {
    if (cssSource[index] === '{') depth += 1;
    else if (cssSource[index] === '}') {
      depth -= 1;
      if (depth === 0) return cssSource.slice(open + 1, index);
    }
  }
  throw new Error(`unterminated ${marker}`);
}

function functionSource(source, name) {
  const start = source.indexOf(`function ${name}(`);
  assert.notEqual(start, -1, `missing function ${name}`);
  const open = source.indexOf('{', start);
  let depth = 0;
  let quote = '';
  let escaped = false;
  for (let index = open; index < source.length; index += 1) {
    const char = source[index];
    if (quote) {
      if (escaped) escaped = false;
      else if (char === '\\') escaped = true;
      else if (char === quote) quote = '';
      continue;
    }
    if (char === '"' || char === "'" || char === '`') {
      quote = char;
      continue;
    }
    if (char === '{') depth += 1;
    else if (char === '}') {
      depth -= 1;
      if (depth === 0) return source.slice(start, index + 1);
    }
  }
  throw new Error(`unterminated function ${name}`);
}

test('primary PDF toolbar remains sticky in base and narrow embedded layouts', () => {
  assert.match(cssRule('.viewer-toolbar'), /position:\s*sticky/);
  assert.match(cssRule('.viewer-toolbar'), /top:\s*0/);
  const narrow = mediaBlock(760);
  assert.match(narrow, /\.viewer-toolbar\s*\{[\s\S]*?position:\s*sticky/);
  assert.doesNotMatch(narrow, /\.viewer-toolbar\s*\{[\s\S]*?position:\s*relative/);
});

test('markup toolbar remains sticky below measured primary toolbar height', () => {
  const markup = cssRule('.markup-toolbar');
  assert.match(markup, /position:\s*sticky/);
  assert.match(markup, /top:\s*var\(--viewer-toolbar-height\)/);
  assert.doesNotMatch(markup, /top:\s*44px/);
  const narrow = mediaBlock(760);
  assert.doesNotMatch(narrow, /\.markup-toolbar\s*\{[\s\S]*?position:\s*relative/);
});

test('sidebar remains an in-layout sticky column rather than a narrow-width overlay', () => {
  const sidebar = cssRule('.pdf-sidebar');
  assert.match(sidebar, /position:\s*sticky/);
  assert.match(sidebar, /top:\s*var\(--viewer-chrome-height\)/);
  const narrow = mediaBlock(760);
  assert.match(narrow, /\.pdf-sidebar\s*\{[\s\S]*?position:\s*sticky/);
  assert.doesNotMatch(narrow, /\.pdf-sidebar\s*\{[\s\S]*?position:\s*fixed/);
  assert.doesNotMatch(narrow, /box-shadow:\s*4px 0 18px/);
});

test('multi-page documents open with thumbnails available while one-page documents stay compact', () => {
  assert.equal(defaultSidebarOpenForPageCount(1), false);
  assert.equal(defaultSidebarOpenForPageCount(2), true);
  assert.equal(defaultSidebarOpenForPageCount(100), true);
  assert.equal(defaultSidebarOpenForPageCount('bad'), false);
  assert.match(viewerSource, /setSidebarOpen\(defaultSidebarOpenForPageCount\(pdf\.numPages\), \{refit: false\}\)/);
});

test('viewer chrome geometry has one measured authority shared by markup and sidebar offsets', () => {
  const sync = functionSource(viewerSource, 'syncViewerChromeGeometry');
  assert.match(sync, /elements\.toolbar\?\.getBoundingClientRect\(\)\.height/);
  assert.match(sync, /toolbarBottom\(\)/);
  assert.match(sync, /--viewer-toolbar-height/);
  assert.match(sync, /--viewer-chrome-height/);
  assert.doesNotMatch(viewerSource, /--sidebar-top|--sidebar-height/);
  assert.match(viewerSource, /new ResizeObserver\(\(\) => syncViewerChromeGeometry\(\)\)/);

  const values = new Map();
  const run = new Function('elements', 'toolbarBottom', 'document', `${sync}; syncViewerChromeGeometry();`);
  run(
    {toolbar: {getBoundingClientRect: () => ({height: 51})}},
    () => 93,
    {documentElement: {style: {setProperty: (name, value) => values.set(name, value)}}},
  );
  assert.equal(values.get('--viewer-toolbar-height'), '51px');
  assert.equal(values.get('--viewer-chrome-height'), '93px');
});

test('search markup resize and focus transitions resynchronize viewer chrome', () => {
  assert.match(functionSource(viewerSource, 'setSearchOpen'), /syncViewerChromeGeometry\(\)/);
  assert.match(functionSource(viewerSource, 'setMarkupOpen'), /syncViewerChromeGeometry\(\)/);
  assert.match(functionSource(viewerSource, 'setFocusActive'), /requestAnimationFrame\(syncViewerChromeGeometry\)/);
  assert.match(viewerSource, /window\.addEventListener\('resize',[\s\S]*?syncViewerChromeGeometry\(\)/);
});

test('narrow embedded layout compresses horizontally without dismantling reader chrome', () => {
  const narrow = mediaBlock(760);
  assert.match(narrow, /flex-wrap:\s*nowrap/);
  assert.match(narrow, /overflow-x:\s*auto/);
  assert.doesNotMatch(narrow, /flex:\s*1 0 100%/);
  assert.doesNotMatch(narrow, /position:\s*fixed/);
});

test('thumbnail contents and annotations views remain present in the PDF sidebar contract', () => {
  for (const marker of [
    'id="sidebar-thumbnails-tab"',
    'id="sidebar-outline-tab"',
    'id="sidebar-annotations-tab"',
    'id="thumbnail-panel"',
    'id="outline-panel"',
    'id="annotation-panel"',
  ]) {
    assert.match(attachmentViewSource, new RegExp(marker.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')));
  }
});
