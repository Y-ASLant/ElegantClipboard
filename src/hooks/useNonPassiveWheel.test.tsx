import { render } from "@testing-library/react";
import { vi } from "vitest";
import { useNonPassiveWheel } from "./useNonPassiveWheel";

function WheelTarget({ onWheel }: { onWheel: (event: WheelEvent) => void }) {
  const ref = useNonPassiveWheel(onWheel);
  return <div ref={ref} data-testid="wheel-target" />;
}

describe("useNonPassiveWheel", () => {
  it("can cancel Ctrl+wheel and prevent it from reaching the parent", () => {
    const onWheel = vi.fn((event: WheelEvent) => {
      if (!event.ctrlKey) return;
      event.preventDefault();
      event.stopPropagation();
    });
    const parentWheel = vi.fn();
    const { getByTestId, unmount } = render(
      <div onWheel={parentWheel}>
        <WheelTarget onWheel={onWheel} />
      </div>,
    );
    const target = getByTestId("wheel-target");

    const ctrlWheel = new WheelEvent("wheel", {
      bubbles: true,
      cancelable: true,
      ctrlKey: true,
      deltaY: 120,
    });
    target.dispatchEvent(ctrlWheel);

    expect(onWheel).toHaveBeenCalledWith(ctrlWheel);
    expect(ctrlWheel.defaultPrevented).toBe(true);
    expect(parentWheel).not.toHaveBeenCalled();

    const normalWheel = new WheelEvent("wheel", {
      bubbles: true,
      cancelable: true,
      deltaY: 120,
    });
    target.dispatchEvent(normalWheel);

    expect(normalWheel.defaultPrevented).toBe(false);
    expect(parentWheel).toHaveBeenCalledTimes(1);

    unmount();
    target.dispatchEvent(ctrlWheel);
    expect(onWheel).toHaveBeenCalledTimes(2);
  });
});
