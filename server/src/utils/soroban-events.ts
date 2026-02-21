import { scValToNative } from '@stellar/stellar-sdk';

/**
 * Extract topic strings from a Soroban event topic array.
 * Handles both raw xdr.ScVal[] and pre-parsed string[].
 */
export function extractTopicStrings(topic: unknown[] | undefined): string[] {
    if (!Array.isArray(topic) || topic.length === 0) return [];
    return topic.map((t) => {
        if (typeof t === 'string') return t;
        try {
            const native = scValToNative(t as any);
            return typeof native === 'string' ? native : String(native);
        } catch {
            return String(t);
        }
    });
}
