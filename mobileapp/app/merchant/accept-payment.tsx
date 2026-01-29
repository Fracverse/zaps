import React, { useState } from 'react';
import {
  View,
  Text,
  StyleSheet,
  TouchableOpacity,
  TextInput,
  StatusBar,
  SafeAreaView,
} from 'react-native';
import { router } from 'expo-router';
import { Ionicons } from '@expo/vector-icons';

const predefinedAmounts = [5, 10, 15, 20, 35, 50, 75, 100];

const AcceptPayment = () => {
  const [amount, setAmount] = useState('');


  const handleAmountSelect = (value: number) => {
    setAmount(value.toString());
  };

  const handleGenerateQR = () => {
    // Navigate to QR code screen with amount parameter
    if (amount) {
      router.push(`/merchant/waiting-payment?amount=${amount}`);
    }
  };

  return (
    <SafeAreaView style={styles.container}>
      <StatusBar barStyle="dark-content" backgroundColor="#fff" />

      {/* Header */}
      <View style={styles.header}>
        <TouchableOpacity onPress={() => router.back()} style={styles.backButton}>
          <Ionicons name="arrow-back" size={24} color="#000" />
        </TouchableOpacity>
        <Text style={styles.headerTitle}>Accept Payment</Text>
        <View style={styles.placeholder} />
      </View>

      <View style={styles.content}>
        {/* Amount Input Section */}
        <Text style={styles.label}>Enter amount to collect</Text>

        <View style={styles.amountInputContainer}>
          <Text style={styles.dollarSign}>$</Text>
          <TextInput
            style={styles.amountInput}
            placeholder="Amount"
            placeholderTextColor="#999"
            value={amount}
            onChangeText={setAmount}
            keyboardType="numeric"
          />
        </View>

        {/* Predefined Amount Buttons */}
        <View style={styles.amountGrid}>
          {predefinedAmounts.map((value) => {
            const isSelected = amount === value.toString();
            return (
              <TouchableOpacity
                key={value}
                style={[
                  styles.amountButton,
                  isSelected && styles.selectedAmountButton
                ]}
                onPress={() => handleAmountSelect(value)}
                activeOpacity={0.7}
              >
                <Text style={[
                  styles.amountButtonText,
                  isSelected && styles.selectedAmountButtonText
                ]}>
                  ${value}
                </Text>
              </TouchableOpacity>
            );
          })}
        </View>
      </View>

      {/* Generate QR Button */}
      <View style={styles.bottomContainer}>
        <TouchableOpacity
          style={[
            styles.generateButton,
            !amount && styles.generateButtonDisabled
          ]}
          onPress={handleGenerateQR}
          disabled={!amount}
        >
          <Text style={[
            styles.generateButtonText,
            !amount && styles.generateButtonTextDisabled
          ]}>
            Generate qr code
          </Text>
        </TouchableOpacity>
      </View>
    </SafeAreaView>
  );
};

const styles = StyleSheet.create({
  container: {
    flex: 1,
    backgroundColor: '#fff',
  },
  header: {
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'space-between',
    paddingHorizontal: 20,
    paddingVertical: 15,
    paddingTop: 40,
    borderBottomWidth: 1,
    borderBottomColor: '#f0f0f0',
  },
  backButton: {
    padding: 5,
  },
  headerTitle: {
    fontSize: 18,
    fontWeight: '600',
    color: '#000',
  },
  placeholder: {
    width: 34,
  },
  content: {
    flex: 1,
    paddingHorizontal: 20,
    paddingTop: 30,
  },
  label: {
    fontSize: 16,
    color: '#666',
    marginBottom: 20,
  },
  amountInputContainer: {
    flexDirection: 'row',
    alignItems: 'center',
    borderBottomWidth: 1,
    borderBottomColor: '#e0e0e0',
    paddingBottom: 10,
    marginBottom: 40,
  },
  dollarSign: {
    fontSize: 24,
    fontWeight: '500',
    color: '#000',
    marginRight: 10,
  },
  amountInput: {
    flex: 1,
    fontSize: 24,
    fontWeight: '500',
    color: '#000',
  },
  amountGrid: {
    flexDirection: 'row',
    flexWrap: 'wrap',
    justifyContent: 'space-between',
  },
  amountButton: {
    width: '22%',
    aspectRatio: 1,
    backgroundColor: '#f8f8f8',
    borderRadius: 12,
    justifyContent: 'center',
    alignItems: 'center',
    marginBottom: 15,
    borderWidth: 1,
    borderColor: '#e0e0e0',
  },
  selectedAmountButton: {
    backgroundColor: '#1A4B4A',
    borderColor: '#1A4B4A',
  },
  amountButtonText: {
    fontSize: 16,
    fontWeight: '600',
    color: '#333',
  },
  selectedAmountButtonText: {
    color: '#80FA98',
  },
  bottomContainer: {
    paddingHorizontal: 20,
    paddingBottom: 30,
  },
  generateButton: {
    backgroundColor: '#1A4B4A',
    borderRadius: 30,
    paddingVertical: 20,
    alignItems: 'center',
  },
  generateButtonDisabled: {
    backgroundColor: '#ccc',
  },
  generateButtonText: {
    color: '#80FA98',
    fontSize: 18,
    fontWeight: '600',
    letterSpacing: 0.5,
  },
  generateButtonTextDisabled: {
    color: '#999',
  },
});

export default AcceptPayment;