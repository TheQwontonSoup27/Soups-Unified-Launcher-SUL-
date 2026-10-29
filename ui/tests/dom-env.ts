// Registers a browser-like global environment. Kept in its own module so that it has finished
// running before React DOM is imported (ES imports are hoisted, so a body that runs after an
// `import "react-dom"` in the same file would be too late: React DOM decides at load time whether it
// is in a browser and which input events it can rely on).
import { GlobalRegistrator } from "@happy-dom/global-registrator";
import "fake-indexeddb/auto";

GlobalRegistrator.register({ url: "http://localhost:5173/" });
(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
