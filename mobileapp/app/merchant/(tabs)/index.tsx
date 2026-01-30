import React from "react";
import {
  View,
  Text,
  StyleSheet,
  ScrollView,
  TouchableOpacity,
} from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import { Ionicons } from "@expo/vector-icons";
import { router } from "expo-router";
import { COLORS } from "../../../src/constants/colors";

import BlinkLogo from "../../../assets/blinkLogo.svg";

import ScanIcon from "../../../assets/QrCode.svg";
import TapIcon from "../../../assets/icon-3.svg";
import TrendUpIcon from "../../../assets/TrendUp.svg";
import BankIcon from "../../../assets/Bank.svg";

const TokenItem = ({ _name, symbol, balance, value, Icon }: any) => (
  <View style={styles.tokenCard}>
    <View style={styles.tokenIcon}>
      <Icon width={24} height={24} />
    </View>
    <View style={styles.tokenInfo}>
      <Text style={styles.tokenSymbol}>{symbol}</Text>
      <Text style={styles.tokenBalance}>{balance}</Text>
    </View>
    <Text style={styles.tokenValue}>${value}</Text>
  </View>
);

const ActionButton = ({ label, Icon, onPress }: any) => {
  return (
    <TouchableOpacity
      style={styles.actionButton}
      activeOpacity={0.8}
      onPress={onPress}
    >
      <View style={styles.actionIconContainer}>
        <Icon width={24} height={24} />
      </View>
      <Text style={styles.actionLabel}>{label}</Text>
    </TouchableOpacity>
  );
};

export default function HomeScreen() {
  return (
    <SafeAreaView style={styles.container} edges={["top"]}>
      <View style={styles.header}>
        <BlinkLogo width={80} height={38} />
        <TouchableOpacity style={styles.notificationBtn}>
          <Ionicons
            name="notifications-outline"
            size={24}
            color={COLORS.black}
          />
        </TouchableOpacity>
      </View>

      <ScrollView
        contentContainerStyle={styles.scrollContent}
        showsVerticalScrollIndicator={false}
      >
        <View style={styles.balanceCard}>
          <Text style={styles.balanceLabel}>USD balance</Text>
          <Text style={styles.balanceAmount}>$15,046.12</Text>

          <View style={styles.tokenList}>
            <TokenItem
              symbol="Today's Sales"
              // balance="100.00"
              value="125.32"
              Icon={TrendUpIcon}
            />
          </View>

          <View style={styles.BLINKSIdContainer}>
            <Text style={styles.BLINKSIdLabel}>Zap ID</Text>
            <View style={styles.BLINKSIdRow}>
              <Text style={styles.BLINKSIdValue}>Ejembiii.zap</Text>
              <TouchableOpacity>
                <Ionicons name="copy-outline" size={16} color={COLORS.black} />
              </TouchableOpacity>
            </View>
          </View>
        </View>

        <View style={styles.actionsGrid}>
          <ActionButton
            label="Tap-to-pay Payment"
            Icon={TapIcon}
            onPress={() => router.push("/merchant/accept-payment")}
          />
          <ActionButton
            label="Receive Via QR Code"
            Icon={ScanIcon}
            onPress={() => router.push("/merchant/qr-code")}
          />
          <ActionButton label="Withdraw to Bank" Icon={BankIcon} />
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
    justifyContent: "space-between",
    alignItems: "center",
    paddingHorizontal: 20,
    paddingVertical: 15,
  },
  logo: {
    fontSize: 24,
    fontFamily: "Outfit_700Bold",
    color: COLORS.black,
  },
  notificationBtn: {
    padding: 5,
  },
  scrollContent: {
    paddingHorizontal: 20,
    paddingBottom: 20,
  },
  balanceCard: {
    backgroundColor: COLORS.white,
    borderRadius: 24,
    padding: 24,
    borderWidth: 1,
    borderColor: "#F0F0F0",
    elevation: 2,
    shadowColor: "#000",
    shadowOffset: { width: 0, height: 2 },
    shadowOpacity: 0.05,
    shadowRadius: 10,
    marginBottom: 24,
  },
  balanceLabel: {
    fontSize: 16,
    fontFamily: "Outfit_400Regular",
    color: "#666",
    marginBottom: 8,
  },
  balanceAmount: {
    fontSize: 36,
    fontFamily: "Outfit_700Bold",
    color: COLORS.black,
    marginBottom: 24,
  },
  tokenList: {
    gap: 12,
    marginBottom: 20,
  },
  tokenCard: {
    flexDirection: "row",
    alignItems: "center",
    padding: 12,
    borderRadius: 100,
    borderWidth: 1,
    borderColor: "#F0F0F0",
  },
  tokenIcon: {
    width: 40,
    height: 40,
    borderRadius: 20,
    backgroundColor: "#F5F5F5",
    justifyContent: "center",
    alignItems: "center",
    marginRight: 12,
  },
  tokenInfo: {
    flex: 1,
  },
  tokenSymbol: {
    fontSize: 16,
    fontFamily: "Outfit_700Bold",
    color: COLORS.black,
  },
  tokenBalance: {
    fontSize: 14,
    fontFamily: "Outfit_400Regular",
    color: "#666",
  },
  tokenValue: {
    fontSize: 16,
    fontFamily: "Outfit_500Medium",
    color: COLORS.black,
  },
  BLINKSIdContainer: {
    flexDirection: "row",
    justifyContent: "space-between",
    alignItems: "center",
    paddingTop: 15,
    borderTopWidth: 1,
    borderTopColor: "#F0F0F0",
  },
  BLINKSIdLabel: {
    fontSize: 14,
    fontFamily: "Outfit_400Regular",
    color: "#999",
  },
  BLINKSIdRow: {
    flexDirection: "row",
    alignItems: "center",
    gap: 8,
  },
  BLINKSIdValue: {
    fontSize: 14,
    fontFamily: "Outfit_700Bold",
    color: COLORS.black,
  },
  actionsGrid: {
    flexDirection: "column",
    gap: 12,
  },
  actionButton: {
    width: "100%",
    height: 56,
    backgroundColor: COLORS.primary,
    borderRadius: 100,
    justifyContent: "center",
    alignItems: "center",
    flexDirection: "row",
    paddingHorizontal: 20,
  },
  actionIconContainer: {
    marginRight: 8,
  },
  actionLabel: {
    fontSize: 16,
    fontFamily: "Outfit_600SemiBold",
    color: COLORS.secondary,
  },
});
