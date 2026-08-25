import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { render, screen, waitFor, act } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import React from "react";

const push = vi.fn();
vi.mock("next/navigation", () => ({
  useRouter: () => ({ push }),
}));

const searchUsers = vi.fn();
vi.mock("@/lib/api", () => ({
  api: { searchUsers: (...a: unknown[]) => searchUsers(...a) },
}));

import SearchBar from "@/components/SearchBar";

const ROWS = [
  { username: "alice", public_key: "GA1ALICE", registered_at: "2026-01-01" },
  { username: "alicia", public_key: "GA2ALICIA", registered_at: "2026-01-02" },
  { username: "bob", public_key: "GA3BOB", registered_at: "2026-01-03" },
];

/** Rendered result rows — one <li> per match in the dropdown. */
function renderedRowCount(): number {
  return screen.queryAllByRole("listitem").length;
}

/** The debounce inside SearchBar is 300 ms. */
const DEBOUNCE_MS = 300;

async function typeQuery(text: string) {
  const user = userEvent.setup({ advanceTimers: vi.advanceTimersByTime });
  const input = screen.getByLabelText("Search users by username");
  await user.type(input, text);
  await act(async () => {
    vi.advanceTimersByTime(DEBOUNCE_MS);
  });
  return input;
}

describe("SearchBar filtering", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    push.mockReset();
    searchUsers.mockReset();
    // Default: filter the fixture by the query, as the backend would.
    searchUsers.mockImplementation(async (q: string) =>
      ROWS.filter((r) => r.username.includes(q.toLowerCase())),
    );
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("renders no rows before anything is typed", () => {
    render(<SearchBar />);
    expect(renderedRowCount()).toBe(0);
  });

  it("filters rows down to the matching subset", async () => {
    render(<SearchBar />);
    await typeQuery("ali");

    await waitFor(() => expect(renderedRowCount()).toBe(2));
    expect(screen.getByText("alice")).toBeInTheDocument();
    expect(screen.getByText("alicia")).toBeInTheDocument();
    expect(screen.queryByText("bob")).not.toBeInTheDocument();
  });

  it("narrows to a single row as the query gets more specific", async () => {
    render(<SearchBar />);
    await typeQuery("alice");

    await waitFor(() => expect(renderedRowCount()).toBe(1));
    expect(screen.getByText("alice")).toBeInTheDocument();
  });

  it("renders nothing when the query matches no rows", async () => {
    render(<SearchBar />);
    await typeQuery("zzz");

    await waitFor(() => expect(searchUsers).toHaveBeenCalled());
    expect(renderedRowCount()).toBe(0);
  });

  it("shows the public key alongside each username", async () => {
    render(<SearchBar />);
    await typeQuery("bob");

    await waitFor(() => expect(renderedRowCount()).toBe(1));
    expect(screen.getByText("GA3BOB")).toBeInTheDocument();
  });

  describe("query threshold", () => {
    it("does not search for a single character", async () => {
      render(<SearchBar />);
      await typeQuery("a");

      expect(searchUsers).not.toHaveBeenCalled();
      expect(renderedRowCount()).toBe(0);
    });

    it("searches once two characters are present", async () => {
      render(<SearchBar />);
      await typeQuery("al");

      await waitFor(() => expect(searchUsers).toHaveBeenCalledWith("al"));
    });

    it("trims whitespace before searching", async () => {
      render(<SearchBar />);
      await typeQuery("  bob  ");

      await waitFor(() => expect(searchUsers).toHaveBeenCalledWith("bob"));
    });
  });

  describe("debouncing", () => {
    it("issues one request for a burst of keystrokes", async () => {
      render(<SearchBar />);
      const user = userEvent.setup({ advanceTimers: vi.advanceTimersByTime });
      const input = screen.getByLabelText("Search users by username");

      await user.type(input, "alice");
      // Still inside the debounce window — nothing should have fired yet.
      expect(searchUsers).not.toHaveBeenCalled();

      await act(async () => {
        vi.advanceTimersByTime(DEBOUNCE_MS);
      });

      await waitFor(() => expect(searchUsers).toHaveBeenCalledTimes(1));
      expect(searchUsers).toHaveBeenCalledWith("alice");
    });

    it("clears results when the query drops below the threshold", async () => {
      render(<SearchBar />);
      await typeQuery("ali");
      await waitFor(() => expect(renderedRowCount()).toBe(2));

      const user = userEvent.setup({ advanceTimers: vi.advanceTimersByTime });
      const input = screen.getByLabelText("Search users by username");
      await user.clear(input);
      await user.type(input, "a");
      await act(async () => {
        vi.advanceTimersByTime(DEBOUNCE_MS);
      });

      expect(renderedRowCount()).toBe(0);
    });
  });

  describe("failure handling", () => {
    it("renders no rows and does not throw when the search request fails", async () => {
      searchUsers.mockRejectedValue(new Error("network down"));
      render(<SearchBar />);

      await typeQuery("ali");

      await waitFor(() => expect(searchUsers).toHaveBeenCalled());
      expect(renderedRowCount()).toBe(0);
    });
  });

  describe("selection", () => {
    it("navigates to the filtered transactions view for the chosen user", async () => {
      render(<SearchBar />);
      await typeQuery("bob");
      await waitFor(() => expect(renderedRowCount()).toBe(1));

      const user = userEvent.setup({ advanceTimers: vi.advanceTimersByTime });
      await user.click(screen.getByRole("button", { name: /bob/i }));

      expect(push).toHaveBeenCalledWith("/dashboard/transactions?user=bob");
    });

    it("clears the query and closes the dropdown after selection", async () => {
      render(<SearchBar />);
      const input = await typeQuery("bob");
      await waitFor(() => expect(renderedRowCount()).toBe(1));

      const user = userEvent.setup({ advanceTimers: vi.advanceTimersByTime });
      await user.click(screen.getByRole("button", { name: /bob/i }));

      expect((input as HTMLInputElement).value).toBe("");
      expect(renderedRowCount()).toBe(0);
    });

    it("encodes a username that needs escaping", async () => {
      searchUsers.mockResolvedValue([
        { username: "a b&c", public_key: "GAX", registered_at: "2026-01-01" },
      ]);
      render(<SearchBar />);
      await typeQuery("ab");
      await waitFor(() => expect(renderedRowCount()).toBe(1));

      const user = userEvent.setup({ advanceTimers: vi.advanceTimersByTime });
      await user.click(screen.getByRole("button", { name: /a b&c/i }));

      expect(push).toHaveBeenCalledWith(
        `/dashboard/transactions?user=${encodeURIComponent("a b&c")}`,
      );
    });
  });
});
