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
  ViewStyle,
  StyleProp,
} from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import { Ionicons } from "@expo/vector-icons";
import * as Haptics from "expo-haptics";
import { useRouter } from "expo-router";
import { COLORS } from "../../src/constants/colors";
import AsyncStorage from "@react-native-async-storage/async-storage";
import {
  likePayment,
  unlikePayment,
  fetchFeed,
} from "../../src/services/socialService";

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

interface YieldSnapshot {
  apy: string;
  totalYieldEarned: string;
  explanation: string;
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
  const [balance] = useState("₦32,450.00");
  const [yieldData, setYieldData] = useState<YieldSnapshot | null>(null);
  const [yieldStatus, setYieldStatus] = useState<"loading" | "success" | "error">(
    "loading"
  );
  const [yieldError, setYieldError] = useState("");
  const [yieldRetryCount, setYieldRetryCount] = useState(0);
  const [earningsModalVisible, setEarningsModalVisible] = useState(false);
  const earningsSheetTranslateY = useRef(new Animated.Value(48)).current;
  const earningsBackdropOpacity = useRef(new Animated.Value(0)).current;
  const shimmerAnim = useRef(new Animated.Value(-1)).current;
  const yieldRequestRef = useRef(0);

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

  const FEED_CACHE_KEY = "feed_items_cache";
  const YIELD_REQUEST_TIMEOUT_MS = 4500;

  const fetchYieldSnapshot = async (): Promise<YieldSnapshot> => {
    await new Promise((resolve) => setTimeout(resolve, 1100));
    return {
      apy: "8.75%",
      totalYieldEarned: "₦3,280.45",
      explanation:
        "Your earnings are generated from your wallet balance and may vary as rates change. APY is an annualized estimate and total yield is updated automatically over time.",
    };
  };

  const withTimeout = async <T,>(
    promise: Promise<T>,
    timeoutMs: number
  ): Promise<T> => {
    const timeoutPromise = new Promise<T>((_, reject) => {
      setTimeout(
        () => reject(new Error("Request timed out. Please try again.")),
        timeoutMs
      );
    });
    return Promise.race([promise, timeoutPromise]);
  };

  const loadYieldData = useCallback(async () => {
    const requestId = ++yieldRequestRef.current;
    setYieldStatus("loading");
    setYieldError("");
    try {
      const data = await withTimeout(
        fetchYieldSnapshot(),
        YIELD_REQUEST_TIMEOUT_MS
      );
      if (requestId !== yieldRequestRef.current) return;
      setYieldData(data);
      setYieldStatus("success");
    } catch (error) {
      if (requestId !== yieldRequestRef.current) return;
      const message =
        error instanceof Error
          ? error.message
          : "Unable to load yield details right now.";
      setYieldError(message);
      setYieldStatus("error");
    }
  }, []);

  // On mount: hydrate UI from cache instantly, then fetch fresh data and overwrite cache
  useEffect(() => {
    const loadFeed = async () => {
      // Step 1: Load cached feed so content is visible immediately
      try {
        const cached = await AsyncStorage.getItem(FEED_CACHE_KEY);
        if (cached !== null) {
          const cachedFeed: FeedItem[] = JSON.parse(cached);
          setFeed(cachedFeed);
        }
      } catch {
        // Cache unreadable (corrupt JSON, etc.) — INITIAL_FEED remains in state
      }

      // Step 2: Fetch fresh data from backend, update state, overwrite cache
      try {
        const fresh = await fetchFeed();
        if (fresh && fresh.length > 0) {
          setFeed(fresh);
          await AsyncStorage.setItem(FEED_CACHE_KEY, JSON.stringify(fresh));
        }
      } catch {
        // Network/server failure — cached or initial data stays visible
      }
    };

    loadFeed();
  }, []);

  useEffect(() => {
    void loadYieldData();
  }, [loadYieldData]);

  useEffect(() => {
    const shimmerLoop = Animated.loop(
      Animated.timing(shimmerAnim, {
        toValue: 1,
        duration: 1000,
        useNativeDriver: true,
      })
    );
    shimmerLoop.start();
    return () => shimmerLoop.stop();
  }, [shimmerAnim]);

  const handleLike = async (id: string) => {
    const currentItem = feed.find((f) => f.id === id);
    if (!currentItem) return;

    void Haptics.impactAsync(Haptics.ImpactFeedbackStyle.Light).catch(
      () => undefined
    );

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
    void Haptics.impactAsync(Haptics.ImpactFeedbackStyle.Medium).catch(
      () => undefined
    );
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

  const openEarningsModal = () => {
    void Haptics.impactAsync(Haptics.ImpactFeedbackStyle.Light).catch(
      () => undefined
    );
    setEarningsModalVisible(true);
    earningsSheetTranslateY.setValue(48);
    earningsBackdropOpacity.setValue(0);
    Animated.parallel([
      Animated.timing(earningsSheetTranslateY, {
        toValue: 0,
        duration: 220,
        useNativeDriver: true,
      }),
      Animated.timing(earningsBackdropOpacity, {
        toValue: 1,
        duration: 220,
        useNativeDriver: true,
      }),
    ]).start();
  };

  const closeEarningsModal = () => {
    Animated.parallel([
      Animated.timing(earningsSheetTranslateY, {
        toValue: 48,
        duration: 200,
        useNativeDriver: true,
      }),
      Animated.timing(earningsBackdropOpacity, {
        toValue: 0,
        duration: 200,
        useNativeDriver: true,
      }),
    ]).start(({ finished }) => {
      if (finished) {
        setEarningsModalVisible(false);
      }
    });
  };

  const handleYieldRetry = () => {
    setYieldRetryCount((prev) => prev + 1);
    shimmerAnim.setValue(-1);
    void loadYieldData();
  };

  const shimmerTranslateX = shimmerAnim.interpolate({
    inputRange: [-1, 1],
    outputRange: [-220, 220],
  });

  const SkeletonBlock = ({ style }: { style?: StyleProp<ViewStyle> }) => (
    <View style={[styles.skeletonBase, style]}>
      <Animated.View
        style={[
          styles.skeletonShimmer,
          { transform: [{ translateX: shimmerTranslateX }] },
        ]}
      />
    </View>
  );

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

          <TouchableOpacity
            style={styles.payRequestButton}
            onPress={() => router.push("/transfer")}
          >
            <Ionicons
              name="send"
              size={18}
              color={COLORS.secondary}
              style={styles.payRequestIcon}
            />
            <Text style={styles.payRequestButtonText}>Pay / Request</Text>
          </TouchableOpacity>

          <View style={styles.quickActions}>
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

        <TouchableOpacity
          style={styles.earningBalanceCard}
          activeOpacity={0.9}
          onPress={openEarningsModal}
        >
          {yieldStatus === "loading" ? (
            <View style={styles.earningContent}>
              <SkeletonBlock style={styles.earningLabelSkeleton} />
              <SkeletonBlock style={styles.earningAmountSkeleton} />
              <SkeletonBlock style={styles.earningHintSkeleton} />
            </View>
          ) : yieldStatus === "error" ? (
            <View style={styles.earningContent}>
              <Text style={styles.earningLabel}>Earning Balance</Text>
              <Text style={styles.earningErrorText}>Unable to load yield</Text>
              <TouchableOpacity
                style={styles.retryChip}
                onPress={handleYieldRetry}
                activeOpacity={0.85}
              >
                <Ionicons name="refresh" size={12} color={COLORS.primary} />
                <Text style={styles.retryChipText}>Retry</Text>
              </TouchableOpacity>
            </View>
          ) : (
            <View style={styles.earningContent}>
              <Text style={styles.earningLabel}>Earning Balance</Text>
              <Text style={styles.earningAmount}>
                {yieldData?.totalYieldEarned ?? "₦0.00"}
              </Text>
              <Text style={styles.earningHint}>Tap to view yield breakdown</Text>
            </View>
          )}
          <View style={styles.earningIconWrap}>
            <Ionicons name="trending-up" size={20} color={COLORS.primary} />
          </View>
        </TouchableOpacity>

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
                <View style={styles.avatarStack}>
                  <View style={[styles.avatar, styles.avatarPrimary]}>
                    <Text style={styles.avatarText}>{item.sender[0]}</Text>
                  </View>
                  <View style={[styles.avatar, styles.avatarSecondary]}>
                    <Text style={styles.avatarText}>{item.receiver[0]}</Text>
                  </View>
                </View>

                <View style={styles.paymentInfo}>
                  <View style={styles.paymentRow}>
                    <Text style={styles.paymentText} numberOfLines={2}>
                      <Text style={styles.boldText}>{item.sender}</Text> paid{" "}
                      <Text style={styles.boldText}>{item.receiver}</Text>
                    </Text>
                    <View style={styles.amountPill}>
                      <Text style={styles.amountText}>{item.amount}</Text>
                    </View>
                  </View>
                  <Text style={styles.timestamp}>{item.timestamp}</Text>
                </View>
              </View>

              <View style={styles.memoContainer}>
                <Text style={styles.memoLabel}>Memo</Text>
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

      {/* Earning Balance Modal */}
      <Modal
        visible={earningsModalVisible}
        transparent={true}
        animationType="none"
        onRequestClose={closeEarningsModal}
      >
        <View style={styles.earningsModalRoot}>
          <TouchableOpacity
            activeOpacity={1}
            style={StyleSheet.absoluteFill}
            onPress={closeEarningsModal}
          >
            <Animated.View
              style={[
                styles.earningsBackdrop,
                { opacity: earningsBackdropOpacity },
              ]}
            />
          </TouchableOpacity>

          <Animated.View
            style={[
              styles.earningsSheet,
              { transform: [{ translateY: earningsSheetTranslateY }] },
            ]}
          >
            <View style={styles.earningsGrabber} />
            <View style={styles.earningsHeader}>
              <Text style={styles.earningsTitle}>Earning Balance</Text>
              <TouchableOpacity onPress={closeEarningsModal}>
                <Ionicons name="close" size={22} color="#111827" />
              </TouchableOpacity>
            </View>

            {yieldStatus === "loading" ? (
              <>
                <View style={styles.earningsMetricCard}>
                  <SkeletonBlock style={styles.modalLabelSkeleton} />
                  <SkeletonBlock style={styles.modalValueSkeleton} />
                </View>
                <View style={styles.earningsMetricCard}>
                  <SkeletonBlock style={styles.modalLabelSkeleton} />
                  <SkeletonBlock style={styles.modalValueSkeleton} />
                </View>
                <SkeletonBlock style={styles.modalCopySkeleton} />
                <SkeletonBlock style={styles.modalCopySkeletonShort} />
              </>
            ) : yieldStatus === "error" ? (
              <View style={styles.yieldErrorCard}>
                <Text style={styles.yieldErrorTitle}>Could not load details</Text>
                <Text style={styles.yieldErrorCopy}>
                  {yieldError}. Check your connection and try again.
                </Text>
                <TouchableOpacity
                  style={styles.retryButton}
                  onPress={handleYieldRetry}
                >
                  <Ionicons name="refresh" size={16} color={COLORS.secondary} />
                  <Text style={styles.retryButtonText}>Retry</Text>
                </TouchableOpacity>
                {yieldRetryCount > 0 && (
                  <Text style={styles.retryMetaText}>
                    Retry attempts: {yieldRetryCount}
                  </Text>
                )}
              </View>
            ) : (
              <>
                <View style={styles.earningsMetricCard}>
                  <Text style={styles.earningsMetricLabel}>Current APY</Text>
                  <Text style={styles.earningsMetricValue}>
                    {yieldData?.apy ?? "0.00%"}
                  </Text>
                </View>

                <View style={styles.earningsMetricCard}>
                  <Text style={styles.earningsMetricLabel}>
                    Total Yield Earned
                  </Text>
                  <Text style={styles.earningsMetricValue}>
                    {yieldData?.totalYieldEarned ?? "₦0.00"}
                  </Text>
                </View>

                <Text style={styles.earningsInfoCopy}>
                  {yieldData?.explanation}
                </Text>
              </>
            )}
          </Animated.View>
        </View>
      </Modal>

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
  payRequestButton: {
    backgroundColor: COLORS.primary,
    borderRadius: 18,
    paddingVertical: 14,
    paddingHorizontal: 18,
    flexDirection: "row",
    alignItems: "center",
    justifyContent: "center",
    marginBottom: 14,
    shadowColor: "#000",
    shadowOffset: { width: 0, height: 6 },
    shadowOpacity: 0.08,
    shadowRadius: 12,
    elevation: 4,
  },
  payRequestButtonText: {
    color: COLORS.secondary,
    fontSize: 16,
    fontFamily: "Outfit_700Bold",
  },
  payRequestIcon: {
    marginRight: 8,
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
    marginTop: 12,
  },
  earningBalanceCard: {
    backgroundColor: "#EEF7EA",
    borderRadius: 22,
    borderWidth: 1,
    borderColor: "#D7EACF",
    paddingVertical: 16,
    paddingHorizontal: 16,
    marginBottom: 8,
    flexDirection: "row",
    justifyContent: "space-between",
    alignItems: "center",
  },
  earningContent: {
    flex: 1,
    marginRight: 12,
  },
  earningLabel: {
    fontSize: 13,
    fontFamily: "Outfit_500Medium",
    color: "#456047",
    marginBottom: 6,
  },
  earningAmount: {
    fontSize: 24,
    fontFamily: "Outfit_700Bold",
    color: COLORS.primary,
    marginBottom: 2,
  },
  earningHint: {
    fontSize: 12,
    fontFamily: "Outfit_400Regular",
    color: "#56785A",
  },
  earningErrorText: {
    fontSize: 14,
    fontFamily: "Outfit_600SemiBold",
    color: "#B45309",
    marginBottom: 6,
  },
  retryChip: {
    borderWidth: 1,
    borderColor: "#CBD5E1",
    borderRadius: 14,
    alignSelf: "flex-start",
    paddingHorizontal: 10,
    paddingVertical: 5,
    flexDirection: "row",
    alignItems: "center",
    gap: 6,
    backgroundColor: "#F8FAFC",
  },
  retryChipText: {
    fontSize: 12,
    color: COLORS.primary,
    fontFamily: "Outfit_600SemiBold",
  },
  earningIconWrap: {
    width: 40,
    height: 40,
    borderRadius: 20,
    backgroundColor: "#DDEFD5",
    justifyContent: "center",
    alignItems: "center",
  },
  earningsModalRoot: {
    flex: 1,
    justifyContent: "flex-end",
  },
  earningsBackdrop: {
    ...StyleSheet.absoluteFillObject,
    backgroundColor: "rgba(0, 0, 0, 0.38)",
  },
  earningsSheet: {
    backgroundColor: COLORS.white,
    borderTopLeftRadius: 26,
    borderTopRightRadius: 26,
    paddingHorizontal: 20,
    paddingTop: 12,
    paddingBottom: 34,
    gap: 12,
  },
  earningsGrabber: {
    alignSelf: "center",
    width: 42,
    height: 4,
    borderRadius: 999,
    backgroundColor: "#E2E8F0",
    marginBottom: 6,
  },
  earningsHeader: {
    flexDirection: "row",
    justifyContent: "space-between",
    alignItems: "center",
    marginBottom: 4,
  },
  earningsTitle: {
    fontSize: 19,
    fontFamily: "Outfit_700Bold",
    color: COLORS.primary,
  },
  earningsMetricCard: {
    borderWidth: 1,
    borderColor: "#E5E7EB",
    borderRadius: 16,
    paddingVertical: 12,
    paddingHorizontal: 14,
    backgroundColor: "#FAFAFA",
  },
  earningsMetricLabel: {
    fontSize: 12,
    fontFamily: "Outfit_500Medium",
    color: "#6B7280",
    marginBottom: 4,
  },
  earningsMetricValue: {
    fontSize: 22,
    fontFamily: "Outfit_700Bold",
    color: "#111827",
  },
  earningsInfoCopy: {
    fontSize: 13,
    lineHeight: 20,
    color: "#475569",
    fontFamily: "Outfit_400Regular",
  },
  yieldErrorCard: {
    borderWidth: 1,
    borderColor: "#F5D0C5",
    backgroundColor: "#FFF7F5",
    borderRadius: 16,
    paddingHorizontal: 14,
    paddingVertical: 14,
  },
  yieldErrorTitle: {
    fontSize: 15,
    fontFamily: "Outfit_700Bold",
    color: "#9A3412",
    marginBottom: 4,
  },
  yieldErrorCopy: {
    fontSize: 13,
    lineHeight: 19,
    fontFamily: "Outfit_400Regular",
    color: "#7C2D12",
    marginBottom: 12,
  },
  retryButton: {
    backgroundColor: COLORS.primary,
    borderRadius: 12,
    paddingVertical: 10,
    flexDirection: "row",
    alignItems: "center",
    justifyContent: "center",
    gap: 8,
  },
  retryButtonText: {
    color: COLORS.secondary,
    fontSize: 14,
    fontFamily: "Outfit_700Bold",
  },
  retryMetaText: {
    fontSize: 12,
    fontFamily: "Outfit_400Regular",
    color: "#92400E",
    marginTop: 10,
  },
  skeletonBase: {
    backgroundColor: "#DDE6D9",
    borderRadius: 10,
    overflow: "hidden",
  },
  skeletonShimmer: {
    position: "absolute",
    top: 0,
    bottom: 0,
    width: "45%",
    backgroundColor: "rgba(255,255,255,0.5)",
  },
  earningLabelSkeleton: {
    height: 13,
    width: "42%",
    marginBottom: 10,
  },
  earningAmountSkeleton: {
    height: 30,
    width: "58%",
    marginBottom: 8,
  },
  earningHintSkeleton: {
    height: 13,
    width: "64%",
  },
  modalLabelSkeleton: {
    height: 12,
    width: "34%",
    marginBottom: 8,
  },
  modalValueSkeleton: {
    height: 26,
    width: "56%",
  },
  modalCopySkeleton: {
    height: 14,
    width: "100%",
    marginTop: 4,
  },
  modalCopySkeletonShort: {
    height: 14,
    width: "78%",
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
    borderRadius: 24,
    padding: 16,
    marginBottom: 14,
    borderWidth: 1,
    borderColor: "#ECECEC",
    shadowColor: "#0F172A",
    shadowOffset: { width: 0, height: 10 },
    shadowOpacity: 0.08,
    shadowRadius: 16,
    elevation: 3,
  },
  feedHeader: {
    flexDirection: "row",
    alignItems: "flex-start",
    marginBottom: 12,
  },
  avatarStack: {
    flexDirection: "row",
    alignItems: "center",
    marginRight: 12,
  },
  avatar: {
    width: 40,
    height: 40,
    borderRadius: 20,
    justifyContent: "center",
    alignItems: "center",
    borderWidth: 2,
    borderColor: COLORS.white,
  },
  avatarPrimary: {
    backgroundColor: "#E2F0D9",
    marginRight: -8,
    zIndex: 2,
  },
  avatarSecondary: {
    backgroundColor: "#FCEEDC",
    zIndex: 1,
  },
  avatarText: {
    color: COLORS.primary,
    fontFamily: "Outfit_700Bold",
    fontSize: 16,
  },
  paymentInfo: {
    flex: 1,
  },
  paymentRow: {
    flexDirection: "row",
    justifyContent: "space-between",
    alignItems: "flex-start",
    flexWrap: "wrap",
  },
  paymentText: {
    flex: 1,
    fontSize: 15,
    lineHeight: 20,
    fontFamily: "Outfit_400Regular",
    color: "#334155",
    flexShrink: 1,
    marginRight: 8,
  },
  boldText: {
    fontFamily: "Outfit_700Bold",
    color: "#111827",
  },
  amountPill: {
    backgroundColor: "#F2F9F0",
    borderRadius: 999,
    paddingHorizontal: 10,
    paddingVertical: 6,
    borderWidth: 1,
    borderColor: "#DDF2DD",
    alignSelf: "flex-start",
    marginTop: 2,
  },
  amountText: {
    fontSize: 13,
    fontFamily: "Outfit_700Bold",
    color: "#2E7D32",
  },
  timestamp: {
    fontSize: 12,
    color: "#94A3B8",
    marginTop: 4,
  },
  memoContainer: {
    backgroundColor: "#F4F6F8",
    borderColor: "#E7EBEF",
    borderWidth: 1,
    paddingHorizontal: 12,
    paddingVertical: 10,
    borderRadius: 16,
  },
  memoLabel: {
    fontSize: 11,
    color: "#64748B",
    fontFamily: "Outfit_600SemiBold",
    textTransform: "uppercase",
    letterSpacing: 0.8,
    marginBottom: 2,
  },
  memoText: {
    fontSize: 14,
    color: "#334155",
    fontFamily: "Outfit_400Regular",
    lineHeight: 19,
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
