import React, { useEffect, useRef } from 'react';
import {
  View,
  Text,
  StyleSheet,
  TouchableOpacity,
  StatusBar,
  SafeAreaView,
  Image,
  Animated,
} from 'react-native';
import { router, useLocalSearchParams } from 'expo-router';

const ContactMadeScreen = () => {
  const { amount } = useLocalSearchParams();
  const fadeAnim = useRef(new Animated.Value(0)).current;
  const scaleAnim = useRef(new Animated.Value(0.8)).current;
  const slideAnim = useRef(new Animated.Value(30)).current;

  useEffect(() => {
    // Entrance animation
    Animated.parallel([
      Animated.timing(fadeAnim, {
        toValue: 1,
        duration: 500,
        useNativeDriver: true,
      }),
      Animated.timing(slideAnim, {
        toValue: 0,
        duration: 500,
        useNativeDriver: true,
      }),
      Animated.spring(scaleAnim, {
        toValue: 1,
        tension: 100,
        friction: 8,
        useNativeDriver: true,
      }),
    ]).start();

    // Set timer to navigate to payment-received screen after 5 seconds
    const timer = setTimeout(() => {
      Animated.parallel([
        Animated.timing(fadeAnim, {
          toValue: 0,
          duration: 400,
          useNativeDriver: true,
        }),
        Animated.timing(scaleAnim, {
          toValue: 1.1,
          duration: 400,
          useNativeDriver: true,
        }),
      ]).start(() => {
        router.push(`/merchant/payment-received?amount=${amount}`);
      });
    }, 5000); // 5 seconds

    // Cleanup timer if component unmounts
    return () => clearTimeout(timer);
  }, [amount, fadeAnim, scaleAnim, slideAnim]);

  const handleScanInstead = () => {
    // Navigate back to accept payment or handle scan functionality
    router.push('/merchant/accept-payment');
  };

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
        {/* Contact Icon */}
        <Animated.View 
          style={[
            styles.iconContainer,
            {
              transform: [{ scale: scaleAnim }],
            }
          ]}
        >
          <Image 
            source={require('../../assets/Cardholder.png')} 
            style={styles.cardholderImage}
            resizeMode="contain"
          />
        </Animated.View>

        {/* Contact Made Text */}
        <Animated.View style={{ opacity: fadeAnim }}>
          <Text style={styles.contactText}>Contact Made</Text>
        </Animated.View>

        {/* Status Text */}
        <Animated.View style={{ opacity: fadeAnim }}>
          <Text style={styles.statusText}>Payment is being processed...</Text>
        </Animated.View>
      </Animated.View>

      {/* Bottom Section */}
      <View style={styles.bottomContainer}>
        <Text style={styles.acceptPaymentText}>Accept Payment</Text>
        <TouchableOpacity style={styles.scanButton} onPress={handleScanInstead}>
          <Text style={styles.scanButtonText}>Scan instead</Text>
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
  cardholderImage: {
    width: 120,
    height: 120,
  },
  contactText: {
    fontSize: 24,
    fontWeight: '600',
    color: '#000',
    marginBottom: 20,
    textAlign: 'center',
  },
  statusText: {
    fontSize: 16,
    color: '#999',
    textAlign: 'center',
  },
  bottomContainer: {
    paddingHorizontal: 20,
    paddingBottom: 30,
    alignItems: 'center',
  },
  acceptPaymentText: {
    fontSize: 18,
    fontWeight: '600',
    color: '#000',
    marginBottom: 15,
  },
  scanButton: {
    backgroundColor: '#1A4B4A',
    borderRadius: 30,
    paddingVertical: 20,
    paddingHorizontal: 40,
    width: '100%',
    alignItems: 'center',
  },
  scanButtonText: {
    color: '#80FA98',
    fontSize: 18,
    fontWeight: '600',
    letterSpacing: 0.5,
  },
});

export default ContactMadeScreen;