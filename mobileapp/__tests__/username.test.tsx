jest.mock("@react-native-async-storage/async-storage", () =>
  require("@react-native-async-storage/async-storage/jest/async-storage-mock")
);

import {
  validateUsernameFormat,
  MIN_USERNAME_LENGTH,
  MAX_USERNAME_LENGTH,
} from "../app/username";

describe("Username validation", () => {
  it("rejects empty username", () => {
    expect(validateUsernameFormat("").isValid).toBe(false);
  });

  it("rejects usernames shorter than MIN_USERNAME_LENGTH", () => {
    const res = validateUsernameFormat("ab");
    expect(res.isValid).toBe(false);
    expect(res.error).toBe(`Username must be at least ${MIN_USERNAME_LENGTH} characters`);
  });

  it("rejects usernames longer than MAX_USERNAME_LENGTH", () => {
    const longName = "a".repeat(MAX_USERNAME_LENGTH + 1);
    const res = validateUsernameFormat(longName);
    expect(res.isValid).toBe(false);
    expect(res.error).toBe(`Username must be at most ${MAX_USERNAME_LENGTH} characters`);
  });

  it("rejects invalid characters like spaces or special symbols", () => {
    const res1 = validateUsernameFormat("user name");
    expect(res1.isValid).toBe(false);
    expect(res1.error).toBe("Username can only contain letters, numbers, and underscores");

    const res2 = validateUsernameFormat("user@name!");
    expect(res2.isValid).toBe(false);
    expect(res2.error).toBe("Username can only contain letters, numbers, and underscores");
  });

  it("accepts valid alphanumeric usernames and underscores", () => {
    expect(validateUsernameFormat("zaps_user").isValid).toBe(true);
    expect(validateUsernameFormat("JohnDoe123").isValid).toBe(true);
    expect(validateUsernameFormat("user_99").isValid).toBe(true);
  });
});
