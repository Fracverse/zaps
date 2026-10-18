import React from "react";
import { render, fireEvent } from "@testing-library/react-native";
import TransactionsScreen from "../app/(personal)/transactions";
import type { Transaction } from "../src/types/transaction";

jest.mock("@react-native-async-storage/async-storage", () =>
  require("@react-native-async-storage/async-storage/jest/async-storage-mock")
);

jest.mock("expo-router", () => ({
  useRouter: jest.fn(() => ({
    push: jest.fn(),
    back: jest.fn(),
  })),
}));

jest.mock("../src/services/stellarWallet", () => ({
  getLocalKeypair: jest.fn(() => null),
  checkFreighter: jest.fn(() => Promise.resolve(false)),
  speedUpTransaction: jest.fn(() => Promise.resolve()),
}));

jest.mock("../src/components/TransactionFilterSheet", () => ({
  TransactionFilterSheet: () => null,
}));

const mockTransactions: Transaction[] = [
  {
    id: "tx-sdp-1",
    type: "mass_payout",
    status: "completed",
    amount: "100.00",
    asset: "USDC",
    fiatValue: "150,000",
    fiatCurrency: "NGN",
    address: "GABC1234567890DEF1234567890ABCDEF1234567890ABCDEF1234567890AB",
    addressLabel: "Relief Payout",
    timestamp: new Date().toISOString(),
    senderName: "Mercy Relief Org",
    senderAnchor: "Anchor Global",
  },
  {
    id: "tx-standard-1",
    type: "sent",
    status: "completed",
    amount: "25.00",
    asset: "XLM",
    fiatValue: "5,000",
    fiatCurrency: "NGN",
    address: "GDEF1234567890DEF1234567890ABCDEF1234567890ABCDEF1234567890CD",
    addressLabel: "Tolu",
    timestamp: new Date().toISOString(),
  },
  {
    id: "tx-standard-2",
    type: "received",
    status: "completed",
    amount: "50.00",
    asset: "USDC",
    fiatValue: "75,000",
    fiatCurrency: "NGN",
    address: "GXYZ1234567890DEF1234567890ABCDEF1234567890ABCDEF1234567890EF",
    addressLabel: "Amina",
    timestamp: new Date().toISOString(),
  },
];

const mockSetFilters = jest.fn();
const mockRefresh = jest.fn();
const mockLoadMore = jest.fn();

jest.mock("../src/hooks/useTransactions", () => ({
  useTransactions: () => ({
    transactions: mockTransactions,
    loading: false,
    refreshing: false,
    loadingMore: false,
    filters: { type: "all", status: "all", search: "" },
    setFilters: mockSetFilters,
    refresh: mockRefresh,
    loadMore: mockLoadMore,
  }),
}));

describe("TransactionsScreen Disbursement History (Issue #921)", () => {
  it("renders distinct section grouping for Received Mass Payouts separate from standard transfers", () => {
    const { getByTestId, getByText } = render(<TransactionsScreen />);

    expect(getByTestId("mass-payout-section-header")).toBeTruthy();
    expect(getByText("Received Mass Payouts (SDP)")).toBeTruthy();
    expect(getByText("Standard Transfers")).toBeTruthy();
  });

  it("renders unique badge and label for SDP disbursements", () => {
    const { getByTestId, getByText } = render(<TransactionsScreen />);

    expect(getByTestId("sdp-disbursement-badge")).toBeTruthy();
    expect(getByText("SDP Disbursement")).toBeTruthy();
  });

  it("renders sender company and anchor information for mass payouts", () => {
    const { getByText, getByTestId } = render(<TransactionsScreen />);

    expect(getByText("Mercy Relief Org")).toBeTruthy();
    expect(getByTestId("sender-anchor-info")).toBeTruthy();
    expect(getByText("Anchor: Anchor Global")).toBeTruthy();
  });

  it("applies conditional list item rendering based on transaction type", () => {
    const { getByTestId } = render(<TransactionsScreen />);

    expect(getByTestId("tx-row-tx-sdp-1")).toBeTruthy();
    expect(getByTestId("tx-row-tx-standard-1")).toBeTruthy();
    expect(getByTestId("tx-row-tx-standard-2")).toBeTruthy();
  });

  it("switches view when SDP Disbursements tab is selected", () => {
    const { getByTestId, queryByText } = render(<TransactionsScreen />);

    const sdpTab = getByTestId("mass-payout-tab");
    fireEvent.press(sdpTab);

    expect(getByTestId("mass-payout-section-header")).toBeTruthy();
    expect(queryByText("Standard Transfers")).toBeNull();
  });
});
