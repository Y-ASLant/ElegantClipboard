import { render, screen, fireEvent, act } from "@testing-library/react";
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { Onboarding } from "./Onboarding";

beforeEach(() => {
  vi.useFakeTimers();
});

afterEach(() => {
  vi.useRealTimers();
});

describe("Onboarding", () => {
  it("renders first step", () => {
    const onComplete = vi.fn();
    render(<Onboarding onComplete={onComplete} />);
    expect(screen.getByText("欢迎使用 ElegantClipboard")).toBeInTheDocument();
  });

  it("renders skip button", () => {
    const onComplete = vi.fn();
    render(<Onboarding onComplete={onComplete} />);
    expect(screen.getByText("跳过")).toBeInTheDocument();
  });

  it("renders next button", () => {
    const onComplete = vi.fn();
    render(<Onboarding onComplete={onComplete} />);
    expect(screen.getByText("下一步")).toBeInTheDocument();
  });

  it("calls onComplete when skip is clicked", () => {
    const onComplete = vi.fn();
    render(<Onboarding onComplete={onComplete} />);
    fireEvent.click(screen.getByText("跳过"));
    expect(onComplete).toHaveBeenCalledTimes(1);
  });

  it("navigates to next step when next is clicked", () => {
    const onComplete = vi.fn();
    render(<Onboarding onComplete={onComplete} />);
    
    act(() => {
      fireEvent.click(screen.getByText("下一步"));
    });
    
    act(() => {
      vi.advanceTimersByTime(350);
    });
    
    expect(screen.getByText("智能搜索")).toBeInTheDocument();
    expect(onComplete).not.toHaveBeenCalled();
  });

  it("navigates through all steps", () => {
    const onComplete = vi.fn();
    render(<Onboarding onComplete={onComplete} />);
    
    // Step 1 -> 2
    act(() => {
      fireEvent.click(screen.getByText("下一步"));
    });
    act(() => {
      vi.advanceTimersByTime(350);
    });
    expect(screen.getByText("智能搜索")).toBeInTheDocument();
    
    // Step 2 -> 3
    act(() => {
      fireEvent.click(screen.getByText("下一步"));
    });
    act(() => {
      vi.advanceTimersByTime(350);
    });
    expect(screen.getByText("置顶与收藏")).toBeInTheDocument();
    
    // Step 3 -> 4
    act(() => {
      fireEvent.click(screen.getByText("下一步"));
    });
    act(() => {
      vi.advanceTimersByTime(350);
    });
    expect(screen.getByText("快捷键操作")).toBeInTheDocument();
  });

  it("shows finish button on last step", () => {
    const onComplete = vi.fn();
    render(<Onboarding onComplete={onComplete} />);
    
    // Navigate to last step
    act(() => {
      fireEvent.click(screen.getByText("下一步"));
    });
    act(() => {
      vi.advanceTimersByTime(350);
    });
    
    act(() => {
      fireEvent.click(screen.getByText("下一步"));
    });
    act(() => {
      vi.advanceTimersByTime(350);
    });
    
    act(() => {
      fireEvent.click(screen.getByText("下一步"));
    });
    act(() => {
      vi.advanceTimersByTime(350);
    });
    
    expect(screen.getByText("开始使用")).toBeInTheDocument();
  });

  it("calls onComplete on last step click", () => {
    const onComplete = vi.fn();
    render(<Onboarding onComplete={onComplete} />);
    
    // Navigate to last step
    act(() => {
      fireEvent.click(screen.getByText("下一步"));
    });
    act(() => {
      vi.advanceTimersByTime(350);
    });
    
    act(() => {
      fireEvent.click(screen.getByText("下一步"));
    });
    act(() => {
      vi.advanceTimersByTime(350);
    });
    
    act(() => {
      fireEvent.click(screen.getByText("下一步"));
    });
    act(() => {
      vi.advanceTimersByTime(350);
    });
    
    act(() => {
      fireEvent.click(screen.getByText("开始使用"));
    });
    
    expect(onComplete).toHaveBeenCalledTimes(1);
  });

  it("renders keyboard hint", () => {
    const onComplete = vi.fn();
    render(<Onboarding onComplete={onComplete} />);
    expect(screen.getByText("Esc")).toBeInTheDocument();
  });
});
