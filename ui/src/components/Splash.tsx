import { useEffect } from "react";

export function Splash({ ms, onDone }: { ms: number; onDone: () => void }) {
  useEffect(() => {
    const reduce = typeof window !== "undefined" && window.matchMedia?.("(prefers-reduced-motion: reduce)").matches;
    const t = setTimeout(onDone, reduce ? Math.min(ms, 600) : ms);
    return () => clearTimeout(t);
  }, [ms, onDone]);
  return (
    <div className="splash" role="img" aria-label="Soups Unified Launcher, Team27" onClick={onDone}>
      <img src="logo.png" alt="" />
    </div>
  );
}
