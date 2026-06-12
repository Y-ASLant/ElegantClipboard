import { render, screen } from "@testing-library/react";
import { describe, it, expect } from "vitest";
import { useClipboardStore } from "@/stores/clipboard";
import { HighlightText } from "./HighlightText";

// Reset store
beforeEach(() => {
  useClipboardStore.setState({ searchQuery: "" });
});

describe("HighlightText", () => {
  it("renders plain text when no search query", () => {
    render(<HighlightText text="Hello World" />);
    expect(screen.getByText("Hello World")).toBeInTheDocument();
  });

  it("renders plain text when search query is empty", () => {
    useClipboardStore.setState({ searchQuery: "  " });
    render(<HighlightText text="Hello World" />);
    expect(screen.getByText("Hello World")).toBeInTheDocument();
  });

  it("highlights matching text", () => {
    useClipboardStore.setState({ searchQuery: "World" });
    render(<HighlightText text="Hello World" />);

    const mark = screen.getByText("World");
    expect(mark.tagName).toBe("MARK");
    expect(mark).toHaveClass("search-highlight");
  });

  it("highlights multiple matches", () => {
    useClipboardStore.setState({ searchQuery: "o" });
    render(<HighlightText text="Hello World" />);

    const marks = screen.getAllByText("o");
    expect(marks.length).toBe(2);
    marks.forEach((mark) => {
      expect(mark.tagName).toBe("MARK");
    });
  });

  it("is case-insensitive", () => {
    useClipboardStore.setState({ searchQuery: "hello" });
    render(<HighlightText text="Hello World" />);

    const mark = screen.getByText("Hello");
    expect(mark.tagName).toBe("MARK");
  });

  it("escapes regex special characters", () => {
    useClipboardStore.setState({ searchQuery: "foo.bar" });
    render(<HighlightText text="foo.bar baz" />);

    const mark = screen.getByText("foo.bar");
    expect(mark.tagName).toBe("MARK");
  });

  it("handles text with no match", () => {
    useClipboardStore.setState({ searchQuery: "xyz" });
    render(<HighlightText text="Hello World" />);

    expect(screen.getByText("Hello World")).toBeInTheDocument();
    expect(screen.queryAllByRole("mark")).toHaveLength(0);
  });

  it("handles empty text", () => {
    useClipboardStore.setState({ searchQuery: "test" });
    const { container } = render(<HighlightText text="" />);
    expect(container.textContent).toBe("");
  });
});
