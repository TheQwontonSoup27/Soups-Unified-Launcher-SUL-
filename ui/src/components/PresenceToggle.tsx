import { usePresence } from "../presence";

/**
 * Global Discord Rich Presence switch (spec: top-left global UI, off by default).
 * It calls the backend through `discord_set_enabled`; turning it off clears the activity.
 */
export function PresenceToggle() {
  const { on, toggle } = usePresence();
  return (
    <button
      type="button"
      role="switch"
      aria-checked={on}
      aria-label={`Discord presence: ${on ? "On" : "Off"}`}
      className={on ? "rpc-toggle on" : "rpc-toggle"}
      title="Show what you are doing in Discord. Off by default; games that are not supported never publish."
      onClick={() => void toggle()}
    >
      RPC
    </button>
  );
}

/** Why presence could not be switched on (Discord not running, no application id, ...). */
export function PresenceMessage() {
  const { message } = usePresence();
  return message ? <p className="presence-msg" role="status">{message}</p> : null;
}
