import React from "react";
import { Share } from "react-native";
import { render, fireEvent, waitFor } from "@testing-library/react-native";
import BatchDetailScreen from "../app/transaction/batch/[id]";
import { captureRef } from "react-native-view-shot";
import { fetchBatchPayoutDetail } from "../src/services/batchPayoutService";
import { useLocalSearchParams } from "expo-router";

jest.mock("../src/services/batchPayoutService", () => ({
  fetchBatchPayoutDetail: jest.fn(),
}));

jest.mock("react-native-view-shot", () => ({
  captureRef: jest.fn(() => Promise.resolve("file:///tmp/zaps-receipt.png")),
}));

jest.mock("expo-router", () => ({
  useLocalSearchParams: jest.fn(() => ({ id: "BATCH-001" })),
  useRouter: () => ({ back: jest.fn() }),
  Stack: {
    Screen: () => null,
  },
}));

describe("BatchReceiptExport", () => {
  beforeEach(() => {
    jest.clearAllMocks();
    (useLocalSearchParams as jest.Mock).mockReturnValue({ id: "BATCH-001" });
    (fetchBatchPayoutDetail as jest.Mock).mockResolvedValue({
      status: "PROCESSING",
      summary: {
        id: "BATCH-001",
        totalAmount: "150.00",
        currency: "USDC",
        itemCount: 1,
        completedCount: 0,
        failedCount: 0,
        createdAt: "2026-07-20T10:00:00Z",
      },
      items: [
        {
          id: "recipient-1",
          recipientName: "User 1",
          recipientAddress: "GTEST",
          amount: "150.00",
          currency: "USDC",
          status: "pending",
        },
      ],
    });
    jest
      .spyOn(Share, "share")
      .mockResolvedValue({ action: "sharedAction" } as any);
  });

  it("renders the export/share button in the header", async () => {
    const { findByLabelText } = render(<BatchDetailScreen />);
    expect(await findByLabelText("Export receipt")).toBeTruthy();
  });

  it("captures the receipt as a PNG and opens the share sheet", async () => {
    const { getByLabelText } = render(<BatchDetailScreen />);
    fireEvent.press(await waitFor(() => getByLabelText("Export receipt")));

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
    fireEvent.press(await waitFor(() => getByLabelText("Export receipt")));
    expect(getByTestId("export-spinner")).toBeTruthy();
  });

  it("shows an explicit not-found state with no mock recipient rows", async () => {
    (useLocalSearchParams as jest.Mock).mockReturnValue({
      id: "missing-batch",
    });
    (fetchBatchPayoutDetail as jest.Mock).mockResolvedValue(null);
    const { findByTestId, queryByText } = render(<BatchDetailScreen />);

    expect(await findByTestId("batch-not-found")).toBeTruthy();
    expect(queryByText("Recipient 1")).toBeNull();
    expect(queryByText("Batch #BATCH-001")).toBeNull();
  });
});
