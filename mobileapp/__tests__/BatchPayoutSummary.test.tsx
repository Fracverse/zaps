import React from "react";
import { render } from "@testing-library/react-native";
import BatchPayoutSummary from "../src/components/BatchPayoutSummary";
import type {
  BatchPayoutSummary as SummaryType,
  BatchPayoutItem,
} from "../src/types/batchPayout";

const baseSummary: SummaryType = {
  id: "batch-001",
  totalAmount: "₦45,000.00",
  currency: "NGN",
  itemCount: 3,
  completedCount: 2,
  failedCount: 0,
  createdAt: "2026-07-20T10:00:00Z",
};

const baseItems: BatchPayoutItem[] = [
  {
    id: "item-1",
    recipientName: "Alice Johnson",
    recipientAddress: "GABC1234...",
    amount: "₦10,000.00",
    currency: "NGN",
    status: "completed",
  },
  {
    id: "item-2",
    recipientName: "Bob Smith",
    recipientAddress: "GDEF5678...",
    amount: "₦15,000.00",
    currency: "NGN",
    status: "completed",
  },
  {
    id: "item-3",
    recipientName: "Carol White",
    recipientAddress: "GHIJ9012...",
    amount: "₦20,000.00",
    currency: "NGN",
    status: "pending",
  },
];

describe("BatchPayoutSummary", () => {
  it("renders total amount and item count", () => {
    const { getByText } = render(
      <BatchPayoutSummary summary={baseSummary} items={baseItems} />
    );

    expect(getByText("₦45,000.00 NGN")).toBeTruthy();
    expect(getByText("3 items")).toBeTruthy();
  });

  it("renders completed and failed counts", () => {
    const { getByText } = render(
      <BatchPayoutSummary summary={baseSummary} items={baseItems} />
    );

    expect(getByText("2 completed")).toBeTruthy();
    expect(getByText("1 pending")).toBeTruthy();
  });

  it("renders all item rows", () => {
    const { getByText } = render(
      <BatchPayoutSummary summary={baseSummary} items={baseItems} />
    );

    expect(getByText("Alice Johnson")).toBeTruthy();
    expect(getByText("Bob Smith")).toBeTruthy();
    expect(getByText("Carol White")).toBeTruthy();
  });

  it("shows correct status badges for pending, completed, and failed items", () => {
    const mixedItems: BatchPayoutItem[] = [
      { ...baseItems[0], status: "completed" },
      { ...baseItems[1], status: "failed" },
      { ...baseItems[2], status: "pending" },
    ];
    const summary = { ...baseSummary, completedCount: 1, failedCount: 1 };

    const { getByText } = render(
      <BatchPayoutSummary summary={summary} items={mixedItems} />
    );

    expect(getByText("Completed")).toBeTruthy();
    expect(getByText("Failed")).toBeTruthy();
    expect(getByText("Pending")).toBeTruthy();
  });

  it("handles empty items list", () => {
    const { getByText } = render(
      <BatchPayoutSummary summary={{ ...baseSummary, itemCount: 0, completedCount: 0 }} items={[]} />
    );

    expect(getByText("No payout items")).toBeTruthy();
  });

  it("handles all-completed state", () => {
    const summary: SummaryType = {
      ...baseSummary,
      completedCount: 3,
      failedCount: 0,
    };

    const { getByText, queryByText } = render(
      <BatchPayoutSummary summary={summary} items={baseItems} />
    );

    expect(getByText("3 completed")).toBeTruthy();
    expect(queryByText("failed")).toBeNull();
    expect(queryByText("pending")).toBeNull();
  });

  it("handles all-failed state", () => {
    const summary: SummaryType = {
      ...baseSummary,
      completedCount: 0,
      failedCount: 3,
    };
    const failedItems = baseItems.map((i) => ({ ...i, status: "failed" as const }));

    const { getByText, queryByText } = render(
      <BatchPayoutSummary summary={summary} items={failedItems} />
    );

    expect(getByText("3 failed")).toBeTruthy();
    expect(queryByText("completed")).toBeNull();
  });

  it("renders singular 'item' label for single item", () => {
    const summary: SummaryType = {
      ...baseSummary,
      itemCount: 1,
      completedCount: 1,
      failedCount: 0,
    };

    const { getByText } = render(
      <BatchPayoutSummary summary={summary} items={[baseItems[0]]} />
    );

    expect(getByText("1 item")).toBeTruthy();
  });
});
