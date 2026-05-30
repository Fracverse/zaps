import { useState, useEffect, useCallback } from 'react';
import {
  checkBiometricStatus,
  promptBiometric,
  promptForOperation,
  isBiometricEnabled,
  setBiometricEnabled,
  hasPINFallback,
  verifyPIN,
  BiometricStatus,
  BiometricAuthResult,
  PROMPT_CONTEXTS,
} from '../services/biometricAuth';

interface UseBiometricReturn {
  status: BiometricStatus | null;
  enabled: boolean;
  hasPIN: boolean;
  isLoading: boolean;
  error: string | null;
  
  // Actions
  authenticate: (operation?: keyof typeof PROMPT_CONTEXTS) => Promise<boolean>;
  authenticateWithPIN: (pin: string) => Promise<boolean>;
  enableBiometric: () => Promise<void>;
  disableBiometric: () => Promise<void>;
  setupPIN: (pin: string) => Promise<void>;
  refreshStatus: () => Promise<void>;
}

export function useBiometric(): UseBiometricReturn {
  const [status, setStatus] = useState<BiometricStatus | null>(null);
  const [enabled, setEnabled] = useState(false);
  const [hasPIN, setHasPIN] = useState(false);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const refreshStatus = useCallback(async () => {
    try {
      setIsLoading(true);
      setError(null);
      
      const [bioStatus, bioEnabled, pinSet] = await Promise.all([
        checkBiometricStatus(),
        isBiometricEnabled(),
        hasPINFallback(),
      ]);
      
      setStatus(bioStatus);
      setEnabled(bioEnabled);
      setHasPIN(pinSet);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to check biometric status');
    } finally {
      setIsLoading(false);
    }
  }, []);

  useEffect(() => {
    refreshStatus();
  }, [refreshStatus]);

  const authenticate = useCallback(async (
    operation?: keyof typeof PROMPT_CONTEXTS
  ): Promise<boolean> => {
    if (!enabled) return true; // Biometric not required
    
    setError(null);
    
    const result = operation 
      ? await promptForOperation(operation)
      : await promptBiometric();
    
    if (!result.success && result.error) {
      setError(result.error);
    }
    
    if (result.warning) {
      console.warn(result.warning);
    }
    
    return result.success;
  }, [enabled]);

  const authenticateWithPIN = useCallback(async (pin: string): Promise<boolean> => {
    try {
      setError(null);
      const valid = await verifyPIN(pin);
      if (!valid) {
        setError('Invalid PIN');
      }
      return valid;
    } catch (err) {
      setError(err instanceof Error ? err.message : 'PIN verification failed');
      return false;
    }
  }, []);

  const enableBiometric = useCallback(async () => {
    try {
      setIsLoading(true);
      
      // First verify biometric works
      const result = await promptBiometric({
        title: 'Enable Biometric Authentication',
        subtitle: 'Confirm your identity',
        description: 'You will use this to secure your wallet and payments',
      });
      
      if (!result.success) {
        throw new Error(result.error || 'Biometric verification failed');
      }
      
      await setBiometricEnabled(true);
      setEnabled(true);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to enable biometric');
      throw err;
    } finally {
      setIsLoading(false);
    }
  }, []);

  const disableBiometric = useCallback(async () => {
    try {
      setIsLoading(true);
      await setBiometricEnabled(false);
      setEnabled(false);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to disable biometric');
      throw err;
    } finally {
      setIsLoading(false);
    }
  }, []);

  const setupPIN = useCallback(async (pin: string) => {
    try {
      setIsLoading(true);
      const { setupPIN: setup } = await import('../services/biometricAuth');
      await setup(pin);
      setHasPIN(true);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to set PIN');
      throw err;
    } finally {
      setIsLoading(false);
    }
  }, []);

  return {
    status,
    enabled,
    hasPIN,
    isLoading,
    error,
    authenticate,
    authenticateWithPIN,
    enableBiometric,
    disableBiometric,
    setupPIN,
    refreshStatus,
  };
}