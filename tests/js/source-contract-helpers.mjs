import assert from 'node:assert/strict';

export function compactRustSource(text) {
  return text.replace(/\s+/g, '');
}

export function assertRustContains(source, fragment, message) {
  const compactSource = compactRustSource(source);
  const compactFragment = compactRustSource(fragment);
  assert.ok(
    compactSource.includes(compactFragment),
    message ?? `missing Rust source fragment: ${fragment}`,
  );
}

export function assertRustOrder(source, fragments, message) {
  const compactSource = compactRustSource(source);
  let cursor = 0;

  for (const fragment of fragments) {
    const compactFragment = compactRustSource(fragment);
    const index = compactSource.indexOf(compactFragment, cursor);
    assert.ok(index >= 0, message ?? `missing ordered Rust source fragment: ${fragment}`);
    cursor = index + compactFragment.length;
  }
}
