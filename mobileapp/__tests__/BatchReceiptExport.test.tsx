import React from "react";
import { Share } from "react-native";
import { render, fireEvent, waitFor } from "@testing-library/react-native";
import BatchDetailScreen from "../app/transaction/batch/[id]";
import { captureRef } from "react-native-view-shot";

jest.mock("react-native-view-shot", () => ({
  captureRef: jest.fn(() => Promise.resolve("file:///tmp/zaps-receipt.png")),
}));

jest.mock("expo-router", () => ({
  useLocalSearchParams: () => ({ id: "BATCH-001" }),
  useRouter: () => ({ back: jest.fn() }),
  Stack: {
    Screen: () => null,
  },
}));

describe("BatchReceiptExport", () => {
  beforeEach(() => {
    jest.clearAllMocks();
    jest
      .spyOn(Share, "share")
      .mockResolvedValue({ action: "sharedAction" } as any);
  });

  it("renders the export/share button in the header", () => {
    const { getByLabelText } = render(<BatchDetailScreen />);
    expect(getByLabelText("Export receipt")).toBeTruthy();
  });

  it("captures the receipt as a PNG and opens the share sheet", async () => {
    const { getByLabelText } = render(<BatchDetailScreen />);
    fireEvent.press(getByLabelText("Export receipt"));

    await waitFor(() => {
      expect(captureRef).toHaveBeenCalledWith(
        expect.anything(),
        expect.objectContaining({ format: "png", result: "tmpfile" })
      );
    });
    await waitFor(() => {
      expect(Share.share).toHaveBeenCalledWith(
        expect.objectContaining({
          url: expect.stringContaining("zaps-receipt.png"),
          message: expect.stringContaining("BATCH-001"),
        })
      );
    });
  });

  it("shows a loading indicator while exporting", async () => {
    (captureRef as jest.Mock).mockImplementation(
      () =>
        new Promise((resolve) =>
          setTimeout(() => resolve("file:///tmp/r.png"), 50)
        )
    );
    const { getByLabelText, getByTestId } = render(
      <BatchDetailScreen key="export-pending" />
    );
    fireEvent.press(getByLabelText("Export receipt"));
    expect(getByTestId("export-spinner")).toBeTruthy();
  });
});
