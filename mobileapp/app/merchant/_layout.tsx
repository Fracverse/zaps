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
          borderTopWidth: 1,
          borderTopColor: "#F0F0F0",
          height: 60,
          paddingBottom: 10,
          paddingTop: 5,
          marginBottom: 20,
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
            <Feather
              name="bar-chart"
              size={size}
              color={color}
              style={{ transform: [{ rotate: "90deg" }] }}
            />
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
        name="change-password"
        options={{
          href: null, // Hide from tab bar
        }}
      />
      <Tabs.Screen
        name="bank-account"
        options={{
          href: null, // Hide from tab bar
        }}
      />
    </Tabs>
  );
}
