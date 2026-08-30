import React from "react";
import {
  View,
  Text,
  StyleSheet,
  TouchableOpacity,
  ScrollView,
} from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import { Ionicons } from "@expo/vector-icons";
import { COLORS } from "../src/constants/colors";
import { useRouter } from "expo-router";
import { useThemeContext } from "../src/contexts/ThemeProvider";
import type { ThemePreference } from "../src/contexts/ThemeProvider";

const appearanceOptions: {
  id: ThemePreference;
  label: string;
  description: string;
  icon: keyof typeof Ionicons.glyphMap;
}[] = [
  {
    id: "system",
    label: "System",
    description: "Follow your device's light or dark setting",
    icon: "phone-portrait-outline",
  },
  {
    id: "light",
    label: "Light",
    description: "Always use the light theme",
    icon: "sunny-outline",
  },
  {
    id: "dark",
    label: "Dark",
    description: "Always use the dark theme",
    icon: "moon-outline",
  },
  {
    id: "high-contrast",
    label: "High Contrast Dark",
    description: "Dark theme with maximum contrast for readability",
    icon: "contrast-outline",
  },
];

export default function AppearanceScreen() {
  const router = useRouter();
  const { preference, setPreference } = useThemeContext();

  return (
    <SafeAreaView style={styles.container}>
      <View style={styles.header}>
        <TouchableOpacity
          onPress={() => router.back()}
          style={styles.backButton}
        >
          <Ionicons name="arrow-back" size={24} color={COLORS.black} />
        </TouchableOpacity>
        <Text style={styles.headerTitle}>Appearance</Text>
        <View style={{ width: 24 }} />
      </View>

      <ScrollView contentContainerStyle={styles.scrollContent}>
        <Text style={styles.sectionHint}>
          Choose how the app looks. Your selection is saved on this device and
          applies as you navigate.
        </Text>

        <View style={styles.optionsList}>
          {appearanceOptions.map((option) => {
            const isSelected = preference === option.id;
            return (
              <TouchableOpacity
                key={option.id}
                style={[
                  styles.optionItem,
                  isSelected && styles.optionItemSelected,
                ]}
                onPress={() => setPreference(option.id)}
                activeOpacity={0.7}
                accessibilityRole="radio"
                accessibilityState={{ selected: isSelected }}
                accessibilityLabel={`${option.label} theme`}
              >
                <View style={styles.optionContent}>
                  <View
                    style={[
                      styles.optionIcon,
                      isSelected && styles.optionIconSelected,
                    ]}
                  >
                    <Ionicons
                      name={option.icon}
                      size={22}
                      color={isSelected ? COLORS.secondary : COLORS.primary}
                    />
                  </View>
                  <View style={styles.optionText}>
                    <Text
                      style={[
                        styles.optionLabel,
                        isSelected && styles.optionLabelSelected,
                      ]}
                    >
                      {option.label}
                    </Text>
                    <Text style={styles.optionDescription}>
                      {option.description}
                    </Text>
                  </View>
                </View>
                {isSelected ? (
                  <View style={styles.radioSelected}>
                    <View style={styles.radioInner} />
                  </View>
                ) : (
                  <View style={styles.radioUnselected} />
                )}
              </TouchableOpacity>
            );
          })}
        </View>
      </ScrollView>
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
  scrollContent: {
    paddingHorizontal: 20,
    paddingTop: 10,
    paddingBottom: 30,
  },
  sectionHint: {
    fontSize: 13,
    fontFamily: "Outfit_400Regular",
    color: "#999",
    lineHeight: 20,
    marginBottom: 20,
  },
  optionsList: {
    gap: 12,
  },
  optionItem: {
    flexDirection: "row",
    alignItems: "center",
    justifyContent: "space-between",
    padding: 16,
    borderRadius: 16,
    backgroundColor: "#F9F9F9",
    borderWidth: 1,
    borderColor: "#F0F0F0",
  },
  optionItemSelected: {
    backgroundColor: COLORS.white,
    borderColor: COLORS.primary,
    borderWidth: 2,
  },
  optionContent: {
    flexDirection: "row",
    alignItems: "center",
    flex: 1,
    marginRight: 12,
  },
  optionIcon: {
    width: 44,
    height: 44,
    borderRadius: 22,
    backgroundColor: "#F0F7F6",
    justifyContent: "center",
    alignItems: "center",
    marginRight: 14,
  },
  optionIconSelected: {
    backgroundColor: COLORS.primary,
  },
  optionText: {
    flex: 1,
  },
  optionLabel: {
    fontSize: 16,
    fontFamily: "Outfit_500Medium",
    color: COLORS.black,
    marginBottom: 2,
  },
  optionLabelSelected: {
    fontFamily: "Outfit_700Bold",
    color: COLORS.primary,
  },
  optionDescription: {
    fontSize: 13,
    fontFamily: "Outfit_400Regular",
    color: "#999",
    lineHeight: 18,
  },
  radioUnselected: {
    width: 24,
    height: 24,
    borderRadius: 12,
    borderWidth: 2,
    borderColor: "#E0E0E0",
  },
  radioSelected: {
    width: 24,
    height: 24,
    borderRadius: 12,
    borderWidth: 2,
    borderColor: COLORS.primary,
    justifyContent: "center",
    alignItems: "center",
  },
  radioInner: {
    width: 12,
    height: 12,
    borderRadius: 6,
    backgroundColor: COLORS.primary,
  },
});
