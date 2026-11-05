export function clampPageNumber(value, pageCount) {
  const count = Math.max(0, Math.trunc(Number(pageCount) || 0));
  if (count === 0) return 0;
  return Math.max(1, Math.min(count, Math.trunc(Number(value) || 1)));
}

export class FenwickTree {
  constructor(size) {
    this.size = Math.max(0, size | 0);
    this.tree = new Float64Array(this.size + 1);
    this.values = new Float64Array(this.size + 1);
  }

  set(index, value) {
    if (index < 1 || index > this.size) return;
    const nextValue = Math.max(0, Number(value) || 0);
    const delta = nextValue - this.values[index];
    if (Math.abs(delta) < 0.0001) return;
    this.values[index] = nextValue;
    for (let cursor = index; cursor <= this.size; cursor += cursor & -cursor) {
      this.tree[cursor] += delta;
    }
  }

  prefixSum(index) {
    let cursor = Math.max(0, Math.min(this.size, index | 0));
    let total = 0;
    while (cursor > 0) {
      total += this.tree[cursor];
      cursor -= cursor & -cursor;
    }
    return total;
  }

  total() {
    return this.prefixSum(this.size);
  }

  lowerBound(target) {
    if (this.size === 0) return 0;
    const wanted = Math.max(0, Number(target) || 0);
    if (wanted <= 0) return 1;
    let index = 0;
    let sum = 0;
    let bit = 1;
    while ((bit << 1) <= this.size) bit <<= 1;
    for (; bit !== 0; bit >>= 1) {
      const next = index + bit;
      if (next <= this.size && sum + this.tree[next] < wanted) {
        index = next;
        sum += this.tree[next];
      }
    }
    return Math.max(1, Math.min(this.size, index + 1));
  }
}
