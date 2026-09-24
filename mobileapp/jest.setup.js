// Jest global setup

jest.mock('@react-native-async-storage/async-storage', () =>
  require('@react-native-async-storage/async-storage/jest/async-storage-mock')
);

// Mock Privy SDK
jest.mock('@privy-io/expo', () => ({
  usePrivy: jest.fn(() => ({
    login: jest.fn(),
    logout: jest.fn(),
    user: null,
    isReady: true,
  })),
}), { virtual: true }); // virtual: true allows mocking modules not listed in package.json
