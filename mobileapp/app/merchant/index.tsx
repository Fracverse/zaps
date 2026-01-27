import React from 'react';
import { View, Text, StyleSheet } from 'react-native';
import { Stack } from 'expo-router';
import { COLORS } from '../../src/constants/colors';

export default function MerchantDashboard() {
  return (
    <View style={styles.container}>
      <Stack.Screen options={{ title: 'Merchant Dashboard', headerShown: true }} />
      <Text style={styles.text}>Merchant Dashboard</Text>
      <Text style={styles.subtext}>Logic and UI to be added by contributors</Text>
    </View>
  );
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
    justifyContent: 'center',
    alignItems: 'center',
    backgroundColor: COLORS.white,
  },
  text: {
    fontSize: 24,
    fontFamily: 'Outfit_700Bold',
    color: COLORS.primary,
  },
  subtext: {
    fontSize: 16,
    fontFamily: 'Outfit_400Regular',
    color: '#666',
    marginTop: 10,
  },
});
