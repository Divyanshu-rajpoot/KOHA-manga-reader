import { DestroyRef, ElementRef, Signal, inject, signal } from '@angular/core';

/** The host element's size in CSS pixels, kept up to date. Call in an injection context. */
export function hostSize(): Signal<{ width: number; height: number }> {
  const element = inject<ElementRef<HTMLElement>>(ElementRef).nativeElement;
  const size = signal({ width: element.clientWidth, height: element.clientHeight });
  const observer = new ResizeObserver(() => {
    const next = { width: element.clientWidth, height: element.clientHeight };
    const current = size();
    if (next.width !== current.width || next.height !== current.height) size.set(next);
  });
  observer.observe(element);
  inject(DestroyRef).onDestroy(() => observer.disconnect());
  return size;
}
