import type { ReactNode } from "react";
import { Icon, type IconName } from "../icons";
import { controlsText } from "../text/controls";
import { IconButton } from "./Button";
import "./Banner.css";

export type BannerTone = "info" | "ok" | "warn" | "error";

const defaultIcons: Record<BannerTone, IconName> = { info: "info", ok: "check", warn: "warning", error: "alert" };

interface BannerProps {
  tone: BannerTone;
  /** The 20 px icon; each tone has a default. */
  icon?: IconName;
  /** One sentence. */
  children: ReactNode;
  /** Small buttons at the right. */
  actions?: ReactNode;
  /** When given, the banner has an x at its right end. */
  onDismiss?: () => void;
}

/**
 * A full-width strip between the bars and the columns. A status, or an
 * alert when it reports an error. docs/09-visual-design.md, "Banners".
 */
export function Banner({ tone, icon, children, actions, onDismiss }: BannerProps) {
  return (
    <div role={tone === "error" ? "alert" : "status"} className={`ow-banner ow-banner--${tone}`}>
      <Icon name={icon ?? defaultIcons[tone]} size="lg" className="ow-banner__icon" />
      <p className="ow-banner__text">{children}</p>
      {actions !== undefined && <div className="ow-banner__actions">{actions}</div>}
      {onDismiss !== undefined && <IconButton icon="x" label={controlsText.dismiss} onClick={onDismiss} />}
    </div>
  );
}
