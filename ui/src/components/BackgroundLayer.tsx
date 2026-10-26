import type { ReactNode } from "react";
import { isVideoType } from "../background";
import { useSettings } from "../settings";

export function BackgroundLayer() {
  const { bg, wallpapers, custom } = useSettings();
  let media: ReactNode = null;
  if (bg.mode === "custom" && custom) {
    media = isVideoType(custom.type)
      ? <video src={custom.url} autoPlay loop muted playsInline />
      : <img src={custom.url} alt="" />;
  } else if (bg.mode === "gallery") {
    const w = wallpapers.find((x) => x.id === bg.galleryId);
    if (w) media = <img src={w.file} alt="" />;
  }
  return (
    <div className="bg-layer" aria-hidden>
      {media}
      {media && <div className="bg-dim" style={{ opacity: bg.overlay }} />}
    </div>
  );
}
