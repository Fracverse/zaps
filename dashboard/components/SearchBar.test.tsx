import { useState } from "react";
import { describe, expect, it, vi } from "vitest";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import SearchBar from "./SearchBar";

const ROWS = [
  { id: "1", user: "alice", action: "deposit" },
  { id: "2", user: "bob", action: "withdraw" },
  { id: "3", user: "carol", action: "deposit" },
  { id: "4", user: "alicia", action: "claim" },
];

/** Minimal table that filters its rows via SearchBar, mirroring dashboard usage. */
function FilterableTable({ onSearch }: { onSearch?: (q: string) => void }) {
  const [query, setQuery] = useState("");
  const term = query.toLowerCase();
  const visible = term
    ? ROWS.filter((r) => [r.user, r.action].some((v) => v.includes(term)))
    : ROWS;

  return (
    <div>
      <SearchBar
        label="Search rows"
        onSearch={(q) => {
          setQuery(q);
          onSearch?.(q);
        }}
      />
      <table>
        <tbody>
          {visible.map((r) => (
            <tr key={r.id}>
              <td>{r.user}</td>
              <td>{r.action}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function bodyRows() {
  return within(screen.getByRole("table")).queryAllByRole("row");
}

describe("SearchBar integration", () => {
  it("renders all rows before any input", () => {
    render(<FilterableTable />);
    expect(screen.getByLabelText("Search rows")).toHaveValue("");
    expect(bodyRows()).toHaveLength(ROWS.length);
  });

  it("calls onSearch with the typed query", async () => {
    const user = userEvent.setup();
    const onSearch = vi.fn();
    render(<FilterableTable onSearch={onSearch} />);

    await user.type(screen.getByLabelText("Search rows"), "bob");

    expect(onSearch).toHaveBeenCalledTimes(3);
    expect(onSearch).toHaveBeenNthCalledWith(1, "b");
    expect(onSearch).toHaveBeenNthCalledWith(2, "bo");
    expect(onSearch).toHaveBeenLastCalledWith("bob");
  });

  it("filters table rows as the user types", async () => {
    const user = userEvent.setup();
    render(<FilterableTable />);
    const input = screen.getByLabelText("Search rows");

    await user.type(input, "ali");
    expect(bodyRows()).toHaveLength(2);
    expect(screen.getByText("alice")).toBeInTheDocument();
    expect(screen.getByText("alicia")).toBeInTheDocument();
    expect(screen.queryByText("bob")).not.toBeInTheDocument();

    await user.type(input, "c");
    expect(bodyRows()).toHaveLength(2);

    await user.type(input, "i");
    expect(bodyRows()).toHaveLength(1);
    expect(screen.getByText("alicia")).toBeInTheDocument();
  });

  it("matches against any column", async () => {
    const user = userEvent.setup();
    render(<FilterableTable />);

    await user.type(screen.getByLabelText("Search rows"), "deposit");
    expect(bodyRows()).toHaveLength(2);
  });

  it("trims whitespace before emitting the query", async () => {
    const user = userEvent.setup();
    const onSearch = vi.fn();
    render(<FilterableTable onSearch={onSearch} />);

    await user.type(screen.getByLabelText("Search rows"), "  bob ");
    expect(onSearch).toHaveBeenLastCalledWith("bob");
    expect(bodyRows()).toHaveLength(1);
  });

  it("shows no rows when nothing matches", async () => {
    const user = userEvent.setup();
    render(<FilterableTable />);

    await user.type(screen.getByLabelText("Search rows"), "zzz");
    expect(bodyRows()).toHaveLength(0);
  });

  it("restores all rows when the clear button is pressed", async () => {
    const user = userEvent.setup();
    const onSearch = vi.fn();
    render(<FilterableTable onSearch={onSearch} />);
    const input = screen.getByLabelText("Search rows");

    await user.type(input, "carol");
    expect(bodyRows()).toHaveLength(1);

    await user.click(screen.getByRole("button", { name: "Clear search" }));
    expect(input).toHaveValue("");
    expect(onSearch).toHaveBeenLastCalledWith("");
    expect(bodyRows()).toHaveLength(ROWS.length);
    expect(screen.queryByRole("button", { name: "Clear search" })).not.toBeInTheDocument();
  });

  it("restores all rows when the input is cleared manually", async () => {
    const user = userEvent.setup();
    render(<FilterableTable />);
    const input = screen.getByLabelText("Search rows");

    await user.type(input, "bob");
    expect(bodyRows()).toHaveLength(1);

    await user.clear(input);
    expect(bodyRows()).toHaveLength(ROWS.length);
  });

  it("resets the search on Escape", async () => {
    const user = userEvent.setup();
    render(<FilterableTable />);
    const input = screen.getByLabelText("Search rows");

    await user.type(input, "bob");
    await user.keyboard("{Escape}");
    expect(input).toHaveValue("");
    expect(bodyRows()).toHaveLength(ROWS.length);
  });
});
