jest.mock("@react-native-async-storage/async-storage", () =>
  require("@react-native-async-storage/async-storage/jest/async-storage-mock")
);

import React from "react";
import { fireEvent, render, waitFor } from "@testing-library/react-native";
import * as Linking from "expo-linking";
import {
  PrivySocialButtons,
  buildPrivyAuthUrl,
} from "../src/components/PrivySocialButtons";

// Mock expo-router
jest.mock("expo-router", () => ({
  useRouter: () => ({
    push: jest.fn(),
    replace: jest.fn(),
  }),
}));

describe("PrivySocialButtons", () => {
  beforeEach(() => {
    jest.clearAllMocks();
  });

  it("builds correct Privy auth URLs per provider", () => {
    expect(buildPrivyAuthUrl("google")).toContain("provider=google");
    expect(buildPrivyAuthUrl("apple")).toContain("provider=apple");
    expect(buildPrivyAuthUrl("email")).toContain("provider=email");
  });

  it("renders Google, Apple, and Email social connection buttons", () => {
    const { getByTestId, getByText } = render(
      <PrivySocialButtons testIDPrefix="test-privy" />
    );

    expect(getByTestId("test-privy-google-button")).toBeTruthy();
    expect(getByTestId("test-privy-apple-button")).toBeTruthy();
    expect(getByTestId("test-privy-email-button")).toBeTruthy();

    expect(getByText("Continue with Google")).toBeTruthy();
    expect(getByText("Continue with Apple")).toBeTruthy();
    expect(getByText("Continue with Email")).toBeTruthy();
  });

  it("launches Privy browser overlay on social button tap", async () => {
    const canOpenSpy = jest
      .spyOn(Linking, "canOpenURL")
      .mockResolvedValue(true as never);
    const openUrlSpy = jest
      .spyOn(Linking, "openURL")
      .mockResolvedValue(true as never);

    const { getByTestId } = render(
      <PrivySocialButtons testIDPrefix="test-privy" />
    );

    fireEvent.press(getByTestId("test-privy-google-button"));

    await waitFor(() => {
      expect(canOpenSpy).toHaveBeenCalled();
      expect(openUrlSpy).toHaveBeenCalled();
    });
  });
});
