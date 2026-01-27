import React, { useState } from "react";
import {
  View,
  Text,
  StyleSheet,
  FlatList,
  TouchableOpacity,
  SafeAreaView,
} from "react-native";
import { Ionicons, Feather } from "@expo/vector-icons";
import { COLORS } from "../../src/constants/colors";
import { useRouter } from "expo-router";

const TRANSACTION_DATA = [
  {
    id: "1",
    type: "received",
    address: "0x4A7d5cBe16...da79bB2cF9a1B",
    time: "14:03:23pm",
    date: "Nov 12",
    amount: "0.00",
    value: "$0.00",
  },
  {
    id: "2",
    type: "transfer",
    address: "0x4A7d5cBe16...da79bB2cF9a1B",
    time: "14:03:23pm",
    date: "Nov 12",
    amount: "0.00",
    value: "$0.00",
  },
  {
    id: "3",
    type: "transfer",
    address: "0x4A7d5cBe16...da79bB2cF9a1B",
    time: "14:03:23pm",
    date: "Nov 12",
    amount: "0.00",
    value: "$0.00",
  },
  {
    id: "4",
    type: "received",
    address: "0x4A7d5cBe16...da79bB2cF9a1B",
    time: "14:03:23pm",
    date: "Nov 12",
    amount: "0.00",
    value: "$0.00",
  },
  {
    id: "5",
    type: "received",
    address: "0x4A7d5cBe16...da79bB2cF9a1B",
    time: "14:03:23pm",
    date: "Nov 12",
    amount: "0.00",
    value: "$0.00",
  },
];

const FilterTab = ({ label, active, onPress }: any) => (
  <TouchableOpacity
    style={[styles.filterTab, active && styles.filterTabActive]}
    onPress={onPress}
  >
    <Text style={[styles.filterText, active && styles.filterTextActive]}>
      {label}
    </Text>
  </TouchableOpacity>
);

const TransactionItem = ({ item }: any) => (
  <View style={styles.transactionCard}>
    <View
      style={[
        styles.statusIcon,
        item.type === "received" ? styles.receivedIcon : styles.transferIcon,
      ]}
    >
      <Feather
        name="repeat"
        size={16}
        color={item.type === "received" ? "#4CAF50" : "#FF5252"}
        style={{
          transform: [{ rotate: item.type === "received" ? "180deg" : "0deg" }],
        }}
      />
    </View>
    <View style={styles.transactionInfo}>
      <Text style={styles.address} numberOfLines={1}>
        {item.address}
      </Text>
      <Text style={styles.dateTime}>
        {item.time} . {item.date}
      </Text>
    </View>
    <View style={styles.amountInfo}>
      <Text style={styles.amount}>{item.amount}</Text>
      <Text style={styles.value}>{item.value}</Text>
    </View>
  </View>
);

export default function HistoryScreen() {
  const router = useRouter();
  const [activeFilter, setActiveFilter] = useState("All");

  const filteredData = TRANSACTION_DATA.filter((item) => {
    if (activeFilter === "All") return true;
    return item.type === activeFilter.toLowerCase();
  });

  return (
    <SafeAreaView style={styles.container}>
      <View style={styles.header}>
        <TouchableOpacity
          onPress={() => router.back()}
          style={styles.backButton}
        >
          <Ionicons name="arrow-back" size={24} color={COLORS.black} />
        </TouchableOpacity>
        <Text style={styles.headerTitle}>History</Text>
        <View style={{ width: 24 }} />
      </View>

      <View style={styles.filterContainer}>
        <FilterTab
          label="All"
          active={activeFilter === "All"}
          onPress={() => setActiveFilter("All")}
        />
        <FilterTab
          label="Transfer"
          active={activeFilter === "Transfer"}
          onPress={() => setActiveFilter("Transfer")}
        />
        <FilterTab
          label="Received"
          active={activeFilter === "Received"}
          onPress={() => setActiveFilter("Received")}
        />
      </View>

      <FlatList
        data={filteredData}
        keyExtractor={(item) => item.id}
        renderItem={({ item }) => <TransactionItem item={item} />}
        contentContainerStyle={styles.listContent}
        showsVerticalScrollIndicator={false}
        ListEmptyComponent={
          <View style={styles.emptyContainer}>
            <Text style={styles.emptyText}>No transactions found</Text>
          </View>
        }
      />
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
  filterContainer: {
    flexDirection: "row",
    paddingHorizontal: 20,
    gap: 12,
    marginBottom: 20,
    justifyContent: "center",
  },
  filterTab: {
    paddingHorizontal: 24,
    paddingVertical: 10,
    borderRadius: 100,
    borderWidth: 1,
    borderColor: "#E0E0E0",
  },
  filterTabActive: {
    backgroundColor: COLORS.white,
    borderColor: COLORS.primary,
  },
  filterText: {
    fontSize: 16,
    fontFamily: "Outfit_500Medium",
    color: "#666",
  },
  filterTextActive: {
    color: COLORS.primary,
  },
  listContent: {
    paddingHorizontal: 20,
    paddingBottom: 20,
    gap: 12,
  },
  transactionCard: {
    flexDirection: "row",
    alignItems: "center",
    padding: 16,
    backgroundColor: "#FAFAFA",
    borderRadius: 100,
  },
  statusIcon: {
    width: 40,
    height: 40,
    borderRadius: 20,
    justifyContent: "center",
    alignItems: "center",
    marginRight: 12,
  },
  receivedIcon: {
    backgroundColor: "#E8F5E9",
  },
  transferIcon: {
    backgroundColor: "#FFEBEE",
  },
  transactionInfo: {
    flex: 1,
  },
  address: {
    fontSize: 14,
    fontFamily: "Outfit_600SemiBold",
    color: COLORS.black,
    marginBottom: 2,
  },
  dateTime: {
    fontSize: 12,
    fontFamily: "Outfit_400Regular",
    color: "#999",
  },
  amountInfo: {
    alignItems: "flex-end",
  },
  amount: {
    fontSize: 14,
    fontFamily: "Outfit_700Bold",
    color: COLORS.black,
  },
  value: {
    fontSize: 12,
    fontFamily: "Outfit_400Regular",
    color: "#999",
  },
  emptyContainer: {
    flex: 1,
    justifyContent: "center",
    alignItems: "center",
    marginTop: 100,
  },
  emptyText: {
    fontFamily: "Outfit_400Regular",
    color: "#999",
  },
});
