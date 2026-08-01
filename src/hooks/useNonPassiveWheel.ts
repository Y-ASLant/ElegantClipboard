import { useEffect, useRef, useState } from "react";

/**
 * Attaches a wheel listener that may call preventDefault(). React registers
 * onWheel passively, so preview interactions that suppress scrolling use this.
 */
export function useNonPassiveWheel<T extends HTMLElement>(
  onWheel: (event: WheelEvent) => void,
) {
  const onWheelRef = useRef(onWheel);
  const [target, setTarget] = useState<T | null>(null);

  useEffect(() => {
    onWheelRef.current = onWheel;
  }, [onWheel]);

  useEffect(() => {
    if (!target) return;

    const listener = (event: WheelEvent) => onWheelRef.current(event);
    target.addEventListener("wheel", listener, { passive: false });
    return () => target.removeEventListener("wheel", listener);
  }, [target]);

  return setTarget;
}
