import React from "react";
import { View, Text, StyleSheet } from "react-native";
import { Ionicons } from "@expo/vector-icons";
import { COLORS } from "../constants/colors";
import { Avatar } from "./Avatar";
import { VerificationBadge } from "./VerificationBadge";
import type { ZapsUser } from "../types/user";

interface TransferConfirmationCardProps {
  recipient: ZapsUser;
  amount: string;
  tokenSymbol: string;
  description?: string;
}

export const TransferConfirmationCard = React.memo(
  function TransferConfirmationCard({
    recipient,
    amount,
    tokenSymbol,
    description,
  }: TransferConfirmationCardProps) {
    return (
      <View style={styles.card}>
        <Avatar uri={recipient.avatar_url} name={recipient.username} size={80} />

        <View style={styles.nameRow}>
          <Text style={styles.username}>{recipient.username}</Text>
          {recipient.isVerified && <VerificationBadge />}
        </View>

        {recipient.address && (
          <Text style={styles.address} numberOfLines={1}>
            {recipient.address.slice(0, 10)}...{recipient.address.slice(-6)}
          </Text>
        )}

        <View style={styles.divider} />

        <View style={styles.detailsSection}>
          <View style={styles.detailRow}>
            <View style={styles.detailIcon}>
              <Ionicons name="cash-outline" size={18} color="#777" />
            </View>
            <View style={styles.detailCol}>
              <Text style={styles.detailLabel}>Amount</Text>
              <Text style={styles.detailValue}>
                {amount} {tokenSymbol}
              </Text>
            </View>
          </View>

          <View style={[styles.detailRow, { marginTop: 16 }]}>
            <View style={styles.detailIcon}>
              <Ionicons name="chatbubble-outline" size={18} color="#777" />
            </View>
            <View style={styles.detailCol}>
              <Text style={styles.detailLabel}>Note</Text>
              <Text style={styles.detailValue}>{description || "No note"}</Text>
            </View>
          </View>
        </View>
      </View>
    );
  }
);

const styles = StyleSheet.create({
  card: {
    backgroundColor: COLORS.white,
    borderRadius: 24,
    padding: 24,
    borderWidth: 1,
    borderColor: "#F0F0F0",
    alignItems: "center",
  },
  nameRow: {
    flexDirection: "row",
    alignItems: "center",
    marginTop: 16,
  },
  username: {
    fontSize: 20,
    fontFamily: "Outfit_700Bold",
    color: COLORS.black,
  },
  address: {
    fontSize: 13,
    fontFamily: "Outfit_400Regular",
    color: "#999",
    marginTop: 4,
  },
  divider: {
    height: 1,
    backgroundColor: "#F0F0F0",
    width: "100%",
    marginVertical: 20,
  },
  detailsSection: {
    width: "100%",
  },
  detailRow: {
    flexDirection: "row",
    alignItems: "center",
  },
  detailIcon: {
    width: 36,
    height: 36,
    borderRadius: 18,
    backgroundColor: "#F5F5F5",
    justifyContent: "center",
    alignItems: "center",
    marginRight: 12,
  },
  detailCol: {
    flex: 1,
  },
  detailLabel: {
    fontSize: 12,
    fontFamily: "Outfit_400Regular",
    color: "#999",
  },
  detailValue: {
    fontSize: 15,
    fontFamily: "Outfit_600SemiBold",
    color: COLORS.black,
    marginTop: 2,
  },
});
