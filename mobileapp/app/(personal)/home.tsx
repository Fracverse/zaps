import React, { useState, useEffect, useRef, useCallback } from "react";
import {
  View,
  Text,
  StyleSheet,
  ScrollView,
  TouchableOpacity,
  TextInput,
  Modal,
  FlatList,
  Animated,
} from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import { Ionicons } from "@expo/vector-icons";
import { useRouter } from "expo-router";
import { COLORS } from "../../src/constants/colors";
import AsyncStorage from "@react-native-async-storage/async-storage";
import { likePayment, unlikePayment } from "../../src/services/socialService";

interface FeedItem {
  id: string;
  sender: string;
  receiver: string;
  amount: string;
  description: string;
  timestamp: string;
  likes: number;
  comments: number;
  hasLiked: boolean;
  visibility: "PUBLIC" | "FRIENDS" | "PRIVATE";
}

const INITIAL_FEED: FeedItem[] = [
  {
    id: "1",
    sender: "Ebube",
    receiver: "Tolu",
    amount: "₦5,000",
    description: "Lunch 🍕",
    timestamp: "2h ago",
    likes: 5,
    comments: 2,
    hasLiked: false,
    visibility: "PUBLIC",
  },
  {
    id: "2",
    sender: "Ejembiii",
    receiver: "Amina",
    amount: "₦12,500",
    description: "Concert tickets 🎟️",
    timestamp: "5h ago",
    likes: 12,
    comments: 4,
    hasLiked: true,
    visibility: "PUBLIC",
  },
  {
    id: "3",
    sender: "Tunde",
    receiver: "Chidi",
    amount: "₦2,000",
    description: "Taxi ride 🚕",
    timestamp: "1d ago",
    likes: 2,
    comments: 0,
    hasLiked: false,
    visibility: "FRIENDS",
  },
];

export default function HomeScreen() {
  const router = useRouter();
  const [activeTab, setActiveTab] = useState<"public" | "friends">("public");
  const [feed, setFeed] = useState<FeedItem[]>(INITIAL_FEED);
  const [balance, setBalance] = useState("₦32,450.00");

  // Animated values for like heart scale per feed item
  const scaleAnims = useRef<Map<string, Animated.Value>>(new Map());

  const getScaleAnim = useCallback((id: string) => {
    if (!scaleAnims.current.has(id)) {
      scaleAnims.current.set(id, new Animated.Value(1));
    }
    return scaleAnims.current.get(id)!;
  }, []);

  // Comments Modal State
  const [commentsModalVisible, setCommentsModalVisible] = useState(false);
  const [selectedItem, setSelectedItem] = useState<FeedItem | null>(null);
  const [commentText, setCommentText] = useState("");
  const [commentsList, setCommentsList] = useState<
    { id: string; user: string; text: string; time: string }[]
  >([]);

  // Load new transfers from AsyncStorage to make it dynamic
  useEffect(() => {
    const loadNewTransfers = async () => {
      try {
        const stored = await AsyncStorage.getItem("pending_transfers");
        if (stored) {
          const transfers = JSON.parse(stored);
          const formatted: FeedItem[] = transfers.map(
            (tx: any, idx: number) => ({
              id: `stored_${idx}`,
              sender: "Me",
              receiver: tx.recipient,
              amount: `₦${tx.amount}`,
              description: tx.description || "Sent payment",
              timestamp: "Just now",
              likes: 0,
              comments: 0,
              hasLiked: false,
              visibility: tx.visibility || "PUBLIC",
            })
          );

          // Filter private out of public feed
          setFeed([...formatted, ...INITIAL_FEED]);

          // Deduct from balance
          const totalDeducted = transfers.reduce(
            (acc: number, item: any) =>
              acc + parseFloat(item.amount.replace(/,/g, "")),
            0
          );
          if (totalDeducted > 0) {
            setBalance(
              `₦${(32450 - totalDeducted).toLocaleString("en-US", { minimumFractionDigits: 2 })}`
            );
          }
        }
      } catch (e) {
        console.error(e);
      }
    };
    loadNewTransfers();
  }, []);

  const handleLike = async (id: string) => {
    const currentItem = feed.find((f) => f.id === id);
    if (!currentItem) return;

    const prevHasLiked = currentItem.hasLiked;
    const prevLikes = currentItem.likes;
    const newHasLiked = !prevHasLiked;
    const newLikes = prevHasLiked ? prevLikes - 1 : prevLikes + 1;

    // Scale animation for instant UI feedback
    const scale = getScaleAnim(id);
    Animated.sequence([
      Animated.spring(scale, {
        toValue: 1.3,
        useNativeDriver: true,
        friction: 3,
      }),
      Animated.spring(scale, {
        toValue: 1,
        useNativeDriver: true,
        friction: 3,
      }),
    ]).start();

    // Optimistic local update
    setFeed((prev) =>
      prev.map((f) =>
        f.id === id ? { ...f, hasLiked: newHasLiked, likes: newLikes } : f
      )
    );

    // Sync to backend
    try {
      if (newHasLiked) {
        await likePayment(id);
      } else {
        await unlikePayment(id);
      }
    } catch {
      // Revert on failure
      setFeed((prev) =>
        prev.map((f) =>
          f.id === id ? { ...f, hasLiked: prevHasLiked, likes: prevLikes } : f
        )
      );
    }
  };

  const openComments = (item: FeedItem) => {
    setSelectedItem(item);
    setCommentsList([
      {
        id: "c1",
        user: "Tolu",
        text: "Thanks for the food! 😋",
        time: "1h ago",
      },
      {
        id: "c2",
        user: "Ebube",
        text: "Anytime! Let's do it again.",
        time: "45m ago",
      },
    ]);
    setCommentsModalVisible(true);
  };

  const submitComment = () => {
    if (!commentText.trim() || !selectedItem) return;
    const newComment = {
      id: Date.now().toString(),
      user: "Me",
      text: commentText,
      time: "Just now",
    };
    setCommentsList([...commentsList, newComment]);
    setCommentText("");

    // Update comments count on item
    setFeed(
      feed.map((item) => {
        if (item.id === selectedItem.id) {
          return { ...item, comments: item.comments + 1 };
        }
        return item;
      })
    );
  };

  const filteredFeed = feed.filter((item) => {
    if (item.visibility === "PRIVATE") return false;
    if (activeTab === "friends") {
      return (
        item.visibility === "FRIENDS" ||
        item.sender === "Me" ||
        item.receiver === "Me"
      );
    }
    return true; // public feed shows all non-private
  });

  return (
    <SafeAreaView style={styles.container} edges={["top"]}>
      {/* Top Header */}
      <View style={styles.header}>
        <Text style={styles.logo}>zaps</Text>
        <View style={styles.headerIcons}>
          <TouchableOpacity
            style={styles.headerBtn}
            onPress={() => router.push("/(personal)/settings")}
          >
            <Ionicons
              name="settings-outline"
              size={22}
              color={COLORS.primary}
            />
          </TouchableOpacity>
        </View>
      </View>

      <ScrollView
        contentContainerStyle={styles.scrollContent}
        showsVerticalScrollIndicator={false}
      >
        {/* Balance Card */}
        <View style={styles.balanceCard}>
          <Text style={styles.balanceLabel}>Stellar Wallet Balance</Text>
          <Text style={styles.balanceAmount}>{balance}</Text>

          {/* Quick Actions Redesign */}
          <View style={styles.quickActions}>
            <TouchableOpacity
              style={[styles.actionBtn, styles.payBtn]}
              onPress={() => router.push("/transfer")}
            >
              <Ionicons
                name="send"
                size={18}
                color={COLORS.secondary}
                style={{ marginRight: 6 }}
              />
              <Text style={styles.payBtnText}>Pay / Request</Text>
            </TouchableOpacity>

            <TouchableOpacity
              style={[styles.actionBtn, styles.receiveBtn]}
              onPress={() => router.push("/receive")}
            >
              <Ionicons
                name="qr-code-outline"
                size={18}
                color={COLORS.primary}
                style={{ marginRight: 6 }}
              />
              <Text style={styles.receiveBtnText}>Receive</Text>
            </TouchableOpacity>

            <TouchableOpacity
              style={[styles.actionBtn, styles.fundBtn]}
              onPress={() => router.push("/fund")}
            >
              <Ionicons
                name="swap-horizontal"
                size={18}
                color={COLORS.primary}
                style={{ marginRight: 6 }}
              />
              <Text style={styles.fundBtnText}>Fund</Text>
            </TouchableOpacity>
          </View>
        </View>

        {/* Social Feed Section */}
        <View style={styles.feedContainer}>
          {/* Feed Header tabs */}
          <View style={styles.tabBar}>
            <TouchableOpacity
              style={[
                styles.tabItem,
                activeTab === "public" && styles.tabItemActive,
              ]}
              onPress={() => setActiveTab("public")}
            >
              <Text
                style={[
                  styles.tabLabel,
                  activeTab === "public" && styles.tabLabelActive,
                ]}
              >
                Public Feed
              </Text>
            </TouchableOpacity>
            <TouchableOpacity
              style={[
                styles.tabItem,
                activeTab === "friends" && styles.tabItemActive,
              ]}
              onPress={() => setActiveTab("friends")}
            >
              <Text
                style={[
                  styles.tabLabel,
                  activeTab === "friends" && styles.tabLabelActive,
                ]}
              >
                Friends
              </Text>
            </TouchableOpacity>
          </View>

          {/* Feed List */}
          {filteredFeed.map((item) => (
            <View key={item.id} style={styles.feedCard}>
              <View style={styles.feedHeader}>
                <View style={styles.avatar}>
                  <Text style={styles.avatarText}>{item.sender[0]}</Text>
                </View>
                <View style={styles.paymentInfo}>
                  <Text style={styles.paymentText}>
                    <Text style={styles.boldText}>{item.sender}</Text> paid{" "}
                    <Text style={styles.boldText}>{item.receiver}</Text>
                  </Text>
                  <Text style={styles.timestamp}>{item.timestamp}</Text>
                </View>
                <Text style={styles.paymentAmount}>{item.amount}</Text>
              </View>

              <View style={styles.memoContainer}>
                <Text style={styles.memoText}>{item.description}</Text>
              </View>

              <View style={styles.actionsDivider} />

              <View style={styles.feedActions}>
                <TouchableOpacity
                  style={styles.actionItem}
                  onPress={() => handleLike(item.id)}
                >
                  <Animated.View
                    style={{ transform: [{ scale: getScaleAnim(item.id) }] }}
                  >
                    <Ionicons
                      name={item.hasLiked ? "heart" : "heart-outline"}
                      size={20}
                      color={item.hasLiked ? "#EF4444" : "#666"}
                    />
                  </Animated.View>
                  <Text
                    style={[
                      styles.actionCount,
                      item.hasLiked && { color: "#EF4444" },
                    ]}
                  >
                    {item.likes}
                  </Text>
                </TouchableOpacity>

                <TouchableOpacity
                  style={styles.actionItem}
                  onPress={() => openComments(item)}
                >
                  <Ionicons name="chatbubble-outline" size={20} color="#666" />
                  <Text style={styles.actionCount}>{item.comments}</Text>
                </TouchableOpacity>

                <View style={{ flex: 1 }} />

                <Ionicons
                  name={
                    item.visibility === "PUBLIC"
                      ? "globe-outline"
                      : "people-outline"
                  }
                  size={16}
                  color="#999"
                />
              </View>
            </View>
          ))}
        </View>
      </ScrollView>

      {/* Comments Modal */}
      <Modal
        visible={commentsModalVisible}
        animationType="slide"
        transparent={true}
      >
        <View style={styles.modalOverlay}>
          <View style={styles.modalContent}>
            <View style={styles.modalHeader}>
              <Text style={styles.modalTitle}>Comments</Text>
              <TouchableOpacity onPress={() => setCommentsModalVisible(false)}>
                <Ionicons name="close" size={24} color="#000" />
              </TouchableOpacity>
            </View>

            <FlatList
              data={commentsList}
              keyExtractor={(item) => item.id}
              contentContainerStyle={{ paddingVertical: 12 }}
              renderItem={({ item }) => (
                <View style={styles.commentItem}>
                  <View style={styles.commentAvatar}>
                    <Text style={styles.avatarText}>{item.user[0]}</Text>
                  </View>
                  <View style={styles.commentDetails}>
                    <View style={styles.commentMeta}>
                      <Text style={styles.commentUser}>{item.user}</Text>
                      <Text style={styles.commentTime}>{item.time}</Text>
                    </View>
                    <Text style={styles.commentText}>{item.text}</Text>
                  </View>
                </View>
              )}
            />

            <View style={styles.inputContainer}>
              <TextInput
                style={styles.commentInput}
                placeholder="Write a comment..."
                value={commentText}
                onChangeText={setCommentText}
              />
              <TouchableOpacity style={styles.sendBtn} onPress={submitComment}>
                <Ionicons name="send" size={20} color={COLORS.primary} />
              </TouchableOpacity>
            </View>
          </View>
        </View>
      </Modal>
    </SafeAreaView>
  );
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
    backgroundColor: "#FDFDFD",
  },
  header: {
    flexDirection: "row",
    justifyContent: "space-between",
    alignItems: "center",
    paddingHorizontal: 20,
    paddingVertical: 12,
    borderBottomWidth: 1,
    borderBottomColor: "#F0F0F0",
  },
  logo: {
    fontSize: 28,
    fontFamily: "Anton_400Regular",
    letterSpacing: 1.5,
    color: COLORS.primary,
    textTransform: "lowercase",
  },
  headerIcons: {
    flexDirection: "row",
    gap: 12,
  },
  headerBtn: {
    padding: 6,
    borderRadius: 20,
    backgroundColor: "#F5F5F5",
  },
  scrollContent: {
    paddingHorizontal: 16,
    paddingBottom: 32,
    paddingTop: 12,
  },
  balanceCard: {
    backgroundColor: COLORS.white,
    borderRadius: 24,
    padding: 20,
    borderWidth: 1,
    borderColor: "#EAEAEA",
    marginBottom: 20,
    shadowColor: "#000",
    shadowOffset: { width: 0, height: 4 },
    shadowOpacity: 0.03,
    shadowRadius: 10,
    elevation: 2,
  },
  balanceLabel: {
    fontSize: 13,
    fontFamily: "Outfit_400Regular",
    color: "#777",
    marginBottom: 4,
  },
  balanceAmount: {
    fontSize: 34,
    fontFamily: "Outfit_700Bold",
    color: COLORS.primary,
    marginBottom: 20,
  },
  quickActions: {
    flexDirection: "row",
    gap: 10,
  },
  actionBtn: {
    flex: 1,
    height: 48,
    borderRadius: 24,
    justifyContent: "center",
    alignItems: "center",
    flexDirection: "row",
    paddingHorizontal: 10,
  },
  payBtn: {
    flex: 1.5,
    backgroundColor: COLORS.primary,
  },
  payBtnText: {
    color: COLORS.secondary,
    fontSize: 14,
    fontFamily: "Outfit_600SemiBold",
  },
  receiveBtn: {
    backgroundColor: "#F5F5F5",
    borderWidth: 1,
    borderColor: "#E0E0E0",
  },
  receiveBtnText: {
    color: COLORS.primary,
    fontSize: 13,
    fontFamily: "Outfit_600SemiBold",
  },
  fundBtn: {
    backgroundColor: "#F5F5F5",
    borderWidth: 1,
    borderColor: "#E0E0E0",
  },
  fundBtnText: {
    color: COLORS.primary,
    fontSize: 13,
    fontFamily: "Outfit_600SemiBold",
  },
  feedContainer: {
    marginTop: 8,
  },
  tabBar: {
    flexDirection: "row",
    borderBottomWidth: 1,
    borderBottomColor: "#F0F0F0",
    marginBottom: 16,
  },
  tabItem: {
    flex: 1,
    paddingVertical: 12,
    alignItems: "center",
  },
  tabItemActive: {
    borderBottomWidth: 2,
    borderBottomColor: COLORS.primary,
  },
  tabLabel: {
    fontSize: 15,
    fontFamily: "Outfit_500Medium",
    color: "#888",
  },
  tabLabelActive: {
    color: COLORS.primary,
    fontFamily: "Outfit_700Bold",
  },
  feedCard: {
    backgroundColor: COLORS.white,
    borderRadius: 16,
    padding: 16,
    marginBottom: 12,
    borderWidth: 1,
    borderColor: "#ECECEC",
  },
  feedHeader: {
    flexDirection: "row",
    alignItems: "center",
  },
  avatar: {
    width: 40,
    height: 40,
    borderRadius: 20,
    backgroundColor: "#E2F0D9",
    justifyContent: "center",
    alignItems: "center",
    marginRight: 12,
  },
  avatarText: {
    color: COLORS.primary,
    fontFamily: "Outfit_700Bold",
    fontSize: 16,
  },
  paymentInfo: {
    flex: 1,
  },
  paymentText: {
    fontSize: 15,
    fontFamily: "Outfit_400Regular",
    color: "#333",
  },
  boldText: {
    fontFamily: "Outfit_700Bold",
    color: "#111",
  },
  timestamp: {
    fontSize: 12,
    color: "#999",
    marginTop: 2,
  },
  paymentAmount: {
    fontSize: 16,
    fontFamily: "Outfit_700Bold",
    color: "#2E7D32",
  },
  memoContainer: {
    marginTop: 12,
    backgroundColor: "#F5F8F6",
    paddingHorizontal: 12,
    paddingVertical: 8,
    borderRadius: 8,
    alignSelf: "flex-start",
  },
  memoText: {
    fontSize: 14,
    color: "#444",
    fontFamily: "Outfit_400Regular",
  },
  actionsDivider: {
    height: 1,
    backgroundColor: "#F5F5F5",
    marginVertical: 12,
  },
  feedActions: {
    flexDirection: "row",
    alignItems: "center",
    gap: 16,
  },
  actionItem: {
    flexDirection: "row",
    alignItems: "center",
    gap: 6,
  },
  actionCount: {
    fontSize: 13,
    color: "#666",
    fontFamily: "Outfit_500Medium",
  },
  modalOverlay: {
    flex: 1,
    backgroundColor: "rgba(0, 0, 0, 0.4)",
    justifyContent: "flex-end",
  },
  modalContent: {
    backgroundColor: COLORS.white,
    borderTopLeftRadius: 24,
    borderTopRightRadius: 24,
    paddingHorizontal: 20,
    paddingBottom: 40,
    paddingTop: 20,
    maxHeight: "75%",
  },
  modalHeader: {
    flexDirection: "row",
    justifyContent: "space-between",
    alignItems: "center",
    paddingBottom: 16,
    borderBottomWidth: 1,
    borderBottomColor: "#F0F0F0",
  },
  modalTitle: {
    fontSize: 18,
    fontFamily: "Outfit_700Bold",
    color: COLORS.primary,
  },
  commentItem: {
    flexDirection: "row",
    marginBottom: 16,
  },
  commentAvatar: {
    width: 32,
    height: 32,
    borderRadius: 16,
    backgroundColor: "#F0F0F0",
    justifyContent: "center",
    alignItems: "center",
    marginRight: 10,
  },
  commentDetails: {
    flex: 1,
    backgroundColor: "#F5F5F5",
    padding: 10,
    borderRadius: 12,
  },
  commentMeta: {
    flexDirection: "row",
    justifyContent: "space-between",
    marginBottom: 4,
  },
  commentUser: {
    fontSize: 13,
    fontFamily: "Outfit_700Bold",
    color: "#222",
  },
  commentTime: {
    fontSize: 11,
    color: "#999",
  },
  commentText: {
    fontSize: 13,
    color: "#444",
    fontFamily: "Outfit_400Regular",
  },
  inputContainer: {
    flexDirection: "row",
    alignItems: "center",
    borderWidth: 1,
    borderColor: "#E0E0E0",
    borderRadius: 24,
    paddingLeft: 16,
    paddingRight: 8,
    paddingVertical: 4,
    marginTop: 12,
  },
  commentInput: {
    flex: 1,
    height: 40,
    fontSize: 14,
    fontFamily: "Outfit_400Regular",
  },
  sendBtn: {
    padding: 8,
  },
});
