import React from "react";
import { Text } from "react-native";
import { render, act, fireEvent } from "@testing-library/react-native";
import AsyncStorage from "@react-native-async-storage/async-storage";
import { ThemeProvider, useThemeContext } from "../src/contexts/ThemeProvider";
import { Colors } from "../src/constants/theme";

jest.mock("@react-native-async-storage/async-storage", () => {
  // eslint-disable-next-line @typescript-eslint/no-require-imports
  return require("@react-native-async-storage/async-storage/jest/async-storage-mock");
});

const mockSystemIsLight = jest.fn(() => "light");
jest.mock("react-native/Libraries/Utilities/useColorScheme", () => ({
  __esModule: true,
  default: () => mockSystemIsLight(),
}));

function Probe() {
  const { theme, isDark, isHighContrast, preference, setPreference } =
    useThemeContext();
  return (
    <>
      <Text testID="bg">{theme.background}</Text>
      <Text testID="isDark">{String(isDark)}</Text>
      <Text testID="hc">{String(isHighContrast)}</Text>
      <Text testID="pref">{preference}</Text>
      <Text testID="set-dark" onPress={() => setPreference("dark")}>
        go-dark
      </Text>
      <Text testID="set-hc" onPress={() => setPreference("high-contrast")}>
        go-hc
      </Text>
    </>
  );
}

describe("ThemeProvider", () => {
  beforeEach(async () => {
    await AsyncStorage.clear();
    mockSystemIsLight.mockReturnValue("light");
  });

  it("defaults to the system palette", () => {
    const { getByTestId } = render(
      <ThemeProvider>
        <Probe />
      </ThemeProvider>
    );
    expect(getByTestId("bg").props.children).toBe(Colors.light.background);
    expect(getByTestId("pref").props.children).toBe("system");
  });

  it("resolves dark palette when system is dark", () => {
    mockSystemIsLight.mockReturnValue("dark");
    const { getByTestId } = render(
      <ThemeProvider>
        <Probe />
      </ThemeProvider>
    );
    expect(getByTestId("bg").props.children).toBe(Colors.dark.background);
    expect(getByTestId("isDark").props.children).toBe("true");
  });

  it("applies dark theme immediately and persists the preference", async () => {
    const { getByTestId } = render(
      <ThemeProvider>
        <Probe />
      </ThemeProvider>
    );
    await act(async () => {
      fireEvent.press(getByTestId("set-dark"));
    });
    expect(getByTestId("bg").props.children).toBe(Colors.dark.background);
    expect(await AsyncStorage.getItem("zaps:theme_preference")).toBe("dark");
  });

  it("applies the high-contrast palette and flags high contrast", async () => {
    const { getByTestId } = render(
      <ThemeProvider>
        <Probe />
      </ThemeProvider>
    );
    await act(async () => {
      fireEvent.press(getByTestId("set-hc"));
    });
    expect(getByTestId("bg").props.children).toBe(
      Colors.highContrast.background
    );
    expect(getByTestId("hc").props.children).toBe("true");
  });

  it("rehydrates the persisted preference on mount", async () => {
    await AsyncStorage.setItem("zaps:theme_preference", "dark");
    const { getByTestId } = render(
      <ThemeProvider>
        <Probe />
      </ThemeProvider>
    );
    await act(async () => {
      await new Promise((r) => setTimeout(r, 0));
    });
    expect(getByTestId("pref").props.children).toBe("dark");
    expect(getByTestId("bg").props.children).toBe(Colors.dark.background);
  });
});
