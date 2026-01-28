import React, { useEffect } from 'react';
import {
  View,
  Text,
  StyleSheet,
  StatusBar,
  SafeAreaView,
  Animated,
} from 'react-native';
import { useLocalSearchParams, router } from 'expo-router';

const WaitingPaymentScreen = () => {
  const { amount } = useLocalSearchParams();
  const fadeAnim = new Animated.Value(0);
  const pulseAnim = new Animated.Value(1);
  const slideAnim = new Animated.Value(50);

  useEffect(() => {
    // Initial entrance animation
    Animated.parallel([
      Animated.timing(fadeAnim, {
        toValue: 1,
        duration: 600,
        useNativeDriver: true,
      }),
      Animated.timing(slideAnim, {
        toValue: 0,
        duration: 600,
        useNativeDriver: true,
      }),
    ]).start();

    // Pulsing animation for the dollar icon
    const pulseAnimation = Animated.loop(
      Animated.sequence([
        Animated.timing(pulseAnim, {
          toValue: 1.1,
          duration: 1000,
          useNativeDriver: true,
        }),
        Animated.timing(pulseAnim, {
          toValue: 1,
          duration: 1000,
          useNativeDriver: true,
        }),
      ])
    );
    pulseAnimation.start();

    const timer = setTimeout(() => {
      Animated.timing(fadeAnim, {
        toValue: 0,
        duration: 400,
        useNativeDriver: true,
      }).start(() => {
        router.push(`/merchant/contact-made?amount=${amount}`);
      });
    }, 20000); // 20 seconds

    // Cleanup timer if component unmounts
    return () => {
      clearTimeout(timer);
      pulseAnimation.stop();
    };
  }, [amount]);

  return (
    <SafeAreaView style={styles.container}>
      <StatusBar barStyle="dark-content" backgroundColor="#fff" />
      
      <Animated.View 
        style={[
          styles.content,
          {
            opacity: fadeAnim,
            transform: [{ translateY: slideAnim }],
          }
        ]}
      >
        {/* Dollar Icon */}
        <Animated.View 
          style={[
            styles.iconContainer,
            {
              transform: [{ scale: pulseAnim }],
            }
          ]}
        >
          <View style={styles.dollarIcon}>
            <Text style={styles.dollarSymbol}>$</Text>
          </View>
        </Animated.View>

        {/* Waiting Text */}
        <Animated.View style={{ opacity: fadeAnim }}>
          <Text style={styles.waitingText}>Waiting for Payment...</Text>
        </Animated.View>

        {/* Amount Display */}
        <Animated.View style={{ opacity: fadeAnim }}>
          <View style={styles.amountContainer}>
            <Text style={styles.amountText}>${amount || '0'}</Text>
          </View>
        </Animated.View>

        {/* Status Text */}
        <Animated.View style={{ opacity: fadeAnim }}>
          <Text style={styles.statusText}>${amount || '0'} waiting to be confirmed</Text>
        </Animated.View>
      </Animated.View>
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
  dollarIcon: {
    width: 80,
    height: 80,
    borderRadius: 40,
    borderWidth: 3,
    borderColor: '#1A4B4A',
    justifyContent: 'center',
    alignItems: 'center',
    backgroundColor: '#fff',
  },
  dollarSymbol: {
    fontSize: 36,
    fontWeight: 'bold',
    color: '#1A4B4A',
  },
  waitingText: {
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
});

export default WaitingPaymentScreen;