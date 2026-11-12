/** `[width, height]` in pixels. `[0, 0]` means unknown. */
export type PageSize = [number, number];

/** Shape assumed for a page whose size could not be read: a tankōbon page. */
const DEFAULT_ASPECT = 1 / 1.42;

/** Width divided by height. */
export function aspectOf(size: PageSize | undefined): number {
  return size && size[0] > 0 && size[1] > 0 ? size[0] / size[1] : DEFAULT_ASPECT;
}

/** A landscape page is a double-page scan and is shown on its own. */
export function isWide(size: PageSize | undefined): boolean {
  return aspectOf(size) > 1;
}

/**
 * Groups pages into two-page spreads, in reading order.
 * The cover (page 0) and any wide page stand alone; the rest pair up.
 */
export function buildSpreads(sizes: PageSize[]): number[][] {
  const spreads: number[][] = [];
  let i = 0;
  while (i < sizes.length) {
    const alone = i === 0 || isWide(sizes[i]) || i + 1 >= sizes.length || isWide(sizes[i + 1]);
    if (alone) {
      spreads.push([i]);
      i += 1;
    } else {
      spreads.push([i, i + 1]);
      i += 2;
    }
  }
  return spreads;
}
