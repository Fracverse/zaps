import React, { useCallback, useEffect, useRef, useState } from "react";
import {
  ActivityIndicator,
  ScrollView,
  StyleSheet,
  Text,
  TouchableOpacity,
  View,
} from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import { Ionicons } from "@expo/vector-icons";
import { Stack, useLocalSearchParams, useRouter } from "expo-router";
import { Button } from "../../src/components/Button";
import { COLORS } from "../../src/constants/colors";
import {
  CircleServiceError,
  fetchCirclePreview,
  joinCircle,
} from "../../src/services/circleService";
import type { CirclePreview } from "../../src/types/circle";

function formatAmount(amount: string, currency: string): string {
  const groupedAmount = amount.replace(/\B(?=(\d{3})+(?!\d))/g, ",");
  return `${groupedAmount} ${currency}`;
}

function frequencyLabel(frequency: CirclePreview["contribution_frequency"]) {
  return frequency.toLowerCase();
}

function CirclePreviewScreen() {
  const router = useRouter();
  const { id } = useLocalSearchParams<{ id: string | string[] }>();
  const circleId = Array.isArray(id) ? id[0] : id;
  const requestId = useRef(0);
  const [circle, setCircle] = useState<CirclePreview | null>(null);
  const [loading, setLoading] = useState(true);
  const [joining, setJoining] = useState(false);
  const [error, setError] = useState("");
  const [unauthorized, setUnauthorized] = useState(false);
  const [joinError, setJoinError] = useState("");

  const loadCircle = useCallback(async () => {
    const currentRequestId = ++requestId.current;
    setError("");
    setUnauthorized(false);
    setJoinError("");

    if (!circleId) {
      setCircle(null);
      setError("This circle could not be found.");
      setLoading(false);
      return;
    }

    setLoading(true);
    try {
      const details = await fetchCirclePreview(circleId);
      if (currentRequestId === requestId.current) setCircle(details);
    } catch (loadError) {
      if (currentRequestId !== requestId.current) return;
      setCircle(null);
      setError(
        loadError instanceof CircleServiceError && loadError.statusCode === 404
          ? "This circle is unavailable or you do not have permission to view it."
          : loadError instanceof Error
            ? loadError.message
            : "Unable to load this circle right now."
      );
      setUnauthorized(
        loadError instanceof CircleServiceError && loadError.statusCode === 401
      );
    } finally {
      if (currentRequestId === requestId.current) setLoading(false);
    }
  }, [circleId]);

  useEffect(() => {
    void loadCircle();
    return () => {
      requestId.current += 1;
    };
  }, [loadCircle]);

  const handleJoin = async () => {
    if (!circle || !circle.can_join || joining) return;
    setJoining(true);
    setJoinError("");
    try {
      await joinCircle(circle.id);
      await loadCircle();
    } catch (joinFailure) {
      setJoinError(
        joinFailure instanceof Error
          ? joinFailure.message
          : "Unable to join this circle right now."
      );
      if (
        joinFailure instanceof CircleServiceError &&
        joinFailure.statusCode === 401
      ) {
        setCircle(null);
        setUnauthorized(true);
        setError("Sign in to view circle information.");
      }
    } finally {
      setJoining(false);
    }
  };

  if (loading) {
    return (
      <SafeAreaView style={styles.centered} edges={["top"]}>
        <Stack.Screen options={{ headerShown: false }} />
        <ActivityIndicator size="large" color={COLORS.primary} />
        <Text style={styles.loadingText}>Loading circle details...</Text>
      </SafeAreaView>
    );
  }

  if (error || !circle) {
    return (
      <SafeAreaView style={styles.centered} edges={["top"]}>
        <Stack.Screen options={{ headerShown: false }} />
        <Ionicons
          name={unauthorized ? "lock-closed-outline" : "alert-circle-outline"}
          size={44}
          color={COLORS.primary}
        />
        <Text style={styles.errorTitle}>
          {unauthorized ? "Sign in required" : "Circle unavailable"}
        </Text>
        <Text style={styles.errorText}>
          {unauthorized
            ? "Sign in to view circle information."
            : error || "This circle could not be found."}
        </Text>
        {unauthorized ? (
          <Button
            title="Sign in"
            onPress={() => router.replace("/")}
            style={styles.errorButton}
          />
        ) : (
          <Button
            title="Try again"
            onPress={() => void loadCircle()}
            style={styles.errorButton}
          />
        )}
        <TouchableOpacity
          accessibilityRole="button"
          accessibilityLabel="Go back"
          onPress={() => router.back()}
          style={styles.backLink}
        >
          <Text style={styles.backLinkText}>Go back</Text>
        </TouchableOpacity>
      </SafeAreaView>
    );
  }

  const progress = Math.min(
    100,
    Math.max(
      0,
      (Number(circle.progress_amount) / Number(circle.goal_amount)) * 100
    )
  );
  const memberCountLabel =
    circle.member_limit === null
      ? `${circle.member_count} members`
      : `${circle.member_count} of ${circle.member_limit} members`;

  return (
    <SafeAreaView style={styles.container} edges={["top"]}>
      <Stack.Screen options={{ headerShown: false }} />
      <View style={styles.header}>
        <TouchableOpacity
          accessibilityRole="button"
          accessibilityLabel="Go back"
          onPress={() => router.back()}
          style={styles.backButton}
        >
          <Ionicons name="arrow-back" size={22} color={COLORS.primary} />
        </TouchableOpacity>
        <Text style={styles.headerTitle}>Circle details</Text>
        <View style={styles.headerSpacer} />
      </View>

      <ScrollView
        contentContainerStyle={styles.content}
        showsVerticalScrollIndicator={false}
      >
        <View style={styles.hero}>
          <View style={styles.heroIcon}>
            <Ionicons name="people-outline" size={30} color={COLORS.primary} />
          </View>
          <View
            style={[
              styles.visibilityBadge,
              circle.visibility === "PRIVATE" && styles.privateBadge,
            ]}
          >
            <Ionicons
              name={
                circle.visibility === "PRIVATE"
                  ? "lock-closed-outline"
                  : "globe-outline"
              }
              size={14}
              color={
                circle.visibility === "PRIVATE" ? "#8A5A00" : COLORS.primary
              }
            />
            <Text
              style={[
                styles.visibilityText,
                circle.visibility === "PRIVATE" && styles.privateText,
              ]}
            >
              {circle.visibility === "PRIVATE" ? "Private" : "Public"}
            </Text>
          </View>
          <Text style={styles.circleName}>{circle.name}</Text>
          <Text style={styles.description}>{circle.description}</Text>
          <Text style={styles.ownerLine}>
            Organized by @{circle.owner_username}
          </Text>
        </View>

        <View style={styles.card}>
          <SectionTitle icon="flag-outline" title="Objective" />
          <Text style={styles.bodyText}>{circle.objective}</Text>
        </View>

        <View style={styles.card}>
          <SectionTitle icon="trending-up-outline" title="Current progress" />
          <View style={styles.amountRow}>
            <Text style={styles.progressAmount}>
              {formatAmount(circle.progress_amount, circle.currency)}
            </Text>
            <Text style={styles.goalAmount}>
              of {formatAmount(circle.goal_amount, circle.currency)}
            </Text>
          </View>
          <View
            accessibilityRole="progressbar"
            accessibilityLabel="Circle savings progress"
            accessibilityValue={{ min: 0, max: 100, now: progress }}
            style={styles.progressTrack}
          >
            <View style={[styles.progressFill, { width: `${progress}%` }]} />
          </View>
          <Text style={styles.progressCaption}>
            {progress.toFixed(0)}% of goal
          </Text>
        </View>

        <View style={styles.card}>
          <SectionTitle
            icon="wallet-outline"
            title="Contribution requirements"
          />
          <Text style={styles.requirementAmount}>
            {formatAmount(circle.contribution_amount, circle.currency)}
          </Text>
          <Text style={styles.mutedText}>
            Required {frequencyLabel(circle.contribution_frequency)}
          </Text>
          {circle.contribution_rules ? (
            <Text style={[styles.bodyText, styles.spacedText]}>
              {circle.contribution_rules}
            </Text>
          ) : null}
        </View>

        <View style={styles.card}>
          <SectionTitle icon="people-outline" title="Membership" />
          <InfoRow label="Members" value={memberCountLabel} />
          <InfoRow
            label="Visibility"
            value={circle.visibility === "PRIVATE" ? "Private" : "Public"}
          />
          {circle.membership_restrictions ? (
            <View style={styles.restrictions}>
              <Text style={styles.infoLabel}>Membership rules</Text>
              <Text style={styles.bodyText}>
                {circle.membership_restrictions}
              </Text>
            </View>
          ) : null}
        </View>

        {joinError ? (
          <View accessibilityRole="alert" style={styles.joinError}>
            <Ionicons name="alert-circle-outline" size={18} color="#B42318" />
            <Text style={styles.joinErrorText}>{joinError}</Text>
          </View>
        ) : null}

        {circle.is_member ? (
          <View style={styles.memberNotice}>
            <Ionicons name="checkmark-circle" size={20} color="#14804A" />
            <Text style={styles.memberNoticeText}>
              You are a member of this circle
            </Text>
          </View>
        ) : circle.visibility === "PUBLIC" && circle.can_join ? (
          <Button
            title="Join Circle"
            onPress={() => void handleJoin()}
            loading={joining}
            accessibilityHint="Join this public circle"
          />
        ) : circle.visibility === "PRIVATE" ? (
          <View style={styles.unavailableNotice}>
            <Ionicons name="lock-closed-outline" size={18} color="#666" />
            <Text style={styles.mutedText}>
              Private circles are membership-only.
            </Text>
          </View>
        ) : (
          <View style={styles.unavailableNotice}>
            <Ionicons
              name="information-circle-outline"
              size={18}
              color="#666"
            />
            <Text style={styles.mutedText}>
              {circle.join_unavailable_reason ||
                "This circle is not accepting members."}
            </Text>
          </View>
        )}
      </ScrollView>
    </SafeAreaView>
  );
}

function SectionTitle({
  icon,
  title,
}: {
  icon: keyof typeof Ionicons.glyphMap;
  title: string;
}) {
  return (
    <View style={styles.sectionTitle}>
      <Ionicons name={icon} size={19} color={COLORS.primary} />
      <Text style={styles.sectionHeading}>{title}</Text>
    </View>
  );
}

function InfoRow({ label, value }: { label: string; value: string }) {
  return (
    <View style={styles.infoRow}>
      <Text style={styles.infoLabel}>{label}</Text>
      <Text style={styles.infoValue}>{value}</Text>
    </View>
  );
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
    backgroundColor: "#F7F9F8",
  },
  centered: {
    flex: 1,
    backgroundColor: "#F7F9F8",
    justifyContent: "center",
    alignItems: "center",
    padding: 28,
  },
  loadingText: {
    marginTop: 14,
    color: "#666",
    fontFamily: "Outfit_400Regular",
    fontSize: 15,
  },
  errorTitle: {
    marginTop: 16,
    color: COLORS.primary,
    fontFamily: "Outfit_700Bold",
    fontSize: 21,
  },
  errorText: {
    color: "#666",
    fontFamily: "Outfit_400Regular",
    fontSize: 15,
    lineHeight: 22,
    textAlign: "center",
    marginTop: 8,
  },
  errorButton: {
    marginTop: 22,
  },
  backLink: {
    minHeight: 44,
    justifyContent: "center",
    paddingHorizontal: 16,
    marginTop: 8,
  },
  backLinkText: {
    color: COLORS.primary,
    fontFamily: "Outfit_500Medium",
    fontSize: 15,
  },
  header: {
    height: 56,
    paddingHorizontal: 16,
    flexDirection: "row",
    alignItems: "center",
    justifyContent: "space-between",
    backgroundColor: "#F7F9F8",
  },
  backButton: {
    width: 44,
    height: 44,
    justifyContent: "center",
  },
  headerTitle: {
    color: COLORS.primary,
    fontFamily: "Outfit_700Bold",
    fontSize: 17,
  },
  headerSpacer: {
    width: 44,
  },
  content: {
    paddingHorizontal: 20,
    paddingTop: 8,
    paddingBottom: 32,
    gap: 14,
  },
  hero: {
    alignItems: "center",
    paddingVertical: 20,
  },
  heroIcon: {
    width: 66,
    height: 66,
    borderRadius: 33,
    backgroundColor: "#E6F5E9",
    alignItems: "center",
    justifyContent: "center",
    marginBottom: 12,
  },
  visibilityBadge: {
    alignSelf: "center",
    flexDirection: "row",
    alignItems: "center",
    gap: 5,
    borderRadius: 20,
    backgroundColor: "#E6F5E9",
    paddingHorizontal: 11,
    paddingVertical: 5,
  },
  privateBadge: {
    backgroundColor: "#FFF1D6",
  },
  visibilityText: {
    color: COLORS.primary,
    fontFamily: "Outfit_500Medium",
    fontSize: 12,
  },
  privateText: {
    color: "#8A5A00",
  },
  circleName: {
    marginTop: 12,
    color: COLORS.primary,
    fontFamily: "Outfit_700Bold",
    fontSize: 26,
    textAlign: "center",
  },
  description: {
    marginTop: 8,
    color: "#555",
    fontFamily: "Outfit_400Regular",
    fontSize: 15,
    lineHeight: 22,
    textAlign: "center",
  },
  ownerLine: {
    marginTop: 10,
    color: "#777",
    fontFamily: "Outfit_400Regular",
    fontSize: 13,
  },
  card: {
    borderRadius: 16,
    backgroundColor: COLORS.white,
    padding: 18,
    borderWidth: 1,
    borderColor: "#E9EEEB",
  },
  sectionTitle: {
    flexDirection: "row",
    alignItems: "center",
    gap: 9,
    marginBottom: 13,
  },
  sectionHeading: {
    color: COLORS.primary,
    fontFamily: "Outfit_700Bold",
    fontSize: 16,
  },
  bodyText: {
    color: "#444",
    fontFamily: "Outfit_400Regular",
    fontSize: 14,
    lineHeight: 21,
  },
  amountRow: {
    flexDirection: "row",
    alignItems: "baseline",
    flexWrap: "wrap",
    gap: 6,
  },
  progressAmount: {
    color: COLORS.primary,
    fontFamily: "Outfit_700Bold",
    fontSize: 23,
  },
  goalAmount: {
    color: "#777",
    fontFamily: "Outfit_400Regular",
    fontSize: 14,
  },
  progressTrack: {
    height: 10,
    backgroundColor: "#E9EEEB",
    borderRadius: 5,
    overflow: "hidden",
    marginTop: 14,
  },
  progressFill: {
    height: "100%",
    backgroundColor: COLORS.secondary,
    borderRadius: 5,
  },
  progressCaption: {
    color: "#777",
    fontFamily: "Outfit_500Medium",
    fontSize: 12,
    marginTop: 8,
    textAlign: "right",
  },
  requirementAmount: {
    color: COLORS.primary,
    fontFamily: "Outfit_700Bold",
    fontSize: 22,
  },
  mutedText: {
    color: "#777",
    fontFamily: "Outfit_400Regular",
    fontSize: 13,
    lineHeight: 19,
  },
  spacedText: {
    marginTop: 12,
  },
  infoRow: {
    minHeight: 36,
    flexDirection: "row",
    alignItems: "center",
    justifyContent: "space-between",
    gap: 12,
  },
  infoLabel: {
    color: "#777",
    fontFamily: "Outfit_400Regular",
    fontSize: 13,
  },
  infoValue: {
    color: "#333",
    fontFamily: "Outfit_500Medium",
    fontSize: 14,
  },
  restrictions: {
    borderTopWidth: 1,
    borderTopColor: "#EEF1EF",
    marginTop: 8,
    paddingTop: 13,
    gap: 6,
  },
  joinError: {
    flexDirection: "row",
    alignItems: "center",
    gap: 8,
    borderRadius: 10,
    backgroundColor: "#FEF3F2",
    padding: 12,
  },
  joinErrorText: {
    flex: 1,
    color: "#B42318",
    fontFamily: "Outfit_400Regular",
    fontSize: 13,
  },
  memberNotice: {
    minHeight: 52,
    flexDirection: "row",
    justifyContent: "center",
    alignItems: "center",
    gap: 8,
    borderRadius: 28,
    backgroundColor: "#EAF7EF",
  },
  memberNoticeText: {
    color: "#14804A",
    fontFamily: "Outfit_700Bold",
    fontSize: 15,
  },
  unavailableNotice: {
    minHeight: 48,
    flexDirection: "row",
    justifyContent: "center",
    alignItems: "center",
    gap: 8,
    paddingHorizontal: 12,
  },
});

export default CirclePreviewScreen;
