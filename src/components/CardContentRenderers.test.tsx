import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { t } from "@/i18n";
import { useClipboardStore } from "@/stores/clipboard";
import { FileContent, ImageCard } from "./CardContentRenderers";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(() => Promise.resolve()),
  convertFileSrc: (path: string) => path,
}));

const imagePath = "E:\\images\\photo.png";
const fileProps = {
  filePaths: [imagePath],
  preview: "photo.png",
  metaItems: [],
  byteSize: 128,
};

beforeEach(() => {
  sessionStorage.clear();
  useClipboardStore.setState({ searchQuery: "" });
});

afterEach(cleanup);

describe("resource availability and preview rendering", () => {
  it("distinguishes cached-image unknown, checking, missing and decode-failed states", () => {
    const props = { image_path: imagePath, metaItems: [] };
    const { rerender } = render(<ImageCard {...props} availability="unknown" />);
    expect(screen.getByText(t("cardContent.resourceUnknown"))).toBeInTheDocument();
    expect(screen.queryByRole("img")).not.toBeInTheDocument();
    expect(screen.queryByText(t("cardContent.invalid"))).not.toBeInTheDocument();

    rerender(<ImageCard {...props} availability="checking" />);
    expect(screen.getByText(t("cardContent.resourceChecking"))).toBeInTheDocument();
    expect(screen.queryByRole("img")).not.toBeInTheDocument();
    expect(screen.queryByText(t("cardContent.previewLoadFailed"))).not.toBeInTheDocument();

    rerender(<ImageCard {...props} availability="unavailable" />);
    expect(screen.getByText(t("cardContent.invalid"))).toBeInTheDocument();
    expect(screen.queryByRole("img")).not.toBeInTheDocument();

    rerender(<ImageCard {...props} availability="available" />);
    fireEvent.error(screen.getByRole("img"));
    expect(screen.getByText(t("cardContent.previewLoadFailed"))).toBeInTheDocument();
    expect(screen.queryByText(t("cardContent.invalid"))).not.toBeInTheDocument();

    rerender(<ImageCard {...props} availability="checking" />);
    expect(screen.getByText(t("cardContent.resourceChecking"))).toBeInTheDocument();
    expect(screen.queryByText(t("cardContent.previewLoadFailed"))).not.toBeInTheDocument();

    rerender(<ImageCard {...props} image_path={"E:\\images\\other.png"} availability="available" />);
    expect(screen.getByRole("img")).toBeInTheDocument();
    fireEvent.load(screen.getByRole("img"));
    expect(screen.queryByText(t("cardContent.previewLoadFailed"))).not.toBeInTheDocument();
  });

  it("does not preview file images before resource availability is established", () => {
    const { rerender } = render(<FileContent {...fileProps} availability="unknown" />);
    expect(screen.getByText(t("cardContent.resourceUnknown"))).toBeInTheDocument();
    expect(screen.queryByRole("img")).not.toBeInTheDocument();
    expect(screen.getByText(imagePath)).not.toHaveClass("line-through");

    rerender(<FileContent {...fileProps} availability="checking" />);
    expect(screen.getByText(t("cardContent.resourceChecking"))).toBeInTheDocument();
    expect(screen.queryByRole("img")).not.toBeInTheDocument();
    expect(screen.queryByText(t("cardContent.invalid"))).not.toBeInTheDocument();

    rerender(<FileContent {...fileProps} availability="unavailable" />);
    expect(screen.getByText(t("cardContent.invalid"))).toBeInTheDocument();
    expect(screen.getByText(imagePath)).toHaveClass("line-through");

    rerender(<FileContent {...fileProps} availability="available" />);
    expect(screen.getByRole("img")).toBeInTheDocument();
    expect(screen.queryByText(t("cardContent.invalid"))).not.toBeInTheDocument();
  });

  it("caches file preview failures without invalidating the source or another image", () => {
    const first = render(<FileContent {...fileProps} availability="available" />);
    fireEvent.error(screen.getByRole("img"));
    expect(screen.getByText(t("cardContent.previewLoadFailed"))).toBeInTheDocument();
    expect(screen.getByText(imagePath)).not.toHaveClass("line-through");
    expect(screen.queryByText(t("cardContent.invalid"))).not.toBeInTheDocument();
    first.unmount();

    const { rerender } = render(<FileContent {...fileProps} availability="available" />);
    expect(screen.getByText(t("cardContent.previewLoadFailed"))).toBeInTheDocument();
    expect(screen.queryByRole("img")).not.toBeInTheDocument();
    expect(screen.getByText(imagePath)).not.toHaveClass("line-through");

    rerender(<FileContent {...fileProps} filePaths={["E:\\images\\other.png"]} availability="available" />);
    expect(screen.getByRole("img")).toBeInTheDocument();
    fireEvent.load(screen.getByRole("img"));
    expect(screen.queryByText(t("cardContent.previewLoadFailed"))).not.toBeInTheDocument();

    rerender(<FileContent {...fileProps} availability="unavailable" />);
    expect(screen.getByText(t("cardContent.invalid"))).toBeInTheDocument();
    expect(screen.queryByText(t("cardContent.previewLoadFailed"))).not.toBeInTheDocument();
  });

  it("treats a skipped oversized preview independently from a missing resource", () => {
    const { rerender } = render(<FileContent {...fileProps} availability="available" tooLarge />);
    expect(screen.getByText(t("cardContent.fileTooLarge"))).toBeInTheDocument();
    expect(screen.queryByRole("img")).not.toBeInTheDocument();
    expect(screen.queryByText(t("cardContent.invalid"))).not.toBeInTheDocument();
    expect(screen.getByText(imagePath)).not.toHaveClass("line-through");

    rerender(<FileContent {...fileProps} availability="unavailable" tooLarge />);
    expect(screen.getByText(t("cardContent.invalid"))).toBeInTheDocument();
    expect(screen.queryByText(t("cardContent.fileTooLarge"))).not.toBeInTheDocument();
  });
});
