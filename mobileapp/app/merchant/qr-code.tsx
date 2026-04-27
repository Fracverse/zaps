import React, { useEffect, useRef } from "react";
import {
  View,
  Text,
  StyleSheet,
  TouchableOpacity,
  StatusBar,
  SafeAreaView,
  Animated,
  Share,
} from "react-native";
import { router, useLocalSearchParams } from "expo-router";
import { Ionicons } from "@expo/vector-icons";
import QRCode from "react-native-qrcode-svg";
import { buildSep0007PayUri } from "../../src/utils/sep0007";

// Merchant's Stellar address — replace with real address from auth context
const MERCHANT_STELLAR_ADDRESS =
  "GABC1234EXAMPLESTELLARADDRESSXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX";

const QRCodeScreen = () => {
  const { amount } = useLocalSearchParams();
  const fadeAnim = useRef(new Animated.Value(0)).current;
  const scaleAnim = useRef(new Animated.Value(0.8)).current;
  const slideAnim = useRef(new Animated.Value(30)).current;

  // Build a SEP-0007 pay URI so any compatible wallet can scan and pay
  const sep0007Uri = buildSep0007PayUri({
    destination: MERCHANT_STELLAR_ADDRESS,
    ...(amount ? { amount: String(amount) } : {}),
    asset_code: "USDC",
    memo: "Blink merchant payment",
  });

  useEffect(() => {
    // Staggered animation entrance
    Animated.sequence([
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
      ]),
      Animated.spring(scaleAnim, {
        toValue: 1,
        tension: 100,
        friction: 8,
        useNativeDriver: true,
      }),
    ]).start();
  }, [fadeAnim, scaleAnim, slideAnim]);

  const handleShare = async () => {
    try {
      await Share.share({
        message: sep0007Uri,
        title: "Blink Payment QR",
      });
    } catch (error) {
      console.log("Share error:", error);
    }
  };

  const handleContinue = () => {
    // Navigate to waiting payment screen with amount parameter
    Animated.parallel([
      Animated.timing(fadeAnim, {
        toValue: 0,
        duration: 300,
        useNativeDriver: true,
      }),
      Animated.timing(scaleAnim, {
        toValue: 0.9,
        duration: 300,
        useNativeDriver: true,
      }),
    ]);
    // .start(() => {
    //   router.push(`/merchant/waiting-payment?amount=${amount}`);
    // });
  };

  return (
    <SafeAreaView style={styles.container}>
      <StatusBar barStyle="dark-content" backgroundColor="#fff" />

      {/* Header */}
      <View style={styles.header}>
        <TouchableOpacity
          onPress={() => router.back()}
          style={styles.backButton}
        >
          <Ionicons name="arrow-back" size={24} color="#000" />
        </TouchableOpacity>
        <Text style={styles.headerTitle}>Accept Payment</Text>
        <View style={styles.placeholder} />
      </View>

      <Animated.View
        style={[
          styles.content,
          {
            opacity: fadeAnim,
            transform: [{ translateY: slideAnim }],
          },
        ]}
      >
        {/* QR Code Container */}
        <Animated.View
          style={[
            styles.qrContainer,
            {
              transform: [{ scale: scaleAnim }],
            },
          ]}
        >
          <View style={styles.qrCode}>
            <QRCode
              value={sep0007Uri}
              size={240}
              color="#000"
              backgroundColor="#fff"
            />
          </View>
        </Animated.View>

        {/* Share Button */}
        <Animated.View style={{ opacity: fadeAnim }}>
          <TouchableOpacity style={styles.shareButton} onPress={handleShare}>
            <Ionicons name="share-outline" size={20} color="#666" />
            <Text style={styles.shareButtonText}>Share qr code</Text>
          </TouchableOpacity>
        </Animated.View>
      </Animated.View>

      {/* Continue Button */}
      <View style={styles.bottomContainer}>
        <TouchableOpacity
          style={styles.continueButton}
          onPress={handleContinue}
        >
          <Text style={styles.continueButtonText}>Continue</Text>
        </TouchableOpacity>
      </View>
    </SafeAreaView>
  );
};

const styles = StyleSheet.create({
  container: {
    flex: 1,
    backgroundColor: "#fff",
  },
  header: {
    flexDirection: "row",
    alignItems: "center",
    justifyContent: "space-between",
    paddingHorizontal: 20,
    paddingVertical: 15,
    paddingTop: 40,
    borderBottomWidth: 1,
    borderBottomColor: "#f0f0f0",
  },
  backButton: {
    padding: 5,
  },
  headerTitle: {
    fontSize: 18,
    fontWeight: "600",
    color: "#000",
  },
  placeholder: {
    width: 34,
  },
  content: {
    flex: 1,
    paddingHorizontal: 20,
    paddingTop: 40,
    alignItems: "center",
  },
  qrContainer: {
    backgroundColor: "#f8f8f8",
    borderRadius: 20,
    padding: 30,
    marginBottom: 40,
  },
  qrCode: {
    width: 280,
    height: 280,
    backgroundColor: "#fff",
    borderRadius: 12,
    padding: 20,
    justifyContent: "center",
    alignItems: "center",
  },
  shareButton: {
    flexDirection: "row",
    alignItems: "center",
    backgroundColor: "#f0f0f0",
    paddingHorizontal: 30,
    paddingVertical: 15,
    borderRadius: 25,
    gap: 8,
  },
  shareButtonText: {
    fontSize: 16,
    color: "#666",
    fontWeight: "500",
  },
  bottomContainer: {
    paddingHorizontal: 20,
    paddingBottom: 30,
  },
  continueButton: {
    backgroundColor: "#1A4B4A",
    borderRadius: 30,
    paddingVertical: 20,
    alignItems: "center",
  },
  continueButtonText: {
    color: "#80FA98",
    fontSize: 18,
    fontWeight: "600",
    letterSpacing: 0.5,
  },
});

export default QRCodeScreen;
