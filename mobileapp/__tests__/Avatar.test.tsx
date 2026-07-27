import React from "react";
import { render } from "@testing-library/react-native";
import { Avatar } from "../src/components/Avatar";

describe("Avatar", () => {
  it("renders initials when no uri is provided", () => {
    const { getByText } = render(<Avatar name="tolu" />);
    expect(getByText("T")).toBeTruthy();
  });

  it("renders two initials for multi-word names", () => {
    const { getByText } = render(<Avatar name="john doe" />);
    expect(getByText("JD")).toBeTruthy();
  });

  it("renders fallback character for single-character names", () => {
    const { getByText } = render(<Avatar name="x" />);
    expect(getByText("X")).toBeTruthy();
  });

  it("renders image when uri is provided", () => {
    const { getByLabelText } = render(
      <Avatar uri="https://example.com/avatar.jpg" name="tolu.zaps" />
    );
    expect(getByLabelText("Avatar for tolu.zaps")).toBeTruthy();
  });

  it("renders initials fallback with accessibility label", () => {
    const { getByLabelText } = render(<Avatar name="tolu.zaps" />);
    expect(getByLabelText("Avatar placeholder for tolu.zaps")).toBeTruthy();
  });
});
