/** @type {import('@jest/types').Config.InitialOptions} */
module.exports = {
  preset: "jest-expo",
  setupFilesAfterEnv: ["<rootDir>/setup.js"],
  testEnvironment: "detox/runners/jest/testEnvironment",
  testMatch: ["<rootDir>/**/*.test.js"],
  testTimeout: 120000,
  maxWorkers: 1,
  globalSetup: "detox/runners/jest/globalSetup",
  globalTeardown: "detox/runners/jest/globalTeardown",
  reporters: ["detox/runners/jest/reporter"],
  testEnvironmentOptions: {
    eventListeners: ["detox/runners/jest/listeners/allure"],
  },
  globals: {
    beforeAll: true,
    beforeEach: true,
    describe: true,
    it: true,
    device: true,
  },
};
