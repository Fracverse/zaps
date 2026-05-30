import * as LocalAuthentication from 'expo-local-authentication';
import * as SecureStore from 'expo-secure-store';
import { Platform } from 'react-native';

// Types 

export type BiometricType = 'face-id' | 'touch-id' | 'fingerprint' | 'iris' | 'none';

export interface BiometricStatus {
  available: boolean;
  enrolled: boolean;
  types: BiometricType[];
  hasChanged: boolean; // Enrollment changed since last check
}

export interface BiometricPromptOptions {
  title?: string;
  subtitle?: string;
  description?: string;
  cancelLabel?: string;
  fallbackLabel?: string;
  disableDeviceFallback?: boolean;
}

export interface BiometricAuthResult {
  success: boolean;
  error?: string;
  warning?: string;
}

//  Constants 

const BIOMETRIC_PREFERENCE_KEY = 'biometric_auth_enabled';
const BIOMETRIC_ENROLLED_HASH_KEY = 'biometric_enrolled_hash';
const BIOMETRIC_PROMPT_CONTEXT_KEY = 'biometric_prompt_context';
const SECURE_STORAGE_KEY = 'wallet_secure_key';

// Context messages for different operations
export const PROMPT_CONTEXTS = {
  walletAccess: {
    title: 'Unlock Wallet',
    subtitle: 'Access your Stellar wallet',
    description: 'Authenticate to view your balance and transactions',
  },
  paymentConfirm: {
    title: 'Confirm Payment',
    subtitle: 'Authorize transaction',
    description: 'Authenticate to sign and send this payment',
  },
  privateKeyView: {
    title: 'View Private Key',
    subtitle: 'High security access',
    description: 'Authenticate to reveal your private key. Never share this.',
  },
  seedPhraseView: {
    title: 'View Recovery Phrase',
    subtitle: 'High security access',
    description: 'Authenticate to reveal your 12-word recovery phrase. Keep it secret.',
  },
  securitySettings: {
    title: 'Security Settings',
    subtitle: 'Change authentication',
    description: 'Authenticate to modify your security preferences',
  },
  sendPayment: {
    title: 'Send Payment',
    subtitle: 'Authorize transfer',
    description: 'Authenticate to confirm this payment',
  },
} as const;

// Core Functions 

/**
 * Check if biometric authentication is available on this device
 */
export async function checkBiometricStatus(): Promise<BiometricStatus> {
  const hasHardware = await LocalAuthentication.hasHardwareAsync();
  const isEnrolled = await LocalAuthentication.isEnrolledAsync();
  const supportedTypes = await LocalAuthentication.supportedAuthenticationTypesAsync();

  const types: BiometricType[] = supportedTypes.map(type => {
    switch (type) {
      case LocalAuthentication.AuthenticationType.FACIAL_RECOGNITION:
        return 'face-id';
      case LocalAuthentication.AuthenticationType.FINGERPRINT:
        return 'fingerprint';
      case LocalAuthentication.AuthenticationType.IRIS:
        return 'iris';
      default:
        return 'none';
    }
  });

  // Detect enrollment changes
  const currentHash = await getEnrollmentHash();
  const storedHash = await SecureStore.getItemAsync(BIOMETRIC_ENROLLED_HASH_KEY);
  const hasChanged = storedHash !== null && storedHash !== currentHash;

  return {
    available: hasHardware,
    enrolled: isEnrolled,
    types: types.filter(t => t !== 'none'),
    hasChanged,
  };
}

/**
 * Prompt user for biometric authentication
 */
export async function promptBiometric(
  options: BiometricPromptOptions = {}
): Promise<BiometricAuthResult> {
  const status = await checkBiometricStatus();

  if (!status.available) {
    return {
      success: false,
      error: 'Biometric authentication not available on this device',
    };
  }

  if (!status.enrolled) {
    return {
      success: false,
      error: 'No biometric credentials enrolled. Please set up Face ID/Touch ID/Fingerprint in device settings.',
    };
  }

  // Detect enrollment change and warn
  if (status.hasChanged) {
    // Still allow auth but warn
    console.warn('Biometric enrollment has changed since last authentication');
  }

  try {
    const result = await LocalAuthentication.authenticateAsync({
      promptMessage: options.title || 'Authenticate',
      subTitle: options.subtitle,
      description: options.description,
      cancelLabel: options.cancelLabel || 'Cancel',
      fallbackLabel: options.fallbackLabel || 'Use PIN',
      disableDeviceFallback: options.disableDeviceFallback ?? false,
      requireConfirmation: Platform.OS === 'android', // Android requires confirmation
    });

    if (result.success) {
      // Update stored enrollment hash
      const newHash = await getEnrollmentHash();
      await SecureStore.setItemAsync(BIOMETRIC_ENROLLED_HASH_KEY, newHash);
    }

    return {
      success: result.success,
      error: result.success ? undefined : result.error || 'Authentication cancelled',
      warning: status.hasChanged ? 'Biometric enrollment changed since last use' : undefined,
    };
  } catch (error) {
    return {
      success: false,
      error: error instanceof Error ? error.message : 'Authentication failed',
    };
  }
}

/**
 * Prompt with specific context for an operation
 */
export async function promptForOperation(
  operation: keyof typeof PROMPT_CONTEXTS
): Promise<BiometricAuthResult> {
  const context = PROMPT_CONTEXTS[operation];
  return promptBiometric(context);
}

//  Preference Management 

/**
 * Check if user has enabled biometric auth
 */
export async function isBiometricEnabled(): Promise<boolean> {
  const value = await SecureStore.getItemAsync(BIOMETRIC_PREFERENCE_KEY);
  return value === 'true';
}

/**
 * Enable/disable biometric authentication
 */
export async function setBiometricEnabled(enabled: boolean): Promise<void> {
  await SecureStore.setItemAsync(BIOMETRIC_PREFERENCE_KEY, enabled ? 'true' : 'false');
  
  if (enabled) {
    // Store initial enrollment hash
    const hash = await getEnrollmentHash();
    await SecureStore.setItemAsync(BIOMETRIC_ENROLLED_HASH_KEY, hash);
  } else {
    // Clear enrollment hash
    await SecureStore.deleteItemAsync(BIOMETRIC_ENROLLED_HASH_KEY);
  }
}

//  Secure Key Storage with Biometric Protection 

/**
 * Store wallet key with biometric protection
 */
export async function storeSecureKey(key: string): Promise<void> {
  await SecureStore.setItemAsync(SECURE_STORAGE_KEY, key, {
    requireAuthentication: true,
    keychainService: 'zaps-wallet',
    keychainAccessible: SecureStore.WHEN_UNLOCKED_THIS_DEVICE_ONLY,
  });
}

/**
 * Retrieve wallet key (triggers biometric if required)
 */
export async function retrieveSecureKey(): Promise<string | null> {
  return SecureStore.getItemAsync(SECURE_STORAGE_KEY);
}

/**
 * Delete stored secure key
 */
export async function deleteSecureKey(): Promise<void> {
  await SecureStore.deleteItemAsync(SECURE_STORAGE_KEY);
}

//  Enrollment Change Detection 

/**
 * Generate a hash of current biometric enrollment state
 */
async function getEnrollmentHash(): Promise<string> {
  const types = await LocalAuthentication.supportedAuthenticationTypesAsync();
  const enrolled = await LocalAuthentication.isEnrolledAsync();
  
  // Simple hash of enrollment state
  const data = `${enrolled}-${types.sort().join(',')}`;
  return btoa(data); // Base64 encode
}

//  Fallback PIN/Password 

const PIN_STORAGE_KEY = 'fallback_pin_hash';
const PIN_ATTEMPTS_KEY = 'pin_attempts_remaining';
const MAX_PIN_ATTEMPTS = 5;

/**
 * Check if PIN fallback is set up
 */
export async function hasPINFallback(): Promise<boolean> {
  const pinHash = await SecureStore.getItemAsync(PIN_STORAGE_KEY);
  return pinHash !== null;
}

/**
 * Set up PIN fallback
 */
export async function setupPIN(pin: string): Promise<void> {
  if (pin.length < 4) {
    throw new Error('PIN must be at least 4 digits');
  }
  
  // Simple hash (in production, use bcrypt or Argon2)
  const hash = await hashPIN(pin);
  await SecureStore.setItemAsync(PIN_STORAGE_KEY, hash);
  await SecureStore.setItemAsync(PIN_ATTEMPTS_KEY, String(MAX_PIN_ATTEMPTS));
}

/**
 * Verify PIN fallback
 */
export async function verifyPIN(pin: string): Promise<boolean> {
  const attemptsStr = await SecureStore.getItemAsync(PIN_ATTEMPTS_KEY);
  let attempts = attemptsStr ? parseInt(attemptsStr, 10) : MAX_PIN_ATTEMPTS;
  
  if (attempts <= 0) {
    throw new Error('Too many failed attempts. Please re-authenticate with device password.');
  }
  
  const storedHash = await SecureStore.getItemAsync(PIN_STORAGE_KEY);
  if (!storedHash) return false;
  
  const inputHash = await hashPIN(pin);
  const valid = inputHash === storedHash;
  
  if (!valid) {
    attempts -= 1;
    await SecureStore.setItemAsync(PIN_ATTEMPTS_KEY, String(attempts));
  } else {
    await SecureStore.setItemAsync(PIN_ATTEMPTS_KEY, String(MAX_PIN_ATTEMPTS));
  }
  
  return valid;
}

/**
 * Reset PIN attempts (after successful biometric auth)
 */
export async function resetPINAttempts(): Promise<void> {
  await SecureStore.setItemAsync(PIN_ATTEMPTS_KEY, String(MAX_PIN_ATTEMPTS));
}

// Simple PIN hash (replace with proper hashing in production)
async function hashPIN(pin: string): Promise<string> {
  const encoder = new TextEncoder();
  const data = encoder.encode(pin + 'zaps-salt-v1');
  const hashBuffer = await crypto.subtle.digest('SHA-256', data);
  const hashArray = Array.from(new Uint8Array(hashBuffer));
  return hashArray.map(b => b.toString(16).padStart(2, '0')).join('');
}