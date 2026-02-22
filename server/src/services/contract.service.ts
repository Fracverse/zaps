import { xdr } from '@stellar/stellar-sdk';
import sorobanService from './soroban.service';
import logger from '../utils/logger';

/**
 * Higher-level contract interaction service.
 * Delegates XDR building and sponsorship to SorobanService.
 */
export class ContractService {
    /**
     * Builds and sponsors a Soroban contract invocation.
     * Returns a half-signed XDR (fee payer signed, user signature pending).
     */
    async buildAndSponsor(
        sourceAddress: string,
        contractId: string,
        method: string,
        args: xdr.ScVal[],
    ): Promise<{ xdr: string; feePayerAddress: string }> {
        const unsignedXdr = await sorobanService.buildContractCall(
            sourceAddress,
            contractId,
            method,
            args,
        );

        const sponsored = await sorobanService.sponsorTransaction(unsignedXdr);

        logger.info('Contract call sponsored', {
            contract: contractId,
            method,
            source: sourceAddress,
        });

        return {
            xdr: sponsored.sponsoredXdr,
            feePayerAddress: sponsored.feePayerAddress,
        };
    }
}

export default new ContractService();
