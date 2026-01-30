import React, { useEffect, useState, useRef } from "react";
import { View, Text, StyleSheet, Animated, Easing } from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import { Stack, useRouter } from "expo-router";
import { COLORS } from "../../src/constants/colors";
import { Button } from "../../src/components/Button";
import { Ionicons } from "@expo/vector-icons";
import BlinkLogo from "../../assets/blinkLogo.svg";

export default function RecoveryStatusScreen() {
  const router = useRouter();
  const [status, setStatus] = useState<"loading" | "found">("loading");
  
  // Animation values
  const pulseAnim = useRef(new Animated.Value(1)).current;

  useEffect(() => {
    // Simulate API call/loading
    const timer = setTimeout(() => {
      setStatus("found");
    }, 3000); // 3 seconds loading

    // Pulsing animation
    const startPulse = () => {
      Animated.loop(
        Animated.sequence([
          Animated.timing(pulseAnim, {
            toValue: 1.2,
            duration: 1000,
            useNativeDriver: true,
            easing: Easing.inOut(Easing.ease),
          }),
          Animated.timing(pulseAnim, {
            toValue: 1,
            duration: 1000,
            useNativeDriver: true,
            easing: Easing.inOut(Easing.ease),
          }),
        ])
      ).start();
    };

    if (status === "loading") {
      startPulse();
    }

    return () => clearTimeout(timer);
  }, [status, pulseAnim]);

  const handleContinue = () => {
    router.replace("/returning-user/password-setup");
  };

  return (
    <SafeAreaView style={styles.container}>
      <Stack.Screen options={{ headerShown: false }} />

      <View style={styles.content}>
        <View style={styles.centerContainer}>
          {status === "loading" ? (
            <>
              <View style={styles.spinnerContainer}>
                 <Animated.View
                  style={[
                    {
                      transform: [{ scale: pulseAnim }],
                    },
                  ]}
                >
                   <View style={styles.logoWrapper}>
                      <BlinkLogo width={60} height={28} />
                   </View>
                </Animated.View>
              </View>
              <Text style={styles.title}>Recovering Account...</Text>
              <Text style={styles.subtitle}>Please wait</Text>
            </>
          ) : (
            <>
              <View style={styles.successOuter}>
                <View
                  style={[
                    styles.successRing,
                    { width: 220, height: 220, opacity: 0.4 },
                  ]}
                />
                <View
                  style={[
                    styles.successRing,
                    { width: 180, height: 180, opacity: 0.4 },
                  ]}
                />
                <View style={styles.successCheck}>
                  <Ionicons name="checkmark" size={60} color="#0E4A47" />
                </View>
              </View>
              <Text style={styles.title}>Account Found</Text>
              <Text style={styles.subtitle}>
                An account was found you can proceed
              </Text>
            </>
          )}
        </View>
      </View>

      <View style={styles.footer}>
        {status === "loading" ? (
           <Button
            title="Scan instead"
            onPress={() => {}} // Placeholder
            variant="primary"
            style={styles.button}
          />
        ) : (
          <Button
            title="Continue"
            onPress={handleContinue}
            variant="primary"
            style={styles.button}
          />
        )}
      </View>
    </SafeAreaView>
  );
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
    backgroundColor: COLORS.white,
  },
  content: {
    flex: 1,
    justifyContent: "flex-end",
    paddingBottom: 150,
    alignItems: "center",
    paddingHorizontal: 20,
  },
  centerContainer: {
    alignItems: "center",
    marginBottom: 0, 
  },
  spinnerContainer: {
    marginBottom: 104,
    justifyContent: 'center',
    alignItems: 'center',
    height: 100, // Enough space for animation
  },
  
  logoWrapper: {
    // Ensure logo stays visual even if container scales
  }, 
  successOuter: {
    width: 250,
    height: 250,
    justifyContent: "center",
    alignItems: "center",
    marginBottom: 40,
  },
  successRing: {
    position: "absolute",
    borderRadius: 999,
    borderWidth: 2,
    borderColor: "#EFEFEF",
  },
  successCheck: {
    width: 100,
    height: 100,
    borderRadius: 50,
    borderWidth: 4,
    borderColor: "#0E4A47",
    justifyContent: "center",
    alignItems: "center",
    backgroundColor: COLORS.white,
  },
  title: {
    fontSize: 20,
    fontFamily: "Outfit_700Bold",
    color: COLORS.black,
    marginBottom: 8,
    textAlign: "center",
  },
  subtitle: {
    fontSize: 14,
    fontFamily: "Outfit_400Regular",
    color: "#999",
    textAlign: "center",
  },
  footer: {
    padding: 20,
    paddingBottom: 40,
  },
  button: {
    backgroundColor: "#1A4B4A",
    borderRadius: 100,
    height: 60,
  },
});
