// #796 — Tests for TruncatedAddress component and clipboard utilities
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, act, fireEvent } from "@testing-library/react";
import React from "react";
import TruncatedAddress from "@/components/TruncatedAddress";
import { truncateAddress, copyToClipboard } from "@/lib/utils";

const SAMPLE_STELLAR_ADDRESS =
  "GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN7";

describe("truncateAddress utility (#796)", () => {
  it("truncates standard 56-char Stellar address into G...ABCD format by default", () => {
    const truncated = truncateAddress(SAMPLE_STELLAR_ADDRESS);
    expect(truncated).toBe("G...CWN7");
  });

  it("supports custom head and tail parameters", () => {
    const custom = truncateAddress(SAMPLE_STELLAR_ADDRESS, 4, 4);
    expect(custom).toBe("GAAZ...CWN7");
  });

  it("handles short or empty strings safely", () => {
    expect(truncateAddress("G123")).toBe("G123");
    expect(truncateAddress("")).toBe("");
    expect(truncateAddress(null as unknown as string)).toBe("");
    expect(truncateAddress(undefined as unknown as string)).toBe("");
  });
});

describe("TruncatedAddress component (#796)", () => {
  let writeTextMock: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    writeTextMock = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: {
        writeText: writeTextMock,
      },
    });
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.restoreAllMocks();
  });

  it("renders truncated address", () => {
    render(<TruncatedAddress address={SAMPLE_STELLAR_ADDRESS} />);
    expect(screen.getByText("G...CWN7")).toBeTruthy();
    expect(screen.queryByTestId("copied-tooltip")).toBeNull();
  });

  it("copies full address to clipboard and displays 'Copied!' tooltip with checkmark", async () => {
    render(<TruncatedAddress address={SAMPLE_STELLAR_ADDRESS} />);
    const btn = screen.getByTestId("truncated-address-btn");

    await act(async () => {
      btn.click();
    });

    expect(writeTextMock).toHaveBeenCalledWith(SAMPLE_STELLAR_ADDRESS);
    expect(screen.getByTestId("copied-tooltip")).toBeTruthy();
    expect(screen.getByText("Copied!")).toBeTruthy();
    expect(screen.getByTestId("copy-checkmark-icon")).toBeTruthy();
  });

  it("copies full address when activated with Enter or Space", async () => {
    render(<TruncatedAddress address={SAMPLE_STELLAR_ADDRESS} />);
    const btn = screen.getByTestId("truncated-address-btn");

    btn.focus();

    await act(async () => {
      fireEvent.keyDown(btn, { key: "Enter" });
    });
    expect(writeTextMock).toHaveBeenCalledWith(SAMPLE_STELLAR_ADDRESS);

    writeTextMock.mockClear();

    await act(async () => {
      fireEvent.keyDown(btn, { key: " " });
    });
    expect(writeTextMock).toHaveBeenCalledWith(SAMPLE_STELLAR_ADDRESS);
  });

  it("hides 'Copied!' tooltip after 2000 ms", async () => {
    vi.useFakeTimers();

    render(<TruncatedAddress address={SAMPLE_STELLAR_ADDRESS} />);
    const btn = screen.getByTestId("truncated-address-btn");

    await act(async () => {
      btn.click();
    });

    expect(screen.getByText("Copied!")).toBeTruthy();

    // Fast-forward by 2000 ms
    act(() => {
      vi.advanceTimersByTime(2000);
    });

    expect(screen.queryByTestId("copied-tooltip")).toBeNull();
    expect(screen.getByTestId("copy-action-icon")).toBeTruthy();
  });
});
