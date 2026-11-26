import { useEffect, useState } from "react";
import { createPortal } from "react-dom";
import { Icon, type IconName } from "../icons";
import { controlsText } from "../text/controls";
import { Button, IconButton } from "./Button";
import { layerRoot } from "./layers";
import { toastMs } from "./timing";
import { dismissToast, useToasts, type Toast, type ToastKind } from "./toastQueue";
import "./Toasts.css";

const icons: Record<ToastKind, IconName> = { info: "info", success: "check", error: "alert" };

/** How long a toast stays, or null until it is dismissed. */
function lifetime(toast: Toast): number | null {
  if (toast.kind === "error") return null;
  return toast.action === undefined ? toastMs.plain : toastMs.withAction;
}

/**
 * The toasts, at the bottom centre of the window, newest at the bottom.
 * Info and success are announced politely, errors at once. Toasts never
 * take focus. Mount once, at the top of the page.
 */
export function Toasts() {
  const toasts = useToasts((s) => s.toasts);
  const polite = toasts.filter((t) => t.kind !== "error");
  const urgent = toasts.filter((t) => t.kind === "error");
  return createPortal(
    <div className="ow-toasts">
      <div role="status" className="ow-toasts__region">
        {polite.map((t) => (
          <ToastItem key={t.id} toast={t} />
        ))}
      </div>
      <div role="alert" className="ow-toasts__region">
        {urgent.map((t) => (
          <ToastItem key={t.id} toast={t} />
        ))}
      </div>
    </div>,
    layerRoot(),
  );
}

function ToastItem({ toast }: { toast: Toast }) {
  const [held, setHeld] = useState(false);
  const stays = lifetime(toast);

  // Hovering or focusing a toast stops its timer; leaving starts it again.
  useEffect(() => {
    if (stays === null || held) return;
    const timer = window.setTimeout(() => dismissToast(toast.id), stays);
    return () => window.clearTimeout(timer);
  }, [stays, held, toast.id]);

  return (
    <div
      className={`ow-toast ow-toast--${toast.kind}`}
      // Two live regions share one column; order keeps the newest at the bottom.
      style={{ order: toast.id }}
      onPointerEnter={() => setHeld(true)}
      onPointerLeave={() => setHeld(false)}
      onFocus={() => setHeld(true)}
      onBlur={() => setHeld(false)}
    >
      <Icon name={icons[toast.kind]} className="ow-toast__icon" />
      <p className="ow-toast__message">{toast.message}</p>
      {toast.action !== undefined && (
        <Button
          size="sm"
          onClick={() => {
            toast.action?.onAction();
            dismissToast(toast.id);
          }}
        >
          {toast.action.label}
        </Button>
      )}
      <IconButton icon="x" label={controlsText.dismiss} onClick={() => dismissToast(toast.id)} />
    </div>
  );
}
