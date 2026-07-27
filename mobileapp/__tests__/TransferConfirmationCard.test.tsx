import React from "react";
import { render } from "@testing-library/react-native";
import { TransferConfirmationCard } from "../src/components/TransferConfirmationCard";

const mockRecipient = {
  username: "tolu.zaps",
  address: "GABC1234567890DEF1234567890ABCDEF1234567890ABCDEF1234567890AB",
  avatar_url: null as string | null,
};

describe("TransferConfirmationCard", () => {
  it("renders recipient username", () => {
    const { getByText } = render(
      <TransferConfirmationCard
        recipient={mockRecipient}
        amount="500"
        tokenSymbol="XLM"
      />
    );
    expect(getByText("tolu.zaps")).toBeTruthy();
  });

  it("renders truncated address", () => {
    const { getByText } = render(
      <TransferConfirmationCard
        recipient={mockRecipient}
        amount="500"
        tokenSymbol="XLM"
      />
    );
    expect(getByText(/GABC123456/)).toBeTruthy();
    expect(getByText(/7890AB/)).toBeTruthy();
  });

  it("renders amount with token symbol", () => {
    const { getByText } = render(
      <TransferConfirmationCard
        recipient={mockRecipient}
        amount="500"
        tokenSymbol="XLM"
      />
    );
    expect(getByText("500 XLM")).toBeTruthy();
  });

  it("renders description when provided", () => {
    const { getByText } = render(
      <TransferConfirmationCard
        recipient={mockRecipient}
        amount="500"
        tokenSymbol="XLM"
        description="Lunch money"
      />
    );
    expect(getByText("Lunch money")).toBeTruthy();
  });

  it("renders 'No note' when description is empty", () => {
    const { getByText } = render(
      <TransferConfirmationCard
        recipient={mockRecipient}
        amount="500"
        tokenSymbol="XLM"
      />
    );
    expect(getByText("No note")).toBeTruthy();
  });

  it("does not render verification badge for non-verified recipient", () => {
    const { queryByLabelText } = render(
      <TransferConfirmationCard
        recipient={mockRecipient}
        amount="500"
        tokenSymbol="XLM"
      />
    );
    expect(queryByLabelText("Verified recipient")).toBeNull();
  });

  it("renders verification badge for verified recipient", () => {
    const { getByLabelText } = render(
      <TransferConfirmationCard
        recipient={{ ...mockRecipient, isVerified: true }}
        amount="500"
        tokenSymbol="XLM"
      />
    );
    expect(getByLabelText("Verified recipient")).toBeTruthy();
  });

  it("renders avatar with fallback initials when no avatar_url", () => {
    const { getByLabelText } = render(
      <TransferConfirmationCard
        recipient={mockRecipient}
        amount="500"
        tokenSymbol="XLM"
      />
    );
    expect(getByLabelText("Avatar placeholder for tolu.zaps")).toBeTruthy();
  });

  it("renders avatar with image when avatar_url is provided", () => {
    const { getByLabelText } = render(
      <TransferConfirmationCard
        recipient={{ ...mockRecipient, avatar_url: "https://example.com/a.jpg" }}
        amount="500"
        tokenSymbol="XLM"
      />
    );
    expect(getByLabelText("Avatar for tolu.zaps")).toBeTruthy();
  });
});
