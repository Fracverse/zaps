import { describe, expect, it } from "vitest";
import { render, screen, within } from "@testing-library/react";
import AuditLogTable, { sortLogsDesc } from "./AuditLogTable";
import { AdminAuditLog } from "@/lib/api";

const LOGS: AdminAuditLog[] = [
  {
    id: "a",
    action: "config_change",
    admin_id: "adm-1",
    admin_email: "ops@zaps.dev",
    ip_address: "10.0.0.1",
    changes: { fee_coefficient: { from: 1, to: 2 } },
    timestamp: "2026-10-01T10:00:00Z",
  },
  {
    id: "b",
    action: "emergency_pause",
    admin_id: "adm-2",
    ip_address: "192.168.1.5",
    timestamp: "2026-10-03T08:30:00Z",
  },
  {
    id: "c",
    action: "emergency_unpause",
    admin_id: "adm-2",
    ip_address: "192.168.1.5",
    timestamp: "2026-10-02T12:00:00Z",
  },
];

describe("sortLogsDesc", () => {
  it("orders entries most recent first without mutating input", () => {
    const input = [...LOGS];
    expect(sortLogsDesc(input).map((l) => l.id)).toEqual(["b", "c", "a"]);
    expect(input.map((l) => l.id)).toEqual(["a", "b", "c"]);
  });
});

describe("AuditLogTable", () => {
  it("renders skeleton rows while the first load is pending", () => {
    render(<AuditLogTable logs={null} loading />);
    expect(screen.getAllByTestId("audit-log-skeleton").length).toBeGreaterThan(0);
    expect(screen.queryByText(/No administrative activity/)).not.toBeInTheDocument();
  });

  it("renders an empty state when there are no logs", () => {
    render(<AuditLogTable logs={[]} loading={false} />);
    expect(screen.getByText("No administrative activity recorded yet")).toBeInTheDocument();
    expect(screen.queryAllByTestId("audit-log-row")).toHaveLength(0);
  });

  it("renders an empty state when loading failed with no data", () => {
    render(<AuditLogTable logs={null} loading={false} error="500 Internal Server Error" />);
    expect(screen.getByText(/500 Internal Server Error/)).toBeInTheDocument();
    expect(screen.getByText("No administrative activity recorded yet")).toBeInTheDocument();
  });

  it("renders rows sorted descending by timestamp with all details", () => {
    render(<AuditLogTable logs={LOGS} loading={false} />);
    const rows = screen.getAllByTestId("audit-log-row");
    expect(rows).toHaveLength(3);

    expect(within(rows[0]).getByText("emergency pause")).toBeInTheDocument();
    expect(within(rows[1]).getByText("emergency unpause")).toBeInTheDocument();
    expect(within(rows[2]).getByText("config change")).toBeInTheDocument();

    expect(within(rows[2]).getByText("ops@zaps.dev")).toBeInTheDocument();
    expect(within(rows[2]).getByText("10.0.0.1")).toBeInTheDocument();
    expect(within(rows[2]).getByText("fee_coefficient: 1 → 2")).toBeInTheDocument();
    expect(within(rows[0]).getByText("adm-2")).toBeInTheDocument();
  });

  it("keeps showing existing rows during a background refresh", () => {
    render(<AuditLogTable logs={LOGS} loading />);
    expect(screen.queryAllByTestId("audit-log-skeleton")).toHaveLength(0);
    expect(screen.getAllByTestId("audit-log-row")).toHaveLength(3);
  });
});
