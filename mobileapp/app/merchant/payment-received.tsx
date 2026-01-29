import React, { useEffect, useRef } from 'react';
import {
  View,
  Text,
  StyleSheet,
  TouchableOpacity,
  StatusBar,
  SafeAreaView,
  Animated,
} from 'react-native';
import { router, useLocalSearchParams } from 'expo-router';
import { Ionicons } from '@expo/vector-icons';

const PaymentReceivedScreen = () => {
  const { amount } = useLocalSearchParams();
  const fadeAnim = useRef(new Animated.Value(0)).current;
  const scaleAnim = useRef(new Animated.Value(0)).current;
  const bounceAnim = useRef(new Animated.Value(0.8)).current;
  const slideAnim = useRef(new Animated.Value(50)).current;

  useEffect(() => {
    // Success celebration animation sequence
    Animated.sequence([
      // Initial fade in
      Animated.timing(fadeAnim, {
        toValue: 1,
        duration: 400,
        useNativeDriver: true,
      }),
      // Success icon bounce
      Animated.spring(scaleAnim, {
        toValue: 1,
        tension: 150,
        friction: 6,
        useNativeDriver: true,
      }),
      // Content slide up
      Animated.parallel([
        Animated.spring(bounceAnim, {
          toValue: 1,
          tension: 100,
          friction: 8,
          useNativeDriver: true,
        }),
        Animated.timing(slideAnim, {
          toValue: 0,
          duration: 600,
          useNativeDriver: true,
        }),
      ]),
    ]).start();
  }, [fadeAnim, scaleAnim, bounceAnim, slideAnim]);

  const handleDone = () => {
    // Exit animation before navigation
    Animated.parallel([
      Animated.timing(fadeAnim, {
        toValue: 0,
        duration: 300,
        useNativeDriver: true,
      }),
      Animated.timing(slideAnim, {
        toValue: -30,
        duration: 300,
        useNativeDriver: true,
      }),
    ]).start(() => {
      router.push('/merchant');
    });
  };

  return (
    <SafeAreaView style={styles.container}>
      <StatusBar barStyle="dark-content" backgroundColor="#fff" />
      
      <Animated.View 
        style={[
          styles.content,
          {
            opacity: fadeAnim,
          }
        ]}
      >
        {/* Success Icon */}
        <Animated.View 
          style={[
            styles.iconContainer,
            {
              transform: [{ scale: scaleAnim }],
            }
          ]}
        >
          <View style={styles.successIcon}>
            <Ionicons name="checkmark" size={36} color="#1A4B4A" />
          </View>
        </Animated.View>

        {/* Payment Received Text */}
        <Animated.View 
          style={{
            opacity: fadeAnim,
            transform: [{ scale: bounceAnim }, { translateY: slideAnim }],
          }}
        >
          <Text style={styles.receivedText}>Payment Received</Text>
        </Animated.View>

        {/* Amount Display */}
        <Animated.View 
          style={{
            opacity: fadeAnim,
            transform: [{ scale: bounceAnim }, { translateY: slideAnim }],
          }}
        >
          <View style={styles.amountContainer}>
            <Text style={styles.amountText}>${amount || '0'}</Text>
          </View>
        </Animated.View>

        {/* Status Text */}
        <Animated.View 
          style={{
            opacity: fadeAnim,
            transform: [{ translateY: slideAnim }],
          }}
        >
          <Text style={styles.statusText}>USD added to your balance</Text>
        </Animated.View>
      </Animated.View>

      {/* Done Button */}
      <View style={styles.bottomContainer}>
        <TouchableOpacity style={styles.doneButton} onPress={handleDone}>
          <Text style={styles.doneButtonText}>Done</Text>
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
  content: {
    flex: 1,
    justifyContent: 'center',
    alignItems: 'center',
    paddingHorizontal: 20,
  },
  iconContainer: {
    marginBottom: 80,
  },
  successIcon: {
    width: 80,
    height: 80,
    borderRadius: 40,
    borderWidth: 3,
    borderColor: '#1A4B4A',
    justifyContent: 'center',
    alignItems: 'center',
    backgroundColor: '#fff',
  },
  receivedText: {
    fontSize: 24,
    fontWeight: '600',
    color: '#000',
    marginBottom: 40,
    textAlign: 'center',
  },
  amountContainer: {
    borderWidth: 2,
    borderColor: '#000',
    borderRadius: 25,
    paddingHorizontal: 30,
    paddingVertical: 15,
    marginBottom: 20,
  },
  amountText: {
    fontSize: 32,
    fontWeight: 'bold',
    color: '#000',
  },
  statusText: {
    fontSize: 16,
    color: '#999',
    textAlign: 'center',
  },
  bottomContainer: {
    paddingHorizontal: 20,
    paddingBottom: 30,
  },
  doneButton: {
    backgroundColor: '#1A4B4A',
    borderRadius: 30,
    paddingVertical: 20,
    alignItems: 'center',
  },
  doneButtonText: {
    color: '#80FA98',
    fontSize: 18,
    fontWeight: '600',
    letterSpacing: 0.5,
  },
});

export default PaymentReceivedScreen;