import { createContext, useCallback, useContext, useEffect, useMemo, useState, type ReactNode } from "react";
import {
  getInvoker, loadPresenceWanted, loadSmallImage, normalizeAssetKey, savePresenceWanted, saveSmallImage, setPresence,
} from "./discord";

export interface PresenceApi {
  on: boolean;
  message: string;
  /** Asset key of the small corner image. Empty means none. */
  smallImage: string;
  toggle: () => Promise<void>;
  /** Saves the asset key and, if presence is on, publishes again so it takes effect. */
  setSmallImage: (key: string) => Promise<void>;
}

const DEFAULT: PresenceApi = { on: false, message: "", smallImage: "", toggle: async () => {}, setSmallImage: async () => {} };
const Ctx = createContext<PresenceApi>(DEFAULT);
export const usePresence = () => useContext(Ctx);

/** Owns the global Discord Rich Presence switch (spec: off by default, opt-in, clears when turned off). */
export function PresenceProvider({ children }: { children: ReactNode }) {
  const [on, setOn] = useState(false);
  const [message, setMessage] = useState("");
  const [smallImage, setSmallImageState] = useState(loadSmallImage);

  useEffect(() => {
    if (!loadPresenceWanted()) return;
    void setPresence(true, getInvoker(), loadSmallImage()).then((r) => {
      setOn(r.on);
      setMessage(r.message);
      if (!r.on) savePresenceWanted(false);
    });
  }, []);

  const toggle = useCallback(async () => {
    const r = await setPresence(!on, getInvoker(), smallImage);
    setOn(r.on);
    setMessage(r.message);
    savePresenceWanted(r.on);
  }, [on, smallImage]);

  const setSmallImage = useCallback(async (raw: string) => {
    const key = normalizeAssetKey(raw);
    saveSmallImage(key);
    setSmallImageState(key);
    if (!on) return;
    const r = await setPresence(true, getInvoker(), key);
    setOn(r.on);
    setMessage(r.message);
    if (!r.on) savePresenceWanted(false);
  }, [on]);

  const api = useMemo<PresenceApi>(() => ({ on, message, smallImage, toggle, setSmallImage }), [on, message, smallImage, toggle, setSmallImage]);
  return <Ctx.Provider value={api}>{children}</Ctx.Provider>;
}
