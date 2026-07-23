import React, { useEffect } from "react";
import ReactDOM from "react-dom/client";
import { FluentProvider } from "@sawcy/memora-ui";

// The Memora stylesheet is imported exactly once, here. Never in a component.
import "@sawcy/memora-ui/styles.css";
import "./styles/app.css";

import App from "./App";
import { AppProvider, useApp } from "./state/AppContext";

/**
 * Applies the user's theme choice and the Windows accent color to the
 * provider.
 *
 * `AppProvider` sits outside `FluentProvider` so the stored theme is available
 * before the provider mounts; the pages that use portalled surfaces
 * (dialogs, menus) all render inside it.
 */
function ThemedApp() {
  const { settings, accentColor } = useApp();
  const theme = settings?.theme ?? "system";

  // The opaque window base in app.css keys off `data-theme`, so it has to
  // track the same choice the provider is given.
  useEffect(() => {
    if (theme === "system") {
      delete document.documentElement.dataset.theme;
    } else {
      document.documentElement.dataset.theme = theme;
    }
  }, [theme]);

  return (
    <FluentProvider
      theme={theme}
      density="compact"
      accentColor={accentColor ?? undefined}
    >
      <App />
    </FluentProvider>
  );
}

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <AppProvider>
      <ThemedApp />
    </AppProvider>
  </React.StrictMode>,
);
