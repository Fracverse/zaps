import { StatusBar } from 'expo-status-bar';
import {
  StyleSheet,
  Text,
  View,
  TouchableOpacity,
  SafeAreaView,
} from 'react-native';

export default function App() {
  return (
    <SafeAreaView style={styles.container}>
      <StatusBar style="light" />

      {/* Header */}
      <View style={styles.header}>
        <Text style={styles.logo}>⚡ ZAPS</Text>
        <Text style={styles.tagline}>
          Pay with crypto.{'\n'}Tap or scan. No banks.
        </Text>
      </View>

      {/* Main Card */}
      <View style={styles.card}>
        <Text style={styles.cardTitle}>How it works</Text>

        <Text style={styles.cardText}>1️⃣ Transfer crypto into ZAPS</Text>
        <Text style={styles.cardText}>2️⃣ Tap or scan to pay</Text>
        <Text style={styles.cardText}>3️⃣ Merchant receives USD instantly</Text>
      </View>

      {/* Actions */}
      <View style={styles.actions}>
        <TouchableOpacity
          style={styles.primaryButton}
          accessibilityRole="button"
          accessibilityLabel="Scan to Pay"
          accessibilityHint="Opens camera to scan a QR code for payment"
        >
          <Text style={styles.primaryButtonText}>📷 Scan to Pay</Text>
        </TouchableOpacity>

        <TouchableOpacity
          style={styles.secondaryButton}
          accessibilityRole="button"
          accessibilityLabel="Tap to Pay"
          accessibilityHint="Initiates NFC payment"
        >
          <Text style={styles.secondaryButtonText}>📱 Tap to Pay</Text>
        </TouchableOpacity>
      </View>

      {/* Footer */}
      <View style={styles.footer}>
        <Text style={styles.footerText}>
          Built on Stellar • No Apple Pay • No Google Pay
        </Text>
      </View>
    </SafeAreaView>
  );
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
    backgroundColor: '#0B0F1A',
    paddingHorizontal: 20,
  },

  header: {
    marginTop: 40,
    marginBottom: 30,
  },

  logo: {
    fontSize: 32,
    fontWeight: '800',
    color: '#FFFFFF',
    marginBottom: 10,
  },

  tagline: {
    fontSize: 20,
    color: '#AAB1C3',
    lineHeight: 28,
  },

  card: {
    backgroundColor: '#12182A',
    borderRadius: 20,
    padding: 20,
    marginBottom: 30,
  },

  cardTitle: {
    fontSize: 18,
    fontWeight: '700',
    color: '#FFFFFF',
    marginBottom: 15,
  },

  cardText: {
    fontSize: 16,
    color: '#D0D6E1',
    marginBottom: 8,
  },

  actions: {
    gap: 15,
  },

  primaryButton: {
    backgroundColor: '#4F46E5',
    paddingVertical: 16,
    borderRadius: 14,
    alignItems: 'center',
  },

  primaryButtonText: {
    color: '#FFFFFF',
    fontSize: 16,
    fontWeight: '700',
  },

  secondaryButton: {
    backgroundColor: '#1E243B',
    paddingVertical: 16,
    borderRadius: 14,
    alignItems: 'center',
    borderWidth: 1,
    borderColor: '#2F365F',
  },

  secondaryButtonText: {
    color: '#FFFFFF',
    fontSize: 16,
    fontWeight: '600',
  },

  footer: {
    marginTop: 'auto',
    paddingVertical: 20,
    alignItems: 'center',
  },

  footerText: {
    color: '#6B7280',
    fontSize: 12,
  },
});
