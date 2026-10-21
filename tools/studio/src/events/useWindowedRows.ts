import { useEffect, useRef, useState } from 'react';

/** Fixed row geometry shared by the read-only event lists. */
export function useWindowedRows(count: number, rowHeight: number) {
  const ref = useRef<HTMLDivElement>(null);
  const [viewport, setViewport] = useState({ top: 0, height: 640 });
  useEffect(() => {
    const element = ref.current;
    if (!element) return;
    const measure = () => setViewport({ top: element.scrollTop, height: element.clientHeight });
    const observer = new ResizeObserver(measure);
    observer.observe(element); measure();
    return () => observer.disconnect();
  }, [count, rowHeight]);
  const start = Math.max(0, Math.floor(viewport.top / rowHeight) - 8);
  const end = Math.min(count, Math.ceil((viewport.top + viewport.height) / rowHeight) + 8);
  const onScroll = () => {
    const element = ref.current;
    if (element) setViewport({ top: element.scrollTop, height: element.clientHeight });
  };
  const scrollTo = (index: number) => {
    const element = ref.current;
    if (!element) return;
    const top = index * rowHeight;
    if (top < element.scrollTop) element.scrollTop = top;
    else if (top + rowHeight > element.scrollTop + element.clientHeight) element.scrollTop = top + rowHeight - element.clientHeight;
    onScroll();
  };
  return { ref, start, end, onScroll, scrollTo, height: count * rowHeight, offset: start * rowHeight };
}
