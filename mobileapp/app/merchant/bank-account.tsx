import React, { useState } from "react";
import {
  View,
  Text,
  StyleSheet,
  TouchableOpacity,
  SafeAreaView,
  KeyboardAvoidingView,
  Platform,
} from "react-native";
import { Ionicons } from "@expo/vector-icons";
import { COLORS } from "../../src/constants/colors";
import { useRouter } from "expo-router";
import { Input } from "../../src/components/Input";

export default function BankAccountScreen() {
  const router = useRouter();
  const [selectedBank] = useState("");
  const [name, setName] = useState("");
  const [accountNumber, setAccountNumber] = useState("");

  return (
    <SafeAreaView style={styles.container}>
      <View style={styles.header}>
        <TouchableOpacity
          onPress={() => router.back()}
          style={styles.backButton}
        >
          <Ionicons name="arrow-back" size={24} color={COLORS.black} />
        </TouchableOpacity>
        <Text style={styles.headerTitle}>Bank Account</Text>
        <View style={{ width: 24 }} />
      </View>

      <KeyboardAvoidingView
        behavior={Platform.OS === "ios" ? "padding" : "height"}
        style={styles.content}
      >
        <View style={styles.formContainer}>
          <TouchableOpacity style={styles.selectField}>
            <Text
              style={[
                styles.selectText,
                !selectedBank && styles.placeholderText,
              ]}
            >
              {selectedBank || "Select bank"}
            </Text>
            <Ionicons name="chevron-down" size={20} color={COLORS.black} />
          </TouchableOpacity>

          <Input
            placeholder="Enter your name"
            value={name}
            onChangeText={setName}
          />

          <Input
            placeholder="Enter account number"
            value={accountNumber}
            onChangeText={setAccountNumber}
            keyboardType="numeric"
          />
        </View>

        <View style={styles.bottomContainer}>
          <TouchableOpacity style={styles.linkedButton} activeOpacity={0.8}>
            <Text style={styles.linkedButtonText}>Linked</Text>
          </TouchableOpacity>
        </View>
      </KeyboardAvoidingView>
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
    padding: 5,
  },
  headerTitle: {
    fontSize: 20,
    fontFamily: "Outfit_700Bold",
    color: COLORS.black,
  },
  content: {
    flex: 1,
    justifyContent: "space-between",
  },
  formContainer: {
    paddingHorizontal: 20,
    paddingTop: 30,
    gap: 16,
  },
  selectField: {
    flexDirection: "row",
    alignItems: "center",
    justifyContent: "space-between",
    paddingHorizontal: 24,
    paddingVertical: 18,
    borderRadius: 100,
    borderWidth: 1,
    borderColor: "#E0E0E0",
    backgroundColor: COLORS.white,
  },
  selectText: {
    fontSize: 16,
    fontFamily: "Outfit_500Medium",
    color: COLORS.black,
  },
  placeholderText: {
    color: "#999",
  },

  bottomContainer: {
    paddingHorizontal: 20,
    paddingBottom: 30,
  },
  linkedButton: {
    width: "100%",
    height: 56,
    backgroundColor: COLORS.primary,
    borderRadius: 100,
    justifyContent: "center",
    alignItems: "center",
  },
  linkedButtonText: {
    fontSize: 18,
    fontFamily: "Outfit_600SemiBold",
    color: COLORS.secondary,
  },
});
