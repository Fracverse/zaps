import React, { useState } from "react";
import { View, Text, StyleSheet, ScrollView, Alert } from "react-native";
import { useTranslation } from "react-i18next";
import { Button } from "./Button";
import { LanguageSelector } from "./LanguageSelector";
import { safeAsyncCall, fetchWithRetry } from "../utils/retry";
import {
  formatCurrency,
  formatDate,
  formatRelativeDate,
} from "../utils/formatting";
import { COLORS } from "../constants/colors";

// Import global toast type
import "./Toast";

export const ErrorHandlingDemo: React.FC = () => {
  const { t, i18n } = useTranslation();
  const [isLoading, setIsLoading] = useState(false);

  const testSuccessToast = () => {
    if (typeof globalThis !== "undefined" && (globalThis as any).toast) {
      (globalThis as any).toast.success(t("transactions.success"));
    }
  };

  const testErrorToast = () => {
    if (typeof globalThis !== "undefined" && (globalThis as any).toast) {
      (globalThis as any).toast.error(t("errors.generic"), {
        label: t("common.retry"),
        onPress: () => {
          testSuccessToast();
        },
      });
    }
  };

  const testNetworkError = async () => {
    setIsLoading(true);
    await safeAsyncCall(
      async () => {
        // Simulate a network request that might fail
        const response = await fetchWithRetry(
          "https://jsonplaceholder.typicode.com/posts/1",
          {},
          {
            maxRetries: 2,
            onRetry: (attempt, error) => {
              console.log(`Retry attempt ${attempt}:`, error);
            },
          }
        );
        return response.json();
      },
      {
        successMessage: t("common.success"),
        errorMessage: t("errors.network"),
        showLoading: true,
      }
    );
    setIsLoading(false);
  };

  const testCrashError = () => {
    throw new Error("This is a test crash to demonstrate error boundary");
  };

  const testFormatting = () => {
    const amount = 1234.56;
    const date = new Date();

    Alert.alert(
      "Formatting Demo",
      `Currency: ${formatCurrency(amount, "USD")}\n` +
        `Date: ${formatDate(date)}\n` +
        `Relative: ${formatRelativeDate(date)}\n` +
        `Language: ${i18n.language}`
    );
  };

  return (
    <ScrollView style={styles.container}>
      <Text style={styles.title}>Error Handling & I18n Demo</Text>

      <View style={styles.section}>
        <Text style={styles.sectionTitle}>Toast Notifications</Text>
        <Button
          title="Show Success Toast"
          onPress={testSuccessToast}
          style={styles.button}
        />
        <Button
          title="Show Error Toast with Retry"
          onPress={testErrorToast}
          variant="outline"
          style={styles.button}
        />
      </View>

      <View style={styles.section}>
        <Text style={styles.sectionTitle}>Network Handling</Text>
        <Button
          title="Test Network Request"
          onPress={testNetworkError}
          loading={isLoading}
          style={styles.button}
        />
      </View>

      <View style={styles.section}>
        <Text style={styles.sectionTitle}>Localization</Text>
        <LanguageSelector />
        <Button
          title="Test Formatting"
          onPress={testFormatting}
          variant="secondary"
          style={styles.button}
        />
      </View>

      <View style={styles.section}>
        <Text style={styles.sectionTitle}>Error Boundary</Text>
        <Button
          title="Test App Crash (Will be caught)"
          onPress={testCrashError}
          variant="ghost"
          style={styles.button}
        />
        <Text style={styles.warning}>
          This will trigger an error to demonstrate the error boundary. The app
          won't crash.
        </Text>
      </View>

      <View style={styles.section}>
        <Text style={styles.sectionTitle}>Current Language Info</Text>
        <Text style={styles.info}>Language: {i18n.language}</Text>
        <Text style={styles.info}>Direction: {i18n.dir()}</Text>
        <Text style={styles.info}>Sample Text: {t("common.loading")}</Text>
      </View>
    </ScrollView>
  );
};

const styles = StyleSheet.create({
  container: {
    flex: 1,
    padding: 16,
    backgroundColor: COLORS.white,
  },
  title: {
    fontSize: 24,
    fontFamily: "Outfit_700Bold",
    color: COLORS.text,
    marginBottom: 24,
    textAlign: "center",
  },
  section: {
    marginBottom: 32,
  },
  sectionTitle: {
    fontSize: 18,
    fontFamily: "Outfit_600SemiBold",
    color: COLORS.text,
    marginBottom: 16,
  },
  button: {
    marginBottom: 12,
  },
  warning: {
    fontSize: 12,
    fontFamily: "Outfit_400Regular",
    color: COLORS.darkGray,
    fontStyle: "italic",
    marginTop: 8,
    textAlign: "center",
  },
  info: {
    fontSize: 14,
    fontFamily: "Outfit_400Regular",
    color: COLORS.darkGray,
    marginBottom: 4,
  },
});
