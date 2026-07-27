// Jest global setup

// Mock Privy SDK
jest.mock('@privy-io/expo', () => ({
  usePrivy: jest.fn(() => ({
    login: jest.fn(),
    logout: jest.fn(),
    user: null,
    isReady: true,
  })),
}), { virtual: true }); // virtual: true allows mocking modules not listed in package.json
