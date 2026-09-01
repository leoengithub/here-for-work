import * as React from "react"
import { Toast as ToastPrimitive } from "@base-ui/react/toast"
import { Cancel01Icon } from "@hugeicons/core-free-icons"
import { HugeiconsIcon } from "@hugeicons/react"

import { Button } from "@/components/ui/button"
import { cn } from "@/lib/utils"
import type { OutcomeNotification } from "@/types"

const toast = ToastPrimitive.createToastManager()

const OUTCOME_TOAST_TIMEOUT = 5_000

function dismissalToastId(roleId: string) {
  return `dismissal-${roleId}`
}

function createDismissalNoticeController(
  toastManager: ReturnType<typeof ToastPrimitive.createToastManager>,
) {
  return {
    show(roleId: string, roleTitle: string, onUndo: () => void) {
      toastManager.add({
        id: dismissalToastId(roleId),
        title: `${roleTitle} dismissed`,
        priority: "low",
        timeout: 30_000,
        actionProps: {
          children: "Undo",
          onClick: onUndo,
        },
      })
    },
    undoing(roleId: string, roleTitle: string) {
      toastManager.update(dismissalToastId(roleId), {
        title: `Restoring ${roleTitle}…`,
        description: undefined,
        type: "loading",
        timeout: 0,
        actionProps: {
          children: "Undo",
          disabled: true,
        },
      })
    },
    completed(roleId: string, roleTitle: string) {
      toastManager.update(dismissalToastId(roleId), {
        title: `${roleTitle} restored`,
        description: "Back in Queue.",
        type: "success",
        timeout: 1_500,
        actionProps: undefined,
      })
    },
    failed(roleId: string, roleTitle: string, detail: string, onRetry: () => void) {
      toastManager.update(dismissalToastId(roleId), {
        title: `Couldn’t restore ${roleTitle}`,
        description: detail,
        type: "error",
        timeout: 0,
        priority: "low",
        actionProps: {
          children: "Try again",
          onClick: onRetry,
        },
      })
    },
  }
}

function createOutcomeNoticeController(
  toastManager: ReturnType<typeof ToastPrimitive.createToastManager>,
) {
  return {
    show(
      notification: OutcomeNotification,
      onViewDetails: (preparationId: string) => void,
      onReviewForm: (sessionId: string) => void,
    ) {
      toastManager.add({
        id: [
          "outcome",
          notification.eventKind,
          notification.roleId,
          notification.preparationId,
          notification.browserSessionId ?? "",
          notification.title,
          notification.body,
        ].join("\u001f"),
        title: notification.title,
        description: notification.body,
        type: notification.eventKind === "preparation_failed" ? "error" : "success",
        priority: "high",
        timeout: OUTCOME_TOAST_TIMEOUT,
        data: { dismissible: true },
        actionProps: {
          children: notification.actionLabel,
          onClick: () => {
            if (notification.actionKind === "view_details") {
              onViewDetails(notification.preparationId)
            } else if (notification.browserSessionId) {
              onReviewForm(notification.browserSessionId)
            }
          },
        },
      })
    },
  }
}

function ToastProvider({ ...props }: ToastPrimitive.Provider.Props) {
  return <ToastPrimitive.Provider {...props} />
}

function ToastPortal({ ...props }: ToastPrimitive.Portal.Props) {
  return <ToastPrimitive.Portal data-slot="toast-portal" {...props} />
}

function ToastViewport({ className, ...props }: ToastPrimitive.Viewport.Props) {
  return (
    <ToastPrimitive.Viewport
      data-slot="toast-viewport"
      className={cn("toast-viewport", className)}
      {...props}
    />
  )
}

function Toast({ className, ...props }: ToastPrimitive.Root.Props) {
  return (
    <ToastPrimitive.Root
      data-slot="toast"
      className={cn("toast", className)}
      swipeDirection={[]}
      {...props}
    />
  )
}

function ToastContent({ className, ...props }: ToastPrimitive.Content.Props) {
  return (
    <ToastPrimitive.Content
      data-slot="toast-content"
      className={cn("toast__content", className)}
      {...props}
    />
  )
}

function ToastTitle({ className, ...props }: ToastPrimitive.Title.Props) {
  return (
    <ToastPrimitive.Title
      data-slot="toast-title"
      className={cn("toast__title", className)}
      {...props}
    />
  )
}

function ToastDescription({ className, ...props }: ToastPrimitive.Description.Props) {
  return (
    <ToastPrimitive.Description
      data-slot="toast-description"
      className={cn("toast__description", className)}
      {...props}
    />
  )
}

function ToastAction({
  className,
  render = <Button variant="outline" size="sm" />,
  ...props
}: ToastPrimitive.Action.Props) {
  return (
    <ToastPrimitive.Action
      data-slot="toast-action"
      render={render}
      className={cn("toast__action", className)}
      {...props}
    />
  )
}

function ToastClose({
  className,
  children,
  render = <Button variant="ghost" size="icon-sm" />,
  ...props
}: ToastPrimitive.Close.Props) {
  return (
    <ToastPrimitive.Close
      data-slot="toast-close"
      aria-label="Close notification"
      render={render}
      className={cn("toast__close", className)}
      {...props}
    >
      {children ?? <HugeiconsIcon icon={Cancel01Icon} strokeWidth={2} aria-hidden="true" />}
    </ToastPrimitive.Close>
  )
}

function ToastList() {
  const { toasts } = ToastPrimitive.useToastManager()

  return toasts.map((toastItem) => (
    <Toast key={toastItem.id} toast={toastItem}>
      <ToastContent>
        <div className="toast__message">
          <ToastTitle />
          <ToastDescription />
        </div>
        {toastItem.actionProps ? <ToastAction /> : null}
        {toastItem.data?.dismissible ? <ToastClose /> : null}
      </ToastContent>
    </Toast>
  ))
}

function Toaster({
  children,
  toastManager = toast,
  timeout = 30_000,
  limit = 3,
  ...props
}: ToastPrimitive.Provider.Props) {
  return (
    <ToastProvider toastManager={toastManager} timeout={timeout} limit={limit} {...props}>
      {children}
      <ToastPortal>
        <ToastViewport>
          <ToastList />
        </ToastViewport>
      </ToastPortal>
    </ToastProvider>
  )
}

const createToastManager = ToastPrimitive.createToastManager

export {
  Toaster,
  Toast,
  ToastAction,
  ToastClose,
  ToastContent,
  ToastDescription,
  ToastPortal,
  ToastProvider,
  ToastTitle,
  ToastViewport,
  createToastManager,
  createDismissalNoticeController,
  createOutcomeNoticeController,
  toast,
}
