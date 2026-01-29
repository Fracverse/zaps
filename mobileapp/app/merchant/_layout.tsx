import { Stack } from "expo-router";

export default function MerchantLayout() {
  return (
    <Stack screenOptions={{ headerShown: false }}>
      <Stack.Screen name="(tabs)" options={{ headerShown: false }} />
      <Stack.Screen name="accept-payment" options={{ headerShown: false }} />
      <Stack.Screen name="qr-code" options={{ headerShown: false }} />
      <Stack.Screen name="waiting-payment" options={{ headerShown: false }} />
      <Stack.Screen name="contact-made" options={{ headerShown: false }} />
      <Stack.Screen name="payment-received" options={{ headerShown: false }} />
      <Stack.Screen name="bank-account" options={{ headerShown: false }} />
      <Stack.Screen name="change-password" options={{ headerShown: false }} />
    </Stack>
  );
}
