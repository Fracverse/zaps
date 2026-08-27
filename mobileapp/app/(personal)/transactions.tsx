/**
 * transactions.tsx
 *
 * Full transaction list screen that supports section grouping,
 * distinguishing SDP "Mass Payout" disbursements from standard transfers.
 *
 * Renders the same HistoryScreen used in the tab navigator, available as
 * a standalone route for deep-link and programmatic navigation use-cases.
 *
 * Issue: #576 — Design transaction list section grouping
 *          Received Mass Payouts distinct from standard transfers.
 */
export { default } from "./history";
