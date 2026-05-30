import React, { useState, useEffect } from 'react';
import { View, Modal, Text, TextInput, TouchableOpacity, StyleSheet, ActivityIndicator } from 'react-native';
import { useBiometric } from '../hooks/useBiometric';
import { PROMPT_CONTEXTS } from '../services/biometricAuth';

interface BiometricGuardProps {
  operation: keyof typeof PROMPT_CONTEXTS;
  children: React.ReactNode;
  onAuthSuccess?: () => void;
  onAuthFailure?: () => void;
  fallbackToPIN?: boolean;
}

export default function BiometricGuard({
  operation,
  children,
  onAuthSuccess,
  onAuthFailure,
  fallbackToPIN = true,
}: BiometricGuardProps) {
  const biometric = useBiometric();
  const [showPIN, setShowPIN] = useState(false);
  const [pin, setPIN] = useState('');
  const [isAuthenticating, setIsAuthenticating] = useState(false);
  const [authComplete, setAuthComplete] = useState(false);

  useEffect(() => {
    if (biometric.enabled && !authComplete && !isAuthenticating) {
      performAuth();
    }
  }, [biometric.enabled, authComplete]);

  const performAuth = async () => {
    setIsAuthenticating(true);
    
    try {
      const success = await biometric.authenticate(operation);
      
      if (success) {
        setAuthComplete(true);
        onAuthSuccess?.();
      } else if (fallbackToPIN && biometric.hasPIN) {
        setShowPIN(true);
      } else {
        onAuthFailure?.();
      }
    } finally {
      setIsAuthenticating(false);
    }
  };

  const handlePINSubmit = async () => {
    if (pin.length < 4) return;
    
    const valid = await biometric.authenticateWithPIN(pin);
    
    if (valid) {
      setShowPIN(false);
      setAuthComplete(true);
      setPIN('');
      onAuthSuccess?.();
    }
  };

  const handleCancel = () => {
    setShowPIN(false);
    setPIN('');
    onAuthFailure?.();
  };

  // If biometric not enabled, render children directly
  if (!biometric.enabled) {
    return <>{children}</>;
  }

  // Show loading while checking status
  if (biometric.isLoading) {
    return (
      <View style={styles.centered}>
        <ActivityIndicator size="large" />
      </View>
    );
  }

  // If auth complete, render children
  if (authComplete) {
    return <>{children}</>;
  }

  // Show PIN fallback modal
  return (
    <View style={styles.container}>
      {isAuthenticating && (
        <View style={styles.overlay}>
          <ActivityIndicator size="large" color="#6366f1" />
          <Text style={styles.overlayText}>Authenticating...</Text>
        </View>
      )}

      <Modal
        visible={showPIN}
        transparent
        animationType="slide"
        onRequestClose={handleCancel}
      >
        <View style={styles.modalOverlay}>
          <View style={styles.modalContent}>
            <Text style={styles.modalTitle}>Enter PIN</Text>
            <Text style={styles.modalSubtitle}>
              {PROMPT_CONTEXTS[operation].description}
            </Text>
            
            <TextInput
              style={styles.pinInput}
              value={pin}
              onChangeText={setPIN}
              keyboardType="number-pad"
              maxLength={6}
              secureTextEntry
              placeholder="••••"
              placeholderTextColor="#9ca3af"
            />
            
            {biometric.error && (
              <Text style={styles.errorText}>{biometric.error}</Text>
            )}
            
            <View style={styles.buttonRow}>
              <TouchableOpacity style={styles.cancelButton} onPress={handleCancel}>
                <Text style={styles.cancelButtonText}>Cancel</Text>
              </TouchableOpacity>
              
              <TouchableOpacity 
                style={[styles.submitButton, pin.length < 4 && styles.submitButtonDisabled]} 
                onPress={handlePINSubmit}
                disabled={pin.length < 4}
              >
                <Text style={styles.submitButtonText}>Confirm</Text>
              </TouchableOpacity>
            </View>
          </View>
        </View>
      </Modal>
    </View>
  );
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
  },
  centered: {
    flex: 1,
    justifyContent: 'center',
    alignItems: 'center',
  },
  overlay: {
    ...StyleSheet.absoluteFillObject,
    backgroundColor: 'rgba(0,0,0,0.5)',
    justifyContent: 'center',
    alignItems: 'center',
    zIndex: 100,
  },
  overlayText: {
    color: '#fff',
    marginTop: 12,
    fontSize: 16,
  },
  modalOverlay: {
    flex: 1,
    backgroundColor: 'rgba(0,0,0,0.5)',
    justifyContent: 'center',
    alignItems: 'center',
    padding: 24,
  },
  modalContent: {
    backgroundColor: '#fff',
    borderRadius: 16,
    padding: 24,
    width: '100%',
    maxWidth: 360,
  },
  modalTitle: {
    fontSize: 20,
    fontWeight: '700',
    color: '#111827',
    marginBottom: 8,
  },
  modalSubtitle: {
    fontSize: 14,
    color: '#6b7280',
    marginBottom: 20,
  },
  pinInput: {
    borderWidth: 1,
    borderColor: '#e5e7eb',
    borderRadius: 12,
    padding: 16,
    fontSize: 24,
    textAlign: 'center',
    letterSpacing: 8,
    marginBottom: 16,
    color: '#111827',
  },
  errorText: {
    color: '#ef4444',
    fontSize: 14,
    marginBottom: 12,
    textAlign: 'center',
  },
  buttonRow: {
    flexDirection: 'row',
    gap: 12,
  },
  cancelButton: {
    flex: 1,
    padding: 14,
    borderRadius: 12,
    backgroundColor: '#f3f4f6',
    alignItems: 'center',
  },
  cancelButtonText: {
    color: '#374151',
    fontWeight: '600',
  },
  submitButton: {
    flex: 1,
    padding: 14,
    borderRadius: 12,
    backgroundColor: '#6366f1',
    alignItems: 'center',
  },
  submitButtonDisabled: {
    backgroundColor: '#a5b4fc',
  },
  submitButtonText: {
    color: '#fff',
    fontWeight: '600',
  },
});