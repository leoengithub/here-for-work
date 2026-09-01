import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react"
import { useState } from "react"
import { afterEach, describe, expect, it, vi } from "vitest"

import {
  Toaster,
  createDismissalNoticeController,
  createToastManager,
} from "./toast"

afterEach(() => {
  cleanup()
  vi.useRealTimers()
})

function activeToastRoots() {
  return document.querySelectorAll('[data-slot="toast"]:not([data-limited])')
}

describe("dismissal notifications", () => {
  it("expires each notice after 30 seconds", () => {
    vi.useFakeTimers()
    const manager = createToastManager()
    const notices = createDismissalNoticeController(manager)
    render(<Toaster toastManager={manager} />)

    act(() => notices.show("role-1", "Frontend Engineer", () => undefined))
    const toastRoot = screen.getByText("Frontend Engineer dismissed").closest('[data-slot="toast"]')

    act(() => vi.advanceTimersByTime(29_999))
    expect(toastRoot).not.toHaveAttribute("data-ending-style")

    act(() => vi.advanceTimersByTime(1))
    expect(toastRoot).toHaveAttribute("data-ending-style")
  })

  it("pauses the timeout while hovered or keyboard focused", () => {
    vi.useFakeTimers()
    const manager = createToastManager()
    const notices = createDismissalNoticeController(manager)
    render(<Toaster toastManager={manager} />)

    act(() => notices.show("hovered", "Hover role", () => undefined))
    const viewport = screen.getByRole("region", { name: "Notifications" })
    const hoverToast = screen.getByText("Hover role dismissed").closest('[data-slot="toast"]')

    fireEvent.mouseEnter(viewport)
    act(() => vi.advanceTimersByTime(30_000))
    expect(hoverToast).not.toHaveAttribute("data-ending-style")
    fireEvent.mouseLeave(viewport)
    act(() => vi.advanceTimersByTime(30_000))
    expect(hoverToast).toHaveAttribute("data-ending-style")

    act(() => notices.show("focused", "Focused role", () => undefined))
    const focusedToast = screen.getByText("Focused role dismissed").closest('[data-slot="toast"]')
    fireEvent.keyDown(window, { key: "F6" })
    expect(viewport).toHaveFocus()
    act(() => vi.advanceTimersByTime(30_000))
    expect(focusedToast).not.toHaveAttribute("data-ending-style")
    fireEvent.blur(viewport, { relatedTarget: document.body })
    act(() => vi.advanceTimersByTime(30_000))
    expect(focusedToast).toHaveAttribute("data-ending-style")
  })

  it("shows no more than three notices and keeps Undo independent", () => {
    vi.useFakeTimers()
    const manager = createToastManager()
    const notices = createDismissalNoticeController(manager)
    const undo = [vi.fn(), vi.fn(), vi.fn(), vi.fn()]
    render(<Toaster toastManager={manager} />)

    act(() => {
      undo.forEach((handler, index) => notices.show(`role-${index}`, `Role ${index}`, handler))
    })

    expect(activeToastRoots()).toHaveLength(3)
    expect(document.querySelectorAll('[data-slot="toast"][data-limited]')).toHaveLength(1)

    const roleTwoToast = screen.getByText("Role 2 dismissed").closest('[data-slot="toast"]')
    expect(roleTwoToast).not.toBeNull()
    fireEvent.click(within(roleTwoToast as HTMLElement).getByRole("button", { name: "Undo" }))
    expect(undo[2]).toHaveBeenCalledOnce()
    expect(undo[0]).not.toHaveBeenCalled()
    expect(undo[1]).not.toHaveBeenCalled()
    expect(undo[3]).not.toHaveBeenCalled()

    act(() => notices.completed("role-2", "Role 2"))
    expect(within(roleTwoToast as HTMLElement).queryByRole("button", { name: "Undo" })).not.toBeInTheDocument()
    expect(screen.getByText("Role 3 dismissed")).toBeInTheDocument()
  })

  it("keeps a failed Undo actionable and supports retry", () => {
    const manager = createToastManager()
    const notices = createDismissalNoticeController(manager)
    const retry = vi.fn()
    render(<Toaster toastManager={manager} />)

    act(() => notices.show("role-1", "Frontend Engineer", () => undefined))
    act(() => notices.undoing("role-1", "Frontend Engineer"))

    expect(screen.getByText("Restoring Frontend Engineer…")).toBeInTheDocument()
    expect(screen.getByRole("button", { name: "Undo" })).toBeDisabled()

    act(() => notices.failed("role-1", "Frontend Engineer", "Career-ops was unavailable.", retry))
    expect(screen.getByText("Couldn’t restore Frontend Engineer")).toBeInTheDocument()
    expect(screen.getByText("Career-ops was unavailable.")).toBeInTheDocument()
    fireEvent.click(screen.getByRole("button", { name: "Try again" }))
    expect(retry).toHaveBeenCalledOnce()
  })

  it("persists across primary routes but resets with a new app session", () => {
    vi.useFakeTimers()
    const manager = createToastManager()
    const notices = createDismissalNoticeController(manager)

    function RouteHarness() {
      const [route, setRoute] = useState<"Queue" | "Applications">("Queue")
      return (
        <>
          <button type="button" onClick={() => setRoute("Applications")}>Applications</button>
          <main>{route}</main>
          <Toaster toastManager={manager} />
        </>
      )
    }

    const session = render(<RouteHarness />)
    act(() => notices.show("role-1", "Frontend Engineer", () => undefined))
    fireEvent.click(screen.getByRole("button", { name: "Applications" }))
    expect(screen.getByText("Frontend Engineer dismissed")).toBeInTheDocument()

    session.unmount()
    render(<RouteHarness />)
    expect(screen.queryByText("Frontend Engineer dismissed")).not.toBeInTheDocument()
  })
})
