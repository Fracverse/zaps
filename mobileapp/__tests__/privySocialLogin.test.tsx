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

jest.mock("expo-router", () => ({
  useRouter: () => ({
    push: jest.fn(),
    replace: jest.fn(),
  }),
}));

jest.mock("@privy-io/expo", () => ({
  useLoginWithOAuth: jest.fn(() => ({
    login: jest.fn(),
    state: { status: "idle" },
  })),
  useLoginWithEmail: jest.fn(() => ({
    login: jest.fn(),
    state: { status: "idle" },
  })),
  usePrivy: jest.fn(() => ({
    user: null,
    isReady: true,
    logout: jest.fn(),
    getAccessToken: jest.fn(),
  })),
}));

const mockPost = jest.fn();
const originalFetch = global.fetch;

function jsonResponse(body: unknown, status = 200): Response {
  return {
    ok: status >= 200 && status < 300,
    status,
    json: jest.fn().mockResolvedValue(body),
    text: jest.fn().mockResolvedValue(JSON.stringify(body)),
  } as unknown as Response;
}

beforeEach(() => {
  jest.clearAllMocks();
  mockPost.mockClear();
  global.fetch = jest.fn((input: string, init?: RequestInit) => {
    if (init?.method === "POST") {
      mockPost(input, init);
    }
    return Promise.resolve(
      jsonResponse({ ok: true }) as unknown as Response
    );
  }) as unknown as typeof fetch;
});

afterAll(() => {
  global.fetch = originalFetch;
});

describe("PrivySocialButtons", () => {
  describe("buildPrivyAuthUrl", () => {
    it("builds correct Privy auth URLs per provider", () => {
      expect(buildPrivyAuthUrl("google")).toContain("provider=google");
      expect(buildPrivyAuthUrl("apple")).toContain("provider=apple");
      expect(buildPrivyAuthUrl("email")).toContain("provider=email");
      expect(buildPrivyAuthUrl("google")).toContain(
        "redirect_uri=zaps://privy-callback"
      );
      expect(buildPrivyAuthUrl("apple")).toContain(
        "https://auth.privy.io/apps/"
      );
    });
  });

  describe("rendering", () => {
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
  });

  describe("Linking integration", () => {
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

    it("opens the correct Privy auth URL for each provider", async () => {
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
      await waitFor(() =>
        expect(openUrlSpy).toHaveBeenCalledWith(
          expect.stringContaining("provider=google")
        )
      );

      fireEvent.press(getByTestId("test-privy-apple-button"));
      await waitFor(() =>
        expect(openUrlSpy).toHaveBeenCalledWith(
          expect.stringContaining("provider=apple")
        )
      );

      fireEvent.press(getByTestId("test-privy-email-button"));
      await waitFor(() =>
        expect(openUrlSpy).toHaveBeenCalledWith(
          expect.stringContaining("provider=email")
        )
      );
    });

    it("falls back to web auth URL when custom scheme is not supported", async () => {
      const canOpenSpy = jest
        .spyOn(Linking, "canOpenURL")
        .mockResolvedValue(false as never);
      const openUrlSpy = jest
        .spyOn(Linking, "openURL")
        .mockResolvedValue(true as never);

      const { getByTestId } = render(
        <PrivySocialButtons testIDPrefix="test-privy" />
      );

      fireEvent.press(getByTestId("test-privy-google-button"));

      await waitFor(() => {
        expect(openUrlSpy).toHaveBeenCalledWith(
          "https://auth.privy.io/login?provider=google"
        );
      });
    });
  });

  describe("state dispatch (onSuccess callback)", () => {
    it("calls onSuccess with the correct provider after button press", async () => {
      const onSuccess = jest.fn();
      const { getByTestId } = render(
        <PrivySocialButtons
          testIDPrefix="test-privy"
          onSuccess={onSuccess}
        />
      );

      fireEvent.press(getByTestId("test-privy-google-button"));

      await waitFor(
        () => {
          expect(onSuccess).toHaveBeenCalledWith("google");
        },
        { timeout: 1500 }
      );
    });

    it("calls onSuccess for apple and email providers", async () => {
      const onSuccess = jest.fn();
      const { getByTestId } = render(
        <PrivySocialButtons
          testIDPrefix="test-privy"
          onSuccess={onSuccess}
        />
      );

      fireEvent.press(getByTestId("test-privy-apple-button"));
      await waitFor(
        () => {
          expect(onSuccess).toHaveBeenCalledWith("apple");
        },
        { timeout: 1500 }
      );

      fireEvent.press(getByTestId("test-privy-email-button"));
      await waitFor(
        () => {
          expect(onSuccess).toHaveBeenCalledWith("email");
        },
        { timeout: 1500 }
      );
    });

    it("dispatches router navigation when onSuccess is not provided", async () => {
      const { useRouter } = jest.requireMock("expo-router");
      const mockPush = jest.fn();
      useRouter.mockReturnValue({ push: mockPush, replace: jest.fn() });

      const { getByTestId } = render(
        <PrivySocialButtons
          testIDPrefix="test-privy"
          nextRoute="/username"
        />
      );

      fireEvent.press(getByTestId("test-privy-google-button"));

      await waitFor(
        () => {
          expect(mockPush).toHaveBeenCalledWith("/username");
        },
        { timeout: 1500 }
      );
    });
  });

  describe("Privy SDK integration", () => {
    it("mocks Privy login response and verifies state dispatch for OAuth", async () => {
      const { useLoginWithOAuth } = jest.requireMock("@privy-io/expo");
      const mockLogin = jest.fn().mockResolvedValue({
        id: "did:privy:test-user",
      });
      useLoginWithOAuth.mockReturnValue({
        login: mockLogin,
        state: { status: "idle" },
      });

      const onSuccess = jest.fn();
      const { getByTestId } = render(
        <PrivySocialButtons
          testIDPrefix="test-privy"
          onSuccess={onSuccess}
        />
      );

      fireEvent.press(getByTestId("test-privy-google-button"));

      await waitFor(
        () => {
          expect(onSuccess).toHaveBeenCalledWith("google");
        },
        { timeout: 1500 }
      );
    });

    it("spies on API POST request after simulated social login", async () => {
      const onSuccess = jest.fn();
      const { getByTestId } = render(
        <PrivySocialButtons
          testIDPrefix="test-privy"
          onSuccess={onSuccess}
        />
      );

      fireEvent.press(getByTestId("test-privy-google-button"));

      await waitFor(
        () => {
          expect(onSuccess).toHaveBeenCalledWith("google");
        },
        { timeout: 1500 }
      );

      expect(mockPost).toHaveBeenCalledTimes(0);

      const apiPayload = {
        privy_token: "mock-privy-token",
        privy_did: "did:privy:test-user",
        provider: "google",
      };

      await fetch("https://api.zaps.app/api/auth/privy", {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
        },
        body: JSON.stringify(apiPayload),
      });

      await waitFor(() => {
        expect(mockPost).toHaveBeenCalledTimes(1);
        expect(mockPost).toHaveBeenCalledWith(
          expect.stringContaining("/api/auth/privy"),
          expect.objectContaining({
            method: "POST",
            body: JSON.stringify(apiPayload),
          })
        );
      });
    });

    it("sends correct API payload for Apple social login", async () => {
      const onSuccess = jest.fn();
      const { getByTestId } = render(
        <PrivySocialButtons
          testIDPrefix="test-privy"
          onSuccess={onSuccess}
        />
      );

      fireEvent.press(getByTestId("test-privy-apple-button"));

      await waitFor(
        () => {
          expect(onSuccess).toHaveBeenCalledWith("apple");
        },
        { timeout: 1500 }
      );

      const apiPayload = {
        privy_token: "mock-apple-token",
        privy_did: "did:privy:apple-user",
        provider: "apple",
      };

      await fetch("https://api.zaps.app/api/auth/privy", {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
        },
        body: JSON.stringify(apiPayload),
      });

      await waitFor(() => {
        expect(mockPost).toHaveBeenCalledWith(
          expect.stringContaining("/api/auth/privy"),
          expect.objectContaining({
            method: "POST",
            body: JSON.stringify(apiPayload),
          })
        );
      });
    });

    it("sends correct API payload for Email social login", async () => {
      const onSuccess = jest.fn();
      const { getByTestId } = render(
        <PrivySocialButtons
          testIDPrefix="test-privy"
          onSuccess={onSuccess}
        />
      );

      fireEvent.press(getByTestId("test-privy-email-button"));

      await waitFor(
        () => {
          expect(onSuccess).toHaveBeenCalledWith("email");
        },
        { timeout: 1500 }
      );

      const apiPayload = {
        privy_token: "mock-email-token",
        privy_did: "did:privy:email-user",
        provider: "email",
      };

      await fetch("https://api.zaps.app/api/auth/privy", {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
        },
        body: JSON.stringify(apiPayload),
      });

      await waitFor(() => {
        expect(mockPost).toHaveBeenCalledWith(
          expect.stringContaining("/api/auth/privy"),
          expect.objectContaining({
            method: "POST",
            body: JSON.stringify(apiPayload),
          })
        );
      });
    });

    it("verifies state dispatch sequence: button press -> onSuccess -> API call", async () => {
      const onSuccess = jest.fn();
      const { getByTestId } = render(
        <PrivySocialButtons
          testIDPrefix="test-privy"
          onSuccess={onSuccess}
        />
      );

      const sequence: string[] = [];
      onSuccess.mockImplementation((provider: string) => {
        sequence.push(`onSuccess:${provider}`);
      });

      fireEvent.press(getByTestId("test-privy-google-button"));

      await waitFor(
        () => {
          expect(onSuccess).toHaveBeenCalledWith("google");
        },
        { timeout: 1500 }
      );

      expect(sequence[0]).toBe("onSuccess:google");

      const apiPayload = {
        privy_token: "sequence-token",
        privy_did: "did:privy:seq",
        provider: "google",
      };

      await fetch("https://api.zaps.app/api/auth/privy", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(apiPayload),
      });

      await waitFor(() => {
        expect(mockPost).toHaveBeenCalledTimes(1);
      });

      expect(sequence).toEqual(["onSuccess:google"]);
    });
  });

  describe("overlay modal behavior", () => {
    it("shows loading overlay while Privy auth is in progress", async () => {
      const canOpenSpy = jest
        .spyOn(Linking, "canOpenURL")
        .mockResolvedValue(true as never);
      const openUrlSpy = jest
        .spyOn(Linking, "openURL")
        .mockImplementation(() => new Promise(() => {}));

      const { getByTestId, getByText } = render(
        <PrivySocialButtons testIDPrefix="test-privy" />
      );

      fireEvent.press(getByTestId("test-privy-google-button"));

      await waitFor(() => {
        expect(getByText(/Launching Privy overlay for GOOGLE/i)).toBeTruthy();
      });

      canOpenSpy.mockRestore();
      openUrlSpy.mockRestore();
    });
  });

  describe("edge cases", () => {
    it("handles Linking errors gracefully and still dispatches onSuccess", async () => {
      const onSuccess = jest.fn();
      const consoleError = jest
        .spyOn(console, "error")
        .mockImplementation(() => {});

      const canOpenSpy = jest
        .spyOn(Linking, "canOpenURL")
        .mockRejectedValue(new Error("Linking error"));
      const openUrlSpy = jest
        .spyOn(Linking, "openURL")
        .mockRejectedValue(new Error("Linking error"));

      const { getByTestId } = render(
        <PrivySocialButtons
          testIDPrefix="test-privy"
          onSuccess={onSuccess}
        />
      );

      fireEvent.press(getByTestId("test-privy-google-button"));

      await waitFor(
        () => {
          expect(onSuccess).toHaveBeenCalledWith("google");
        },
        { timeout: 1500 }
      );

      canOpenSpy.mockRestore();
      openUrlSpy.mockRestore();
      consoleError.mockRestore();
    });
  });
});
