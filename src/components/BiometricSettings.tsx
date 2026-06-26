import React, { useState } from 'react';
import {
  View,
  Text,
  Switch,
  TouchableOpacity,
  StyleSheet,
  Alert,
  ActivityIndicator,
} from 'react-native';
import { useBiometric } from '../hooks/useBiometric';

export default function BiometricSettings() {
  const biometric = useBiometric();
  const [showPINSetup, setShowPINSetup] = useState(false);
  const [newPIN, setNewPIN] = useState('');

  const handleToggle = async (value: boolean) => {
    try {
      if (value) {
        // Enabling biometric
        await biometric.enableBiometric();
        
        // Suggest setting up PIN fallback
        if (!biometric.hasPIN) {
          Alert.alert(
            'Set Up PIN Fallback',
            'Would you like to set up a PIN as a backup authentication method?',
            [
              { text: 'Not Now', style: 'cancel' },
              { text: 'Set Up', onPress: () => setShowPINSetup(true) },
            ]
          );
        }
      } else {
        // Disabling biometric — require auth first
        const success = await biometric.authenticate('securitySettings');
        if (success) {
          await biometric.disableBiometric();
        }
      }
    } catch (err) {
      Alert.alert('Error', err instanceof Error ? err.message : 'Failed to update settings');
    }
  };

  const handlePINSetup = async () => {
    if (newPIN.length < 4) {
      Alert.alert('Invalid PIN', 'PIN must be at least 4 digits');
      return;
    }
    
    try {
      await biometric.setupPIN(newPIN);
      setShowPINSetup(false);
      setNewPIN('');
      Alert.alert('Success', 'PIN fallback has been set up');
    } catch (err) {
      Alert.alert('Error', err instanceof Error ? err.message : 'Failed to set PIN');
    }
  };

  if (biometric.isLoading) {
    return (
      <View style={styles.container}>
        <ActivityIndicator />
      </View>
    );
  }

  const status = biometric.status;
  const available = status?.available ?? false;
  const enrolled = status?.enrolled ?? false;
  const types = status?.types ?? [];

  const typeLabel = types.includes('face-id')
    ? 'Face ID'
    : types.includes('fingerprint')
    ? 'Fingerprint'
    : types.includes('iris')
    ? 'Iris'
    : 'Biometric';

  return (
    <View style={styles.container}>
      <Text style={styles.header}>Security</Text>
      
      {/* Biometric Toggle */}
      <View style={styles.row}>
        <View style={styles.rowContent}>
          <Text style={styles.rowTitle}>{typeLabel}</Text>
          <Text style={styles.rowSubtitle}>
            {available 
              ? enrolled 
                ? `Use ${typeLabel.toLowerCase()} to secure your wallet`
                : `${typeLabel} not enrolled. Set up in device settings.`
              : 'Not available on this device'
            }
          </Text>
        </View>
        <Switch
          value={biometric.enabled}
          onValueChange={handleToggle}
          disabled={!available || !enrolled}
          trackColor={{ false: '#e5e7eb', true: '#6366f1' }}
        />
      </View>

      {/* PIN Fallback */}
      {biometric.enabled && (
        <View style={styles.section}>
          <Text style={styles.sectionTitle}>Fallback Authentication</Text>
          
          <View style={styles.row}>
            <View style={styles.rowContent}>
              <Text style={styles.rowTitle}>PIN Code</Text>
              <Text style={styles.rowSubtitle}>
                {biometric.hasPIN 
                  ? 'PIN fallback is set up' 
                  : 'Set up a PIN as backup'}
              </Text>
            </View>
            {!biometric.hasPIN && (
              <TouchableOpacity 
                style={styles.setupButton}
                onPress={() => setShowPINSetup(true)}
              >
                <Text style={styles.setupButtonText}>Set Up</Text>
              </TouchableOpacity>
            )}
          </View>
        </View>
      )}

      {/* Enrollment Warning */}
      {status?.hasChanged && (
        <View style={styles.warningBox}>
          <Text style={styles.warningText}>
            ⚠️ Your biometric enrollment has changed. Please re-authenticate to continue using biometric security.
          </Text>
        </View>
      )}

      {/* PIN Setup Modal */}
      {showPINSetup && (
        <View style={styles.pinModal}>
          <Text style={styles.pinModalTitle}>Create PIN</Text>
          <Text style={styles.pinModalSubtitle}>Enter a 4-6 digit PIN for fallback</Text>
          
          {/* PIN input would go here — simplified for brevity */}
          <TouchableOpacity style={styles.confirmButton} onPress={handlePINSetup}>
            <Text style={styles.confirmButtonText}>Save PIN</Text>
          </TouchableOpacity>
          
          <TouchableOpacity onPress={() => setShowPINSetup(false)}>
            <Text style={styles.cancelText}>Cancel</Text>
          </TouchableOpacity>
        </View>
      )}
    </View>
  );
}

const styles = StyleSheet.create({
  container: {
    padding: 16,
  },
  header: {
    fontSize: 24,
    fontWeight: '700',
    color: '#111827',
    marginBottom: 20,
  },
  section: {
    marginTop: 24,
  },
  sectionTitle: {
    fontSize: 14,
    fontWeight: '600',
    color: '#6b7280',
    textTransform: 'uppercase',
    letterSpacing: 0.5,
    marginBottom: 12,
  },
  row: {
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'space-between',
    paddingVertical: 12,
    borderBottomWidth: 1,
    borderBottomColor: '#f3f4f6',
  },
  rowContent: {
    flex: 1,
    marginRight: 12,
  },
  rowTitle: {
    fontSize: 16,
    fontWeight: '500',
    color: '#111827',
  },
  rowSubtitle: {
    fontSize: 13,
    color: '#6b7280',
    marginTop: 2,
  },
  setupButton: {
    backgroundColor: '#6366f1',
    paddingHorizontal: 16,
    paddingVertical: 8,
    borderRadius: 8,
  },
  setupButtonText: {
    color: '#fff',
    fontWeight: '600',
    fontSize: 14,
  },
  warningBox: {
    backgroundColor: '#fef3c7',
    borderRadius: 12,
    padding: 16,
    marginTop: 20,
  },
  warningText: {
    color: '#92400e',
    fontSize: 14,
    lineHeight: 20,
  },
  pinModal: {
    marginTop: 20,
    padding: 20,
    backgroundColor: '#f9fafb',
    borderRadius: 16,
  },
  pinModalTitle: {
    fontSize: 18,
    fontWeight: '600',
    color: '#111827',
  },
  pinModalSubtitle: {
    fontSize: 14,
    color: '#6b7280',
    marginTop: 4,
    marginBottom: 16,
  },
  confirmButton: {
    backgroundColor: '#6366f1',
    padding: 14,
    borderRadius: 12,
    alignItems: 'center',
  },
  confirmButtonText: {
    color: '#fff',
    fontWeight: '600',
  },
  cancelText: {
    color: '#6b7280',
    textAlign: 'center',
    marginTop: 12,
    fontSize: 14,
  },
});