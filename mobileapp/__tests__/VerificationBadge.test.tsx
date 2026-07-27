import React from "react";
import { render } from "@testing-library/react-native";
import { VerificationBadge } from "../src/components/VerificationBadge";

describe("VerificationBadge", () => {
  it("renders with accessible label", () => {
    const { getByLabelText } = render(<VerificationBadge />);
    expect(getByLabelText("Verified recipient")).toBeTruthy();
  });

  it("renders with custom size", () => {
    const { getByLabelText } = render(<VerificationBadge size={24} />);
    expect(getByLabelText("Verified recipient")).toBeTruthy();
  });
});
