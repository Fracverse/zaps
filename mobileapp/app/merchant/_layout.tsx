import React from "react";
import { Tabs } from "expo-router";
import { Ionicons, Feather } from "@expo/vector-icons";
import { COLORS } from "../../src/constants/colors";

export default function MerchantLayout() {
  return (
    <Tabs
      screenOptions={{
        headerShown: false,
        tabBarActiveTintColor: COLORS.primary,
        tabBarInactiveTintColor: "#999",
        tabBarStyle: {
          backgroundColor: "#FFFFFF",
          borderTopWidth: 1,
          borderTopColor: "#E0E0E0",
          height: 65,
          paddingBottom: 10,
          paddingTop: 8,
        },
        tabBarLabelStyle: {
          fontFamily: "Outfit_500Medium",
          fontSize: 12,
        },
      }}
    >
      <Tabs.Screen
        name="index"
        options={{
          title: "Home",
          tabBarIcon: ({ color, size }) => (
            <Ionicons name="home-outline" size={size} color={color} />
          ),
        }}
      />
      <Tabs.Screen
        name="history"
        options={{
          title: "History",
          tabBarIcon: ({ color, size }) => (
            <Feather name="list" size={size} color={color} />
          ),
        }}
      />
      <Tabs.Screen
        name="settings"
        options={{
          title: "Settings",
          tabBarIcon: ({ color, size }) => (
            <Ionicons name="settings-outline" size={size} color={color} />
          ),
        }}
      />
      <Tabs.Screen
        name="success"
        options={{
          href: null,
        }}
      />
      <Tabs.Screen
        name="accept-payment"
        options={{
          href: null,
        }}
      />
      <Tabs.Screen
        name="qr-code"
        options={{
          href: null,
        }}
      />
      <Tabs.Screen
        name="waiting-payment"
        options={{
          href: null,
        }}
      />
      <Tabs.Screen
        name="contact-made"
        options={{
          href: null,
        }}
      />
      <Tabs.Screen
        name="payment-received"
        options={{
          href: null,
        }}
      />
      <Tabs.Screen
        name="withdraw-bank"
        options={{
          href: null,
        }}
      />
      <Tabs.Screen
        name="transfer-summary"
        options={{
          href: null,
        }}
      />
      <Tabs.Screen
        name="transfer-confirmation"
        options={{
          href: null,
        }}
      />
      <Tabs.Screen
        name="change-password"
        options={{
          href: null,
        }}
      />
      <Tabs.Screen
        name="bank-account"
        options={{
          href: null,
        }}
      />
    </Tabs>
  );
}
