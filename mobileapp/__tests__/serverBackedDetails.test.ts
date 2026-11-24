import { apiFetch } from "../src/services/api";
import { fetchBatchPayoutDetail } from "../src/services/batchPayoutService";

jest.mock("../src/services/api", () => ({
  apiFetch: jest.fn(),
}));

const apiFetchMock = apiFetch as jest.Mock;

function batchPayload(id: string) {
  return {
    batch: {
      id,
      status: "PROCESSING",
      currency: "USDC",
      total_recipients: 1,
      total_amount: 2_500_000,
      succeeded_count: 0,
      failed_count: 0,
      created_at: "2026-07-20T10:00:00Z",
    },
    recipients: [
      {
        id: `recipient-${id}`,
        user_id: null,
        destination_address: `G${id}`,
        amount: 2_500_000,
        status: "PENDING",
        tx_hash: null,
        attempt_count: 0,
        created_at: "2026-07-20T10:00:00Z",
      },
    ],
  };
}

describe("server-backed batch detail", () => {
  beforeEach(() => jest.clearAllMocks());

  it("requests and returns data for each route ID independently", async () => {
    apiFetchMock.mockImplementation((url: string) => {
      const id = url.split("/").pop()!;
      return Promise.resolve({
        ok: true,
        status: 200,
        json: async () => batchPayload(id),
      });
    });

    const first = await fetchBatchPayoutDetail("batch-a");
    const second = await fetchBatchPayoutDetail("batch-b");

    expect(apiFetchMock.mock.calls.map(([url]) => url)).toEqual([
      expect.stringContaining("/api/payouts/batch/batch-a"),
      expect.stringContaining("/api/payouts/batch/batch-b"),
    ]);
    expect(first?.summary.id).toBe("batch-a");
    expect(second?.summary.id).toBe("batch-b");
    expect(second?.items[0].recipientAddress).toBe("Gbatch-b");
    expect(second?.items).toHaveLength(1);
  });

  it("treats an unknown route ID as not found instead of supplying rows", async () => {
    apiFetchMock.mockResolvedValue({ ok: false, status: 404 });

    await expect(fetchBatchPayoutDetail("unknown")).resolves.toBeNull();
    expect(apiFetchMock).toHaveBeenCalledWith(
      expect.stringContaining("/api/payouts/batch/unknown")
    );
  });
});
