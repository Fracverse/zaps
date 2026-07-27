import React, { useEffect, useState } from "react";
import { View, Text, StyleSheet, ScrollView } from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import { COLORS } from "../src/constants/colors";
import { Button } from "../src/components/Button";
import { PrivySocialButtons } from "../src/components/PrivySocialButtons";
import { Stack, useRouter } from "expo-router";

import Icon1 from "../assets/icon-1.svg";
import Icon2 from "../assets/icon-2.svg";
import Icon3 from "../assets/icon-3.svg";
import ZapsLogo from "../assets/zapsLogo.svg";

export default function OnboardingScreen() {
  const router = useRouter();
  const [showSplash, setShowSplash] = useState(true);

  useEffect(() => {
    const timer = setTimeout(() => {
      setShowSplash(false);
    }, 5000); // 5 seconds splash

    return () => clearTimeout(timer);
  }, []);

  if (showSplash) {
    return (
      <SafeAreaView style={styles.splashContainer}>
        <Stack.Screen options={{ headerShown: false }} />
        <View style={styles.splashContent}>
          <ZapsLogo width={216} height={103} style={styles.splashLogo} />
          <Text style={styles.splashText}>ZAPS</Text>
        </View>
      </SafeAreaView>
    );
  }

  return (
    <SafeAreaView style={styles.container}>
      <Stack.Screen options={{ headerShown: false }} />

      <ScrollView
        contentContainerStyle={styles.content}
        showsVerticalScrollIndicator={false}
      >
        <View style={styles.header}>
          <ZapsLogo width={116} height={53} style={styles.splashLogo} />
        </View>

        <View style={styles.featureContainer}>
          {/* Top Row - Instant */}
          <View style={styles.trackRow}>
            <View style={[styles.featureCard, styles.cardLeft]}>
              <Icon1 style={styles.icon} />
              <Text style={styles.featureText}>Instant</Text>
            </View>
          </View>

          {/* Middle Row - Non-Custodial */}
          <View style={styles.trackRow}>
            <View style={[styles.featureCard, styles.cardCenter]}>
              <Icon2 style={styles.icon} />
              <Text style={styles.featureText}>Non-Custodial</Text>
            </View>
          </View>

          {/* Bottom Row - Tap or Scan */}
          <View style={styles.trackRow}>
            <View style={[styles.featureCard, styles.cardRight]}>
              <Icon3 style={styles.icon} />
              <Text style={styles.featureText}>Tap or Scan</Text>
            </View>
          </View>
        </View>

        <View style={styles.titleContainer}>
          <Text style={styles.title}>PAY OR GET PAID</Text>
          <Text style={styles.title}>WITH CRYPTO</Text>
          <Text style={styles.subtitle}>
            Zaps is the fastest to move{"\n"}crypto around
          </Text>
        </View>

        <View style={styles.footer}>
          {/* Privy Social Connection Buttons */}
          <PrivySocialButtons nextRoute="/username" testIDPrefix="index-privy" />

          <View style={styles.dividerRow}>
            <View style={styles.dividerLine} />
            <Text style={styles.dividerText}>OR</Text>
            <View style={styles.dividerLine} />
          </View>

          <Button
            title="Set Up Wallet Manually"
            onPress={() => router.push("/onboarding-start")}
            variant="secondary"
            style={styles.manualButton}
            textStyle={styles.manualButtonText}
          />
        </View>
      </ScrollView>
    </SafeAreaView>
  );
}

const styles = StyleSheet.create({
  splashContainer: {
    flex: 1,
    backgroundColor: COLORS.secondary,
    justifyContent: "center",
    alignItems: "center",
  },
  splashContent: {
    alignItems: "center",
    gap: 10,
  },
  splashLogo: {
    marginBottom: 0,
  },
  splashText: {
    fontSize: 80,
    fontFamily: "Anton_400Regular",
    color: COLORS.primary,
    letterSpacing: 4,
    textTransform: "uppercase",
  },
  container: {
    flex: 1,
    backgroundColor: COLORS.secondary,
  },
  content: {
    paddingHorizontal: 20,
    paddingVertical: 20,
    justifyContent: "space-between",
    flexGrow: 1,
  },
  header: {
    alignItems: "center",
    marginBottom: 16,
    paddingTop: 10,
  },
  featureContainer: {
    marginVertical: 12,
    gap: 16,
    paddingHorizontal: 10,
  },
  trackRow: {
    width: "100%",
    backgroundColor: "#74D189",
    borderRadius: 100,
    height: 80,
    justifyContent: "center",
    padding: 5,
  },
  featureCard: {
    backgroundColor: COLORS.primary,
    borderRadius: 100,
    flexDirection: "row",
    alignItems: "center",
    paddingHorizontal: 24,
    paddingVertical: 18,
    position: "absolute",
    height: "100%",
  },
  cardLeft: {
    left: 5,
    paddingRight: 40,
    minWidth: "55%",
  },
  cardCenter: {
    alignSelf: "center",
    justifyContent: "center",
    minWidth: "60%",
  },
  cardRight: {
    right: 5,
    paddingLeft: 40,
    minWidth: "55%",
    flexDirection: "row",
    justifyContent: "center",
  },
  icon: {
    marginRight: 10,
    tintColor: "#AEDCBA",
  },
  featureText: {
    color: "#80FA98",
    fontSize: 18,
    fontFamily: "Outfit_500Medium",
  },
  titleContainer: {
    alignItems: "center",
    marginVertical: 16,
  },
  title: {
    fontSize: 40,
    fontFamily: "Anton_400Regular",
    color: COLORS.primary,
    textAlign: "center",
    lineHeight: 46,
    textTransform: "uppercase",
  },
  subtitle: {
    fontSize: 18,
    color: COLORS.primary,
    textAlign: "center",
    marginTop: 12,
    lineHeight: 22,
    fontFamily: "Outfit_500Medium",
  },
  footer: {
    paddingBottom: 20,
    gap: 12,
  },
  dividerRow: {
    flexDirection: "row",
    alignItems: "center",
    marginVertical: 4,
    gap: 12,
  },
  dividerLine: {
    flex: 1,
    height: 1,
    backgroundColor: "rgba(26,75,74,0.2)",
  },
  dividerText: {
    fontSize: 13,
    color: COLORS.primary,
    fontFamily: "Outfit_600SemiBold",
  },
  manualButton: {
    backgroundColor: "transparent",
    borderRadius: 100,
    height: 52,
    borderWidth: 1,
    borderColor: COLORS.primary,
  },
  manualButtonText: {
    fontSize: 16,
    fontFamily: "Outfit_600SemiBold",
    color: COLORS.primary,
  },
});

