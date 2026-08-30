import React from "react";
import { Text } from "react-native";
import { render, fireEvent, act } from "@testing-library/react-native";
import { ErrorBoundary } from "../src/components/ErrorBoundary";
import { reportCrash } from "../src/services/crashReporter";

jest.mock("../src/services/crashReporter", () => ({
  reportCrash: jest.fn().mockResolvedValue(undefined),
}));

function Bomb(): React.ReactElement {
  throw new Error("boom");
}

/** Renders a crashing child until `armed` flips to false. */
function ToggleBomb({ armed }: { armed: boolean }): React.ReactElement {
  if (armed) {
    throw new Error("boom");
  }
  return <Text>Recovered</Text>;
}

describe("ErrorBoundary", () => {
  const originalError = console.error;
  beforeAll(() => {
    console.error = jest.fn();
  });
  afterAll(() => {
    console.error = originalError;
  });

  it("renders children when no error is thrown", () => {
    const { getByText } = render(
      <ErrorBoundary>
        <Text>Healthy content</Text>
      </ErrorBoundary>
    );
    expect(getByText("Healthy content")).toBeTruthy();
  });

  it("catches render crashes and shows the friendly fallback", () => {
    const { getByText } = render(
      <ErrorBoundary>
        <Bomb />
      </ErrorBoundary>
    );
    expect(getByText("Oops! Something went wrong")).toBeTruthy();
    expect(getByText("Try Again")).toBeTruthy();
  });

  it("reports the crash via reportCrash with component stack", () => {
    const spy = reportCrash as jest.Mock;
    const { unmount } = render(
      <ErrorBoundary>
        <Bomb />
      </ErrorBoundary>
    );
    unmount();
    expect(spy).toHaveBeenCalled();
    const [error, componentStack] = spy.mock.calls[0];
    expect(error.message).toBe("boom");
    expect(componentStack).toEqual(expect.any(String));
  });

  it("resets the boundary state and invokes onReset when retry is pressed", async () => {
    const onReset = jest.fn(() => {
      void Promise.resolve();
    });
    function Harness(): React.ReactElement {
      const [armed, setArmed] = React.useState(true);
      return (
        <ErrorBoundary
          onReset={() => {
            onReset();
            setArmed(false);
          }}
        >
          <ToggleBomb armed={armed} />
        </ErrorBoundary>
      );
    }
    const { getByText, queryByText } = render(<Harness />);
    expect(getByText("Oops! Something went wrong")).toBeTruthy();

    await act(async () => {
      fireEvent.press(getByText("Try Again"));
    });

    expect(onReset).toHaveBeenCalled();
    expect(getByText("Recovered")).toBeTruthy();
    expect(queryByText("Oops! Something went wrong")).toBeNull();
  });

  it("renders a custom fallback when provided", () => {
    const { getByText } = render(
      <ErrorBoundary fallback={<Text>Custom fallback</Text>}>
        <Bomb />
      </ErrorBoundary>
    );
    expect(getByText("Custom fallback")).toBeTruthy();
  });
});
