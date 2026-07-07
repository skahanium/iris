export const loadManagementCenterPanel = () =>
  import("@/components/settings/ManagementCenterPanel").then((m) => ({
    default: m.ManagementCenterPanel,
  }));

export function preloadManagementCenter(): void {
  void loadManagementCenterPanel();
}
