import React, { useState, useEffect } from "react";
import {
  View,
  Text,
  StyleSheet,
  TouchableOpacity,
  TextInput,
  Keyboard,
  Platform,
} from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import { Stack, useRouter } from "expo-router";
import { COLORS } from "../../src/constants/colors";
import { Button } from "../../src/components/Button";
import { Ionicons } from "@expo/vector-icons";

export default function ReturningUserScreen() {
  const router = useRouter();
  const [secretKey, setSecretKey] = useState("");
  const [isKeyboardVisible, setKeyboardVisible] = useState(false);

  useEffect(() => {
    const keyboardDidShowListener = Keyboard.addListener(
      Platform.OS === "ios" ? "keyboardWillShow" : "keyboardDidShow",
      () => {
        setKeyboardVisible(true);
      }
    );
    const keyboardDidHideListener = Keyboard.addListener(
      Platform.OS === "ios" ? "keyboardWillHide" : "keyboardDidHide",
      () => {
        setKeyboardVisible(false);
      }
    );

    return () => {
      keyboardDidHideListener.remove();
      keyboardDidShowListener.remove();
    };
  }, []);

  const handleRecover = () => {
    if (secretKey.length > 0) {
      // Logic for recovery would go here
      // Navigate to recovery status
      router.push("/returning-user/status");
    }
  };

  const dismissKeyboard = () => {
    Keyboard.dismiss();
  };

  return (
    <SafeAreaView style={styles.container}>
      <Stack.Screen options={{ headerShown: false }} />

      <View style={styles.header}>
        <TouchableOpacity
          style={styles.backButton}
          onPress={() => router.back()}
        >
          <Ionicons name="arrow-back" size={24} color={COLORS.black} />
        </TouchableOpacity>
        <Text style={styles.headerTitle}>Enter Your Secret Key</Text>
        <View style={{ width: 40 }} />
      </View>

      <View style={styles.content}>
        <Text style={styles.description}>
          Input your secret key to recover your account
        </Text>

        <View style={styles.inputCard}>
          <Text style={styles.inputLabel}>Enter Your Secret Key Here</Text>
          <TextInput
            style={styles.input}
            value={secretKey}
            onChangeText={setSecretKey}
            placeholder="SCZANGBA5YHTNYVVV33H6MNWUQY7CZJM2ZCQMFRPY2DXNYBM6BKMB5M7"
            placeholderTextColor="#999"
            multiline
            numberOfLines={3}
            textAlignVertical="top"
            autoCapitalize="none"
            autoCorrect={false}
            returnKeyType="default" 
            blurOnSubmit={false} // Needed for multiline to not submit on return
          />
        </View>

        {isKeyboardVisible && (
          <TouchableOpacity
            style={styles.dismissButton}
            onPress={dismissKeyboard}
            activeOpacity={0.8}
          >
            <Text style={styles.dismissButtonText}>Done</Text>
          </TouchableOpacity>
        )}
      </View>

      <View style={styles.footer}>
        <Button
          title="Recover Account"
          onPress={handleRecover}
          variant="primary"
          style={styles.button}
          disabled={secretKey.length === 0}
        />
      </View>
    </SafeAreaView>
  );
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
    backgroundColor: COLORS.white,
  },
  header: {
    flexDirection: "row",
    alignItems: "center",
    justifyContent: "space-between",
    paddingHorizontal: 20,
    paddingVertical: 15,
  },
  backButton: {
    padding: 8,
  },
  headerTitle: {
    fontSize: 20,
    fontFamily: "Outfit_700Bold",
    color: COLORS.black,
  },
  content: {
    flex: 1,
    paddingHorizontal: 20,
    paddingTop: 20,
  },
  description: {
    fontSize: 16,
    color: "#666",
    marginBottom: 24,
    fontFamily: "Outfit_400Regular",
  },
  inputCard: {
    backgroundColor: COLORS.white,
    borderRadius: 20,
    padding: 24,
    borderWidth: 1,
    borderColor: "#F0F0F0",
    shadowColor: "#000",
    shadowOffset: { width: 0, height: 2 },
    shadowOpacity: 0.05,
    shadowRadius: 10,
    elevation: 2,
  },
  inputLabel: {
    fontSize: 16,
    fontFamily: "Outfit_600SemiBold",
    color: COLORS.black,
    marginBottom: 12,
  },
  input: {
    fontFamily: "Outfit_400Regular",
    fontSize: 15,
    color: COLORS.black,
    lineHeight: 22,
    minHeight: 80,
  },
  dismissButton: {
    alignSelf: "flex-end",
    marginTop: 10,
    backgroundColor: "#E0E0E0",
    paddingVertical: 8,
    paddingHorizontal: 16,
    borderRadius: 8,
  },
  dismissButtonText: {
    fontFamily: "Outfit_600SemiBold",
    color: COLORS.black,
    fontSize: 14,
  },
  footer: {
    padding: 20,
    paddingBottom: 40,
  },
  button: {
    backgroundColor: "#1A4B4A", // Dark green/teal from screenshot
    borderRadius: 100,
    height: 60,
  },
});
