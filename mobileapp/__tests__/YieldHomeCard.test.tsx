import React from "react";
import { fireEvent, render } from "@testing-library/react-native";
import YieldHomeCard from "../src/components/YieldHomeCard";

const noop = () => {};

describe("YieldHomeCard", () => {
  it("renders the earning balance and active status", () => {
    const { getByText } = render(
      <YieldHomeCard
        earningBalance="₦3,280.45"
        autoYieldEnabled
        onToggleAutoYield={noop}
      />
    );

    expect(getByText("Earning Balance")).toBeTruthy();
    expect(getByText("₦3,280.45")).toBeTruthy();
    expect(getByText("Your money is working")).toBeTruthy();
    expect(getByText("Auto-yield")).toBeTruthy();
  });

  it("reflects the current auto-yield toggle state", () => {
    const { getByTestId } = render(
      <YieldHomeCard
        earningBalance="₦0.00"
        autoYieldEnabled={false}
        onToggleAutoYield={noop}
      />
    );

    expect(getByTestId("auto-yield-switch").props.value).toBe(false);
  });

  it("reflects the enabled state when auto-yield is on", () => {
    const { getByTestId } = render(
      <YieldHomeCard
        earningBalance="₦1,250.00"
        autoYieldEnabled={true}
        onToggleAutoYield={noop}
      />
    );

    expect(getByTestId("auto-yield-switch").props.value).toBe(true);
  });

  it("invokes onToggleAutoYield when the switch changes", () => {
    const onToggleAutoYield = jest.fn();
    const { getByTestId } = render(
      <YieldHomeCard
        earningBalance="₦0.00"
        autoYieldEnabled={false}
        onToggleAutoYield={onToggleAutoYield}
      />
    );

    fireEvent(getByTestId("auto-yield-switch"), "valueChange", true);

    expect(onToggleAutoYield).toHaveBeenCalledWith(true);
  });

  it("can turn auto-yield off by firing the reverse value", () => {
    const onToggleAutoYield = jest.fn();
    const { getByTestId } = render(
      <YieldHomeCard
        earningBalance="₦0.00"
        autoYieldEnabled={true}
        onToggleAutoYield={onToggleAutoYield}
      />
    );

    fireEvent(getByTestId("auto-yield-switch"), "valueChange", false);

    expect(onToggleAutoYield).toHaveBeenCalledWith(false);
  });
});
