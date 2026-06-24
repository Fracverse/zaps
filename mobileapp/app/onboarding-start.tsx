import React, { useState, useEffect } from "react";
import {
  View,
  Text,
  StyleSheet,
  TouchableOpacity,
  ScrollView,
  Image,
  Alert,
  Platform,
  TextInput,
} from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import { Stack, useRouter } from "expo-router";
import * as ImagePicker from "expo-image-picker";
import { COLORS } from "../src/constants/colors";
import { Button } from "../src/components/Button";
import { Input } from "../src/components/Input";
import { Ionicons } from "@expo/vector-icons";
import { fetchWithRetry } from "../src/utils/retry";
import AsyncStorage from "@react-native-async-storage/async-storage";

const API_BASE = process.env.EXPO_PUBLIC_API_URL || "http://localhost:8080";
const PROFILE_CACHE_KEY = "user_profile";

interface UserProfile {
  displayName: string;
  bio: string;
  avatarUri: string | null;
}

export default function ProfileSetupScreen() {
  const router = useRouter();
  const [displayName, setDisplayName] = useState("");
  const [bio, setBio] = useState("");
  const [avatarUri, setAvatarUri] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [loadingProfile, setLoadingProfile] = useState(true);

  useEffect(() => {
    (async () => {
      try {
        const cached = await AsyncStorage.getItem(PROFILE_CACHE_KEY);
        if (cached) {
          const profile: UserProfile = JSON.parse(cached);
          setDisplayName(profile.displayName || "");
          setBio(profile.bio || "");
          setAvatarUri(profile.avatarUri || null);
        }
      } finally {
        setLoadingProfile(false);
      }
    })();
  }, []);

  const handlePickAvatar = async () => {
    const permission = await ImagePicker.requestMediaLibraryPermissionsAsync();
    if (!permission.granted) {
      Alert.alert(
        "Permission Required",
        "Camera roll permission is needed to select an avatar."
      );
      return;
    }

    const result = await ImagePicker.launchImageLibraryAsync({
      mediaTypes: ["images"],
      allowsEditing: true,
      aspect: [1, 1],
      quality: 0.7,
    });

    if (!result.canceled && result.assets[0]) {
      setAvatarUri(result.assets[0].uri);
    }
  };

  const handleSave = async () => {
    if (!displayName.trim()) {
      Alert.alert("Required", "Display name is required.");
      return;
    }

    setSaving(true);
    try {
      let finalAvatarUri = avatarUri;

      if (avatarUri && avatarUri.startsWith("file://")) {
        const formData = new FormData();
        const filename = avatarUri.split("/").pop() || "avatar.jpg";
        formData.append("avatar", {
          uri: avatarUri,
          name: filename,
          type: "image/jpeg",
        } as any);

        const uploadRes = await fetchWithRetry(
          `${API_BASE}/api/profile/avatar`,
          {
            method: "POST",
            body: formData,
            headers: {},
          }
        );
        const uploadData = await uploadRes.json();
        finalAvatarUri = uploadData.url || avatarUri;
      }

      await fetchWithRetry(`${API_BASE}/api/profile`, {
        method: "PUT",
        body: JSON.stringify({
          displayName: displayName.trim(),
          bio: bio.trim(),
          avatarUri: finalAvatarUri,
        }),
      });

      const profile: UserProfile = {
        displayName: displayName.trim(),
        bio: bio.trim(),
        avatarUri: finalAvatarUri,
      };
      await AsyncStorage.setItem(PROFILE_CACHE_KEY, JSON.stringify(profile));

      if (!loadingProfile) {
        router.replace("/(personal)/home");
      }
    } catch {
      Alert.alert("Error", "Failed to save profile. Please try again.");
    } finally {
      setSaving(false);
    }
  };

  const handleSkip = () => {
    router.replace("/(personal)/home");
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
        <Text style={styles.headerTitle}>Set Up Profile</Text>
        <TouchableOpacity onPress={handleSkip}>
          <Text style={styles.skipText}>Skip</Text>
        </TouchableOpacity>
      </View>

      <ScrollView
        contentContainerStyle={styles.scrollContent}
        showsVerticalScrollIndicator={false}
      >
        <Text style={styles.subtitle}>
          Customize your profile so others can find and recognize you on Zaps.
        </Text>

        <View style={styles.avatarSection}>
          <TouchableOpacity
            style={styles.avatarContainer}
            onPress={handlePickAvatar}
          >
            {avatarUri ? (
              <Image source={{ uri: avatarUri }} style={styles.avatarImage} />
            ) : (
              <View style={styles.avatarPlaceholder}>
                <Ionicons
                  name="person-outline"
                  size={40}
                  color={COLORS.secondary}
                />
              </View>
            )}
            <View style={styles.cameraBadge}>
              <Ionicons name="camera" size={14} color={COLORS.secondary} />
            </View>
          </TouchableOpacity>
          <Text style={styles.avatarHint}>Tap to change photo</Text>
        </View>

        <View style={styles.formSection}>
          <Input
            label="Display Name"
            placeholder="Enter your display name"
            value={displayName}
            onChangeText={setDisplayName}
            maxLength={50}
            accessibilityHint="Your display name shown to other users"
          />

          <View style={styles.bioContainer}>
            <Text style={styles.bioLabel}>Bio</Text>
            <View style={styles.bioInputWrapper}>
              <TextInput
                style={styles.bioInput}
                placeholder="Tell others about yourself (optional)"
                placeholderTextColor="#999"
                value={bio}
                onChangeText={setBio}
                multiline
                numberOfLines={3}
                maxLength={160}
                accessibilityLabel="Bio"
                accessibilityHint="Short description about yourself"
              />
              <Text style={styles.charCount}>{bio.length}/160</Text>
            </View>
          </View>
        </View>
      </ScrollView>

      <View style={styles.footer}>
        <Button
          title={saving ? "Saving..." : "Save Profile"}
          onPress={handleSave}
          loading={saving}
          disabled={!displayName.trim() || saving}
          style={styles.saveButton}
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
    paddingVertical: 10,
  },
  backButton: {
    padding: 8,
  },
  headerTitle: {
    fontSize: 20,
    fontFamily: "Outfit_700Bold",
    color: COLORS.black,
  },
  skipText: {
    fontSize: 15,
    fontFamily: "Outfit_500Medium",
    color: "#999",
    padding: 8,
  },
  scrollContent: {
    paddingHorizontal: 20,
    paddingTop: 10,
    paddingBottom: 40,
    flexGrow: 1,
  },
  subtitle: {
    fontSize: 16,
    color: "#666",
    marginBottom: 32,
    fontFamily: "Outfit_500Medium",
    lineHeight: 22,
  },
  avatarSection: {
    alignItems: "center",
    marginBottom: 32,
  },
  avatarContainer: {
    position: "relative",
    marginBottom: 8,
  },
  avatarImage: {
    width: 100,
    height: 100,
    borderRadius: 50,
  },
  avatarPlaceholder: {
    width: 100,
    height: 100,
    borderRadius: 50,
    backgroundColor: COLORS.primary,
    justifyContent: "center",
    alignItems: "center",
  },
  cameraBadge: {
    position: "absolute",
    bottom: 0,
    right: 0,
    width: 32,
    height: 32,
    borderRadius: 16,
    backgroundColor: COLORS.primary,
    justifyContent: "center",
    alignItems: "center",
    borderWidth: 3,
    borderColor: COLORS.white,
  },
  avatarHint: {
    fontSize: 13,
    color: "#999",
    fontFamily: "Outfit_400Regular",
  },
  formSection: {
    gap: 4,
  },
  bioContainer: {
    marginBottom: 16,
  },
  bioLabel: {
    fontSize: 16,
    fontFamily: "Outfit_600SemiBold",
    color: "#000",
    marginBottom: 8,
  },
  bioInputWrapper: {
    position: "relative",
  },
  bioInput: {
    minHeight: 80,
    backgroundColor: "#FFFFFF",
    borderRadius: 16,
    paddingHorizontal: 16,
    paddingTop: 14,
    fontSize: 16,
    fontFamily: "Outfit_400Regular",
    color: COLORS.black,
    borderWidth: 1,
    borderColor: "#eee",
    textAlignVertical: "top",
  },
  charCount: {
    position: "absolute",
    bottom: 8,
    right: 12,
    fontSize: 11,
    color: "#bbb",
    fontFamily: "Outfit_400Regular",
  },
  footer: {
    padding: 20,
    paddingBottom: Platform.OS === "ios" ? 40 : 20,
  },
  saveButton: {
    height: 56,
    borderRadius: 28,
  },
});
