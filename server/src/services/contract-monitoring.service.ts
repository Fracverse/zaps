import config from '../config';
import logger from '../utils/logger';
import redisClient from '../utils/redis';
import sorobanService from './soroban.service';
import { extractTopicStrings } from '../utils/soroban-events';

const CURSOR_KEY = 'contract:monitor:events:cursor';
const READ_SOURCE = 'GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF';
const POLL_INTERVAL_MS = 30_000;
const HEALTH_INTERVAL_MS = 60_000;

export type AlertSeverity = 'info' | 'warning' | 'critical';

export interface ContractHealthStatus {
    name: string;
    contractId: string;
    configured: boolean;
    reachable: boolean;
    paused?: boolean;
    lastChecked: string;
    error?: string;
}

export interface ContractPerformanceMetrics {
    sorobanRpcLatencyMs: number;
    latestLedger: number;
    eventPollLagLedgers: number;
    lastEventPollAt: string | null;
    eventsTotal: {
        initiated: number;
        settled: number;
        failed: number;
        other: number;
    };
    simulationCount: number;
    simulationErrorCount: number;
    avgSimulationMs: number;
    uptimeSeconds: number;
}

export interface ContractAlert {
    id: string;
    severity: AlertSeverity;
    title: string;
    message: string;
    metric: string;
    value: number;
    threshold: number;
    timestamp: string;
}

interface MonitoredContract {
    name: string;
    contractId: string;
    healthMethod?: string;
    supportsPaused: boolean;
}

class ContractMonitoringService {
    private running = false;
    private pollTimer: ReturnType<typeof setTimeout> | null = null;
    private healthTimer: ReturnType<typeof setTimeout> | null = null;
    private startedAt = Date.now();
    private lastLedgerCursor = 0;
    private latestLedger = 0;
    private sorobanRpcLatencyMs = 0;
    private lastEventPollAt: Date | null = null;
    private simulationCount = 0;
    private simulationErrorCount = 0;
    private simulationDurationsMs: number[] = [];
    private eventsTotal = { initiated: 0, settled: 0, failed: 0, other: 0 };
    private contractHealth: ContractHealthStatus[] = [];
    private activeAlerts: ContractAlert[] = [];
    private lastWebhookAlertIds = new Set<string>();

    private getMonitoredContracts(): MonitoredContract[] {
        const contracts: MonitoredContract[] = [];
        if (config.stellar.paymentRouterContract) {
            contracts.push({
                name: 'payment_router',
                contractId: config.stellar.paymentRouterContract,
                healthMethod: 'is_paused',
                supportsPaused: true,
            });
        }
        if (config.stellar.registryContract) {
            contracts.push({
                name: 'registry',
                contractId: config.stellar.registryContract,
                healthMethod: 'get_admin',
                supportsPaused: false,
            });
        }
        return contracts;
    }

    async start(): Promise<void> {
        if (this.running) return;
        this.running = true;
        this.startedAt = Date.now();
        logger.info('[ContractMonitor] Starting contract monitoring');

        try {
            const stored = await redisClient.get(CURSOR_KEY);
            if (stored) {
                this.lastLedgerCursor = parseInt(stored, 10);
            } else {
                this.lastLedgerCursor = await sorobanService.getLatestLedger();
                await redisClient.set(CURSOR_KEY, String(this.lastLedgerCursor));
            }
        } catch (err) {
            logger.warn('[ContractMonitor] Could not load event cursor', { err });
        }

        await this.runHealthChecks();
        this.scheduleHealthChecks();
        this.scheduleEventPoll();
    }

    stop(): void {
        this.running = false;
        if (this.pollTimer) clearTimeout(this.pollTimer);
        if (this.healthTimer) clearTimeout(this.healthTimer);
        this.pollTimer = null;
        this.healthTimer = null;
        logger.info('[ContractMonitor] Stopped');
    }

    private scheduleHealthChecks(): void {
        const run = async () => {
            if (!this.running) return;
            try {
                await this.runHealthChecks();
            } catch (err) {
                logger.error('[ContractMonitor] Health check failed', { err });
            }
            if (this.running) {
                this.healthTimer = setTimeout(run, HEALTH_INTERVAL_MS);
            }
        };
        this.healthTimer = setTimeout(run, HEALTH_INTERVAL_MS);
    }

    private scheduleEventPoll(): void {
        const run = async () => {
            if (!this.running) return;
            try {
                await this.pollContractEvents();
            } catch (err) {
                logger.error('[ContractMonitor] Event poll failed', { err });
                this.recordRpcFailure();
            }
            if (this.running) {
                this.pollTimer = setTimeout(run, POLL_INTERVAL_MS);
            }
        };
        this.pollTimer = setTimeout(run, POLL_INTERVAL_MS);
    }

    async runHealthChecks(): Promise<ContractHealthStatus[]> {
        const rpcStart = Date.now();
        try {
            this.latestLedger = await sorobanService.getLatestLedger();
            this.sorobanRpcLatencyMs = Date.now() - rpcStart;
        } catch (err) {
            this.sorobanRpcLatencyMs = Date.now() - rpcStart;
            this.latestLedger = 0;
            logger.error('[ContractMonitor] Soroban RPC unreachable', { err });
        }

        const results: ContractHealthStatus[] = [];

        for (const contract of this.getMonitoredContracts()) {
            const status: ContractHealthStatus = {
                name: contract.name,
                contractId: contract.contractId,
                configured: true,
                reachable: false,
                lastChecked: new Date().toISOString(),
            };

            if (!contract.healthMethod) {
                results.push(status);
                continue;
            }

            const simStart = Date.now();
            try {
                const value = await sorobanService.simulateContractRead(
                    contract.contractId,
                    contract.healthMethod,
                );
                this.simulationCount++;
                this.simulationDurationsMs.push(Date.now() - simStart);
                if (this.simulationDurationsMs.length > 100) {
                    this.simulationDurationsMs.shift();
                }
                status.reachable = true;
                if (contract.supportsPaused && contract.healthMethod === 'is_paused') {
                    status.paused = Boolean(value);
                }
            } catch (err) {
                this.simulationErrorCount++;
                status.error = err instanceof Error ? err.message : String(err);
                logger.warn('[ContractMonitor] Contract health read failed', {
                    contract: contract.name,
                    error: status.error,
                });
            }

            results.push(status);
        }

        this.contractHealth = results;
        this.evaluateAlerts();
        return results;
    }

    private async pollContractEvents(): Promise<void> {
        const rpcStart = Date.now();
        this.latestLedger = await sorobanService.getLatestLedger();
        this.sorobanRpcLatencyMs = Date.now() - rpcStart;

        const contractIds = this.getMonitoredContracts().map((c) => c.contractId);
        const startLedger = this.lastLedgerCursor || Math.max(1, this.latestLedger - 10);

        const filters =
            contractIds.length > 0
                ? [{ type: 'contract' as const, contractIds }]
                : [{ type: 'contract' as const }];

        const response = await sorobanService.getEvents(startLedger, filters);
        const rawEvents =
            (response as { events?: Array<{ topic?: unknown[]; ledger?: number | string }> })
                .events ?? [];

        let maxLedger = startLedger;

        for (const event of rawEvents) {
            const topics = extractTopicStrings(event.topic);
            const ledgerNum = parseInt(String(event.ledger ?? 0), 10);
            if (ledgerNum > maxLedger) maxLedger = ledgerNum;

            if (topics[0] === 'payment') {
                const kind = topics[1];
                if (kind === 'PaymentInitiated') this.eventsTotal.initiated++;
                else if (kind === 'PaymentSettled') this.eventsTotal.settled++;
                else if (kind === 'PaymentFailed') this.eventsTotal.failed++;
                else this.eventsTotal.other++;
            } else {
                this.eventsTotal.other++;
            }
        }

        if (rawEvents.length > 0) {
            this.lastLedgerCursor = maxLedger + 1;
            await redisClient.set(CURSOR_KEY, String(this.lastLedgerCursor));
        }

        this.lastEventPollAt = new Date();
        this.evaluateAlerts();
    }

    private recordRpcFailure(): void {
        this.sorobanRpcLatencyMs = -1;
        this.latestLedger = 0;
        this.evaluateAlerts();
    }

    private getEventPollLag(): number {
        if (!this.latestLedger || !this.lastLedgerCursor) return 0;
        return Math.max(0, this.latestLedger - this.lastLedgerCursor);
    }

    private getFailureRate(): number {
        const { settled, failed } = this.eventsTotal;
        const total = settled + failed;
        if (total === 0) return 0;
        return (failed / total) * 100;
    }

    private avgSimulationMs(): number {
        if (this.simulationDurationsMs.length === 0) return 0;
        const sum = this.simulationDurationsMs.reduce((a, b) => a + b, 0);
        return sum / this.simulationDurationsMs.length;
    }

    evaluateAlerts(): ContractAlert[] {
        const alerts: ContractAlert[] = [];
        const now = new Date().toISOString();

        if (this.sorobanRpcLatencyMs < 0 || this.latestLedger === 0) {
            alerts.push({
                id: 'soroban_rpc_down',
                severity: 'critical',
                title: 'Soroban RPC Unreachable',
                message: 'Cannot reach the Soroban RPC endpoint for contract operations.',
                metric: 'soroban_rpc_up',
                value: 0,
                threshold: 1,
                timestamp: now,
            });
        } else if (this.sorobanRpcLatencyMs > 5000) {
            alerts.push({
                id: 'soroban_rpc_slow',
                severity: 'critical',
                title: 'Soroban RPC High Latency',
                message: `RPC latency is ${this.sorobanRpcLatencyMs}ms (threshold: 5000ms).`,
                metric: 'soroban_rpc_latency_ms',
                value: this.sorobanRpcLatencyMs,
                threshold: 5000,
                timestamp: now,
            });
        } else if (this.sorobanRpcLatencyMs > 2000) {
            alerts.push({
                id: 'soroban_rpc_degraded',
                severity: 'warning',
                title: 'Soroban RPC Elevated Latency',
                message: `RPC latency is ${this.sorobanRpcLatencyMs}ms (threshold: 2000ms).`,
                metric: 'soroban_rpc_latency_ms',
                value: this.sorobanRpcLatencyMs,
                threshold: 2000,
                timestamp: now,
            });
        }

        const lag = this.getEventPollLag();
        if (lag > 200) {
            alerts.push({
                id: 'event_poll_lag_critical',
                severity: 'critical',
                title: 'Contract Event Indexer Stalled',
                message: `Event poll is ${lag} ledgers behind the network.`,
                metric: 'soroban_event_poll_lag_ledgers',
                value: lag,
                threshold: 200,
                timestamp: now,
            });
        } else if (lag > 50) {
            alerts.push({
                id: 'event_poll_lag_warning',
                severity: 'warning',
                title: 'Contract Event Indexer Lagging',
                message: `Event poll is ${lag} ledgers behind the network.`,
                metric: 'soroban_event_poll_lag_ledgers',
                value: lag,
                threshold: 50,
                timestamp: now,
            });
        }

        for (const contract of this.contractHealth) {
            if (contract.configured && !contract.reachable) {
                alerts.push({
                    id: `contract_unreachable_${contract.name}`,
                    severity: 'critical',
                    title: `Contract Unreachable: ${contract.name}`,
                    message: contract.error ?? 'Health check simulation failed.',
                    metric: 'contract_reachable',
                    value: 0,
                    threshold: 1,
                    timestamp: now,
                });
            }
            if (contract.paused) {
                alerts.push({
                    id: `contract_paused_${contract.name}`,
                    severity: 'critical',
                    title: `Contract Paused: ${contract.name}`,
                    message: 'Payments are blocked while the contract is paused.',
                    metric: 'contract_paused',
                    value: 1,
                    threshold: 0,
                    timestamp: now,
                });
            }
        }

        const failureRate = this.getFailureRate();
        if (failureRate > 25) {
            alerts.push({
                id: 'payment_failure_rate_critical',
                severity: 'critical',
                title: 'High On-Chain Payment Failure Rate',
                message: `${failureRate.toFixed(1)}% of tracked payment events failed.`,
                metric: 'contract_payment_failure_rate_percent',
                value: failureRate,
                threshold: 25,
                timestamp: now,
            });
        } else if (failureRate > 10) {
            alerts.push({
                id: 'payment_failure_rate_warning',
                severity: 'warning',
                title: 'Elevated On-Chain Payment Failure Rate',
                message: `${failureRate.toFixed(1)}% of tracked payment events failed.`,
                metric: 'contract_payment_failure_rate_percent',
                value: failureRate,
                threshold: 10,
                timestamp: now,
            });
        }

        this.activeAlerts = alerts;
        void this.dispatchAlertWebhooks(alerts);
        return alerts;
    }

    private async dispatchAlertWebhooks(alerts: ContractAlert[]): Promise<void> {
        const webhookUrl = process.env.ALERT_WEBHOOK_URL;
        if (!webhookUrl) return;

        const critical = alerts.filter((a) => a.severity === 'critical');
        for (const alert of critical) {
            if (this.lastWebhookAlertIds.has(alert.id)) continue;
            this.lastWebhookAlertIds.add(alert.id);
            try {
                await fetch(webhookUrl, {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({
                        source: 'zaps-contract-monitor',
                        alert,
                    }),
                });
            } catch (err) {
                logger.error('[ContractMonitor] Failed to send alert webhook', {
                    alertId: alert.id,
                    err,
                });
            }
        }

        const activeIds = new Set(alerts.map((a) => a.id));
        for (const id of this.lastWebhookAlertIds) {
            if (!activeIds.has(id)) this.lastWebhookAlertIds.delete(id);
        }
    }

    getHealth(): {
        status: string;
        contracts: ContractHealthStatus[];
        sorobanRpc: string;
        latestLedger: number;
    } {
        const rpcOk = this.latestLedger > 0 && this.sorobanRpcLatencyMs >= 0;
        const contractsOk = this.contractHealth.every(
            (c) => !c.configured || (c.reachable && !c.paused),
        );
        const status = rpcOk && contractsOk ? 'healthy' : 'unhealthy';

        return {
            status,
            contracts: this.contractHealth,
            sorobanRpc: rpcOk ? 'connected' : 'error',
            latestLedger: this.latestLedger,
        };
    }

    getMetrics(): ContractPerformanceMetrics {
        return {
            sorobanRpcLatencyMs: Math.max(0, this.sorobanRpcLatencyMs),
            latestLedger: this.latestLedger,
            eventPollLagLedgers: this.getEventPollLag(),
            lastEventPollAt: this.lastEventPollAt?.toISOString() ?? null,
            eventsTotal: { ...this.eventsTotal },
            simulationCount: this.simulationCount,
            simulationErrorCount: this.simulationErrorCount,
            avgSimulationMs: this.avgSimulationMs(),
            uptimeSeconds: Math.floor((Date.now() - this.startedAt) / 1000),
        };
    }

    getAlerts(): ContractAlert[] {
        return this.activeAlerts;
    }

    /** Prometheus text exposition for contract metrics (scrape target). */
    getPrometheusMetrics(): string {
        const lines: string[] = [
            '# HELP soroban_rpc_up Soroban RPC reachability (1=up, 0=down)',
            '# TYPE soroban_rpc_up gauge',
            `soroban_rpc_up ${this.latestLedger > 0 && this.sorobanRpcLatencyMs >= 0 ? 1 : 0}`,
            '# HELP soroban_rpc_latency_ms Soroban RPC round-trip latency',
            '# TYPE soroban_rpc_latency_ms gauge',
            `soroban_rpc_latency_ms ${Math.max(0, this.sorobanRpcLatencyMs)}`,
            '# HELP soroban_event_poll_lag_ledgers Ledgers behind latest for event indexer',
            '# TYPE soroban_event_poll_lag_ledgers gauge',
            `soroban_event_poll_lag_ledgers ${this.getEventPollLag()}`,
            '# HELP soroban_events_total Soroban contract events by type',
            '# TYPE soroban_events_total counter',
            `soroban_events_total{event_type="initiated"} ${this.eventsTotal.initiated}`,
            `soroban_events_total{event_type="settled"} ${this.eventsTotal.settled}`,
            `soroban_events_total{event_type="failed"} ${this.eventsTotal.failed}`,
            `soroban_events_total{event_type="other"} ${this.eventsTotal.other}`,
            '# HELP contract_paused Contract pause state (1=paused)',
            '# TYPE contract_paused gauge',
        ];

        for (const c of this.contractHealth) {
            lines.push(
                `contract_paused{contract="${c.name}"} ${c.paused ? 1 : 0}`,
            );
            lines.push(
                `contract_reachable{contract="${c.name}"} ${c.reachable ? 1 : 0}`,
            );
        }

        lines.push(
            '# HELP contract_payment_failure_rate_percent On-chain payment failure rate',
            '# TYPE contract_payment_failure_rate_percent gauge',
            `contract_payment_failure_rate_percent ${this.getFailureRate()}`,
        );

        return lines.join('\n') + '\n';
    }
}

export default new ContractMonitoringService();
