/**
 * Avatar.tsx
 *
 * Displays a user's profile picture (or initials fallback).
 *
 * When `editable` is true the component becomes tappable and opens the
 * device photo library via expo-image-picker.  The selected image is
 * compressed client-side, uploaded to the IPFS/S3 backend proxy, and the
 * resulting URI is passed back through `onAvatarUploaded`.
 *
 * Backward-compatible: all existing usages continue to render read-only
 * avatars without any changes.
 */

import React, { useState, useCallback } from "react";
import {
  View,
  Text,
  Image,
  StyleSheet,
  TouchableOpacity,
  ActivityIndicator,
  Alert,
} from "react-native";
import * as ImagePicker from "expo-image-picker";
import { Ionicons } from "@expo/vector-icons";
import { COLORS } from "../constants/colors";
import { uploadAvatar } from "../services/avatarService";

// ── Types ─────────────────────────────────────────────────────────────────────

interface AvatarProps {
  uri?: string | null;
  name: string;
  size?: number;
  /** Enable image-picker mode. Makes the avatar tappable. */
  editable?: boolean;
  /** Bearer token used for the upload & profile-update calls. */
  authToken?: string;
  /** Called with the new IPFS/S3 URI after a successful upload. */
  onAvatarUploaded?: (newUri: string) => void;
}

// ── Component ─────────────────────────────────────────────────────────────────

export const Avatar = React.memo(function Avatar({
  uri,
  name,
  size = 64,
  editable = false,
  authToken,
  onAvatarUploaded,
}: AvatarProps) {
  const [uploading, setUploading] = useState(false);

  const initials = name
    .split(/[\s._-]+/)
    .filter(Boolean)
    .slice(0, 2)
    .map((w) => w.charAt(0).toUpperCase())
    .join("");

  const fontSize = Math.round(size * 0.38);

  // ── Image picker handler ────────────────────────────────────────────────

  const pickImage = useCallback(async () => {
    if (uploading) return;

    // Request photo-library permission (Android needs this at runtime).
    const { status } =
      await ImagePicker.requestMediaLibraryPermissionsAsync();
    if (status !== "granted") {
      Alert.alert(
        "Permission required",
        "Please grant photo library access to change your avatar."
      );
      return;
    }

    const result = await ImagePicker.launchImageLibraryAsync({
      mediaTypes: ["images"],
      allowsEditing: true,
      aspect: [1, 1],
      quality: 0.7, // Client-side JPEG compression (~70 % quality)
    });

    if (result.canceled || !result.assets?.[0]?.uri) return;

    setUploading(true);
    try {
      const { avatarUrl } = await uploadAvatar(
        result.assets[0].uri,
        authToken
      );
      onAvatarUploaded?.(avatarUrl);
    } catch (err: any) {
      const message =
        err?.message ?? "Something went wrong while uploading your photo.";
      Alert.alert("Upload failed", message);
    } finally {
      setUploading(false);
    }
  }, [uploading, authToken, onAvatarUploaded]);

  // ── Inner content (shared between editable & read-only modes) ───────────

  const content =
    uri && !uploading ? (
      <Image
        source={{ uri }}
        style={[
          styles.image,
          { width: size, height: size, borderRadius: size / 2 },
        ]}
        accessibilityLabel={`Avatar for ${name}`}
      />
    ) : (
      <View
        style={[
          styles.fallback,
          { width: size, height: size, borderRadius: size / 2 },
        ]}
        accessibilityLabel={`Avatar placeholder for ${name}`}
      >
        {uploading ? (
          <ActivityIndicator
            size="small"
            color={COLORS.secondary}
            accessibilityLabel="Uploading avatar"
          />
        ) : (
          <Text style={[styles.initials, { fontSize }]}>
            {initials || "?"}
          </Text>
        )}
      </View>
    );

  // ── Read-only mode ──────────────────────────────────────────────────────

  if (!editable) {
    return content;
  }

  // ── Editable mode ───────────────────────────────────────────────────────

  const editIconSize = Math.max(18, Math.round(size * 0.28));

  return (
    <TouchableOpacity
      onPress={pickImage}
      activeOpacity={0.7}
      disabled={uploading}
      accessibilityRole="button"
      accessibilityLabel={`Change avatar for ${name}`}
      accessibilityHint="Opens photo library to select a new profile picture"
      style={[
        styles.editableContainer,
        { width: size, height: size },
      ]}
    >
      {content}

      {/* Camera badge overlay */}
      <View
        style={[
          styles.editBadge,
          {
            width: editIconSize + 10,
            height: editIconSize + 10,
            borderRadius: (editIconSize + 10) / 2,
          },
        ]}
      >
        <Ionicons
          name="camera-outline"
          size={editIconSize}
          color={COLORS.white}
        />
      </View>
    </TouchableOpacity>
  );
});

// ── Styles ────────────────────────────────────────────────────────────────────

const styles = StyleSheet.create({
  image: {
    backgroundColor: COLORS.gray,
  },
  fallback: {
    backgroundColor: COLORS.primary,
    justifyContent: "center",
    alignItems: "center",
  },
  initials: {
    fontFamily: "Outfit_700Bold",
    color: COLORS.secondary,
  },
  editableContainer: {
    position: "relative",
  },
  editBadge: {
    position: "absolute",
    bottom: 0,
    right: 0,
    backgroundColor: COLORS.primary,
    justifyContent: "center",
    alignItems: "center",
    borderWidth: 2,
    borderColor: COLORS.white,
  },
});
