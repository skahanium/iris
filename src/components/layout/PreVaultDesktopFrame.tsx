import type { ReactNode } from "react";

import { DesktopFrame } from "@/components/layout/DesktopFrame";
import { MinimalWindowChrome } from "@/components/layout/MinimalWindowChrome";

export function PreVaultDesktopFrame({ children }: { children: ReactNode }) {
  return (
    <DesktopFrame>
      <MinimalWindowChrome />
      {children}
    </DesktopFrame>
  );
}
