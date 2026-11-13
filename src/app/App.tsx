import { AppProviders } from "./providers/AppProviders";
import { Shell } from "./shell/Shell";

export function App() {
  return (
    <AppProviders>
      <Shell />
    </AppProviders>
  );
}
