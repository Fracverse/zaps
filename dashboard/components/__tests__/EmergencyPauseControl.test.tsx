import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor, fireEvent } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import React from "react";
import EmergencyPauseControl from "../EmergencyPauseControl";

describe("EmergencyPauseControl (#1013)", () => {
  const mockToggle = vi.fn();

  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("renders prominent emergency pause button and active status badge", () => {
    render(
      <EmergencyPauseControl
        paused={false}
        onTogglePause={mockToggle}
        isSuperAdmin={true}
      />
    );

    expect(screen.getByTestId("emergency-pause-section")).toBeTruthy();
    expect(screen.getByTestId("vault-status-indicator").textContent).toContain("ACTIVE / OPERATIONAL");
    expect(screen.getByTestId("emergency-pause-toggle-btn").textContent).toContain("Emergency Freeze Vault");
  });

  it("opens confirmation drawer on click with accessible dialog roles and motion-safe styling", async () => {
    const user = userEvent.setup();
    render(
      <EmergencyPauseControl
        paused={false}
        onTogglePause={mockToggle}
        isSuperAdmin={true}
      />
    );

    const toggleBtn = screen.getByTestId("emergency-pause-toggle-btn");
    expect(toggleBtn.getAttribute("aria-haspopup")).toBe("dialog");
    expect(toggleBtn.getAttribute("aria-expanded")).toBe("false");

    await user.click(toggleBtn);

    const drawer = screen.getByTestId("pause-confirmation-drawer");
    expect(drawer).toBeTruthy();
    expect(drawer.getAttribute("role")).toBe("dialog");
    expect(drawer.getAttribute("aria-modal")).toBe("true");

    // Check motion-safe / reduced-motion accessibility classes to prevent motion sickness
    expect(drawer.className).toContain("motion-reduce:transition-none");
    expect(drawer.className).toContain("motion-safe:transition-opacity");
  });

  it("enforces double validation verification before enabling freeze execution", async () => {
    const user = userEvent.setup();
    render(
      <EmergencyPauseControl
        paused={false}
        onTogglePause={mockToggle}
        isSuperAdmin={true}
      />
    );

    await user.click(screen.getByTestId("emergency-pause-toggle-btn"));

    const confirmBtn = screen.getByTestId("drawer-final-confirm-btn");
    const ackCheckbox = screen.getByTestId("drawer-ack-checkbox");
    const confirmInput = screen.getByTestId("drawer-confirm-input");

    // Initially disabled
    expect(confirmBtn).toBeDisabled();

    // Step 1 only (checkbox) -> still disabled
    await user.click(ackCheckbox);
    expect(confirmBtn).toBeDisabled();

    // Step 2 with incorrect text -> still disabled
    await user.type(confirmInput, "WRONG");
    expect(confirmBtn).toBeDisabled();

    // Clear and enter exact required phrase "FREEZE"
    await user.clear(confirmInput);
    await user.type(confirmInput, "FREEZE");

    // Now double validation is satisfied -> button is enabled
    expect(confirmBtn).toBeEnabled();

    // Clicking final confirm triggers onTogglePause
    await user.click(confirmBtn);
    expect(mockToggle).toHaveBeenCalledTimes(1);
    expect(mockToggle).toHaveBeenCalledWith(true);
  });

  it("handles unpause / resume vault with RESUME double validation phrase", async () => {
    const user = userEvent.setup();
    render(
      <EmergencyPauseControl
        paused={true}
        onTogglePause={mockToggle}
        isSuperAdmin={true}
      />
    );

    expect(screen.getByTestId("vault-status-indicator").textContent).toContain("EMERGENCY FROZEN");
    const resumeBtn = screen.getByTestId("emergency-pause-toggle-btn");
    expect(resumeBtn.textContent).toContain("Resume Vault Operations");

    await user.click(resumeBtn);

    const confirmBtn = screen.getByTestId("drawer-final-confirm-btn");
    const ackCheckbox = screen.getByTestId("drawer-ack-checkbox");
    const confirmInput = screen.getByTestId("drawer-confirm-input");

    await user.click(ackCheckbox);
    await user.type(confirmInput, "RESUME");

    expect(confirmBtn).toBeEnabled();
    await user.click(confirmBtn);

    expect(mockToggle).toHaveBeenCalledTimes(1);
    expect(mockToggle).toHaveBeenCalledWith(false);
  });

  it("closes drawer when Escape key is pressed (accessible keyboard behavior)", async () => {
    const user = userEvent.setup();
    render(
      <EmergencyPauseControl
        paused={false}
        onTogglePause={mockToggle}
        isSuperAdmin={true}
      />
    );

    await user.click(screen.getByTestId("emergency-pause-toggle-btn"));
    expect(screen.getByTestId("pause-confirmation-drawer")).toBeTruthy();

    fireEvent.keyDown(window, { key: "Escape" });

    await waitFor(() => {
      expect(screen.queryByTestId("pause-confirmation-drawer")).toBeNull();
    });
  });

  it("disables trigger button when user is not superadmin", () => {
    render(
      <EmergencyPauseControl
        paused={false}
        onTogglePause={mockToggle}
        isSuperAdmin={false}
      />
    );

    const toggleBtn = screen.getByTestId("emergency-pause-toggle-btn");
    expect(toggleBtn).toBeDisabled();
  });
});
